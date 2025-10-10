use std::arch::asm;
use std::cell::RefCell;
use std::fmt::Debug;
use log::{debug, error, info, trace, warn};
use crate::newtype_enum;

#[repr(u8)]
enum Command {
	ReadConfigByte = 0x20,
	WriteConfigByte = 0x60,
	DisablePortTwo = 0xA7,
	EnablePortTwo = 0xA8,
	TestPortTwo = 0xA9,
	TestController = 0xAA,
	TestPortOne = 0xAB,
	DisablePortOne = 0xAD,
	EnablePortOne = 0xAE,
	WritePortTwo = 0xD4,
}

newtype_enum! {
	pub enum PortTestResult: u8 => {
		OK = 0,
		CLK_LOW = 1,
		CLK_HIGH = 2,
		DATA_LOW = 3,
		DATA_HIGH = 4,
	}
}

bitflags::bitflags! {
	#[derive(Debug, Copy, Clone)]
	struct Config: u8 {
		const IRQ1_ENABLE = 1 << 0;
		const IRQ2_ENABLE = 1 << 1;
		const POST_SUCCESSFUL = 1 << 2;
		const CLK1_DISABLE = 1 << 4;
		const CLK2_DISABLE = 1 << 5;
		const TRANSLATION_ENABLE = 1 << 6;
	}

	#[derive(Debug, Copy, Clone)]
	struct Status: u8 {
		const READ_READY = 1 << 0;
		const WRITE_FULL = 1 << 1;
		const TIMEOUT = 1 << 6;
		const PARITY_ERROR = 1 << 7;
	}
}

fn inb(port: u16) -> u8 {
	let value: u8;
	unsafe {
		asm!("in al, dx", out("al") value, in("dx") port, options(nomem, nostack));
	}
	value
}

fn outb(port: u16, value: u8) {
	unsafe {
		asm!("out dx, al", in("al") value, in("dx") port, options(nomem, nostack));
	}
}

#[derive(Debug)]
pub enum Error {
	SelfTestFail,
	NoPorts,
}

pub struct Port<'c> {
	is_second: bool,
	controller: &'c Controller,
}

impl Debug for Port<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Port")
			.field("is_second", &self.is_second)
			.finish()
	}
}

impl Port<'_> {
	pub fn transaction(&self, command: &[u8], response_count: usize) -> Result<Vec<u8>, ()> {
		let mut buf = Vec::with_capacity(response_count);

		let _guard = self.controller.lock.borrow_mut();

		for _ in 0..3 {
			for command in command {
				if self.is_second {
					outb(self.controller.command, Command::WritePortTwo as u8);
				}

				loop {
					let status = Status::from_bits_retain(inb(self.controller.command));
					if !status.contains(Status::WRITE_FULL) { break; }
				}

				outb(self.controller.data, *command);
			}

			loop {
				let status = Status::from_bits_retain(inb(self.controller.command));
				if status.contains(Status::READ_READY) { break; }
			}

			let response = inb(self.controller.data);
			trace!("PS/2 response = {response:#x}");
			match response {
				0xFA => break,
				0xFE => warn!("resend requested"),
				response => {
					error!("unknown ps/2 device response {response:#x}");
					return Err(());
				}
			}
		}

		while buf.len() < response_count {
			let Ok(packet) = self.read_inner() else { break; };
			buf.push(packet);
		}

		Ok(buf)
	}

	fn read_inner(&self) -> Result<u8, ()> {
		let status = Status::from_bits_retain(inb(self.controller.command));
		if !status.contains(Status::READ_READY) {
			std::thread::yield_now(); // hacky
			let status = Status::from_bits_retain(inb(self.controller.command));
			if !status.contains(Status::READ_READY) {
				//warn!("timeout waiting for response byte");
				return Err(());
			}
		}
		let packet = inb(self.controller.data);
		trace!("PS/2 data = {packet:#x}");
		Ok(packet)
	}

	pub fn read(&self) -> Result<u8, ()> {
		let _guard = self.controller.lock.borrow_mut();
		self.read_inner()
	}
}

pub struct Controller {
	port1: bool,
	port2: bool,
	lock: RefCell<()>,
	command: u16,
	data: u16,
}

impl Controller {
	pub fn new(command_port: u16, data_port: u16) -> Result<Self, Error> {
		debug!("initialising controller");

		outb(command_port, Command::DisablePortOne as u8);
		outb(command_port, Command::DisablePortTwo as u8);
		let _ = inb(data_port);

		outb(command_port, Command::ReadConfigByte as u8);
		let mut config = Config::from_bits_retain(dbg!(inb(data_port)));
		debug!("config = {:#?}", config);
		config &= !(Config::IRQ1_ENABLE | Config::IRQ2_ENABLE | Config::TRANSLATION_ENABLE);
		outb(command_port, Command::WriteConfigByte as u8);
		outb(data_port, dbg!(config.bits()));

		outb(command_port, Command::TestController as u8);
		let result = inb(data_port);
		if result != 0x55 {
			error!("controller self test failed with {result:#x}");
			return Err(Error::SelfTestFail);
		}

		outb(command_port, Command::WriteConfigByte as u8);
		outb(data_port, config.bits());

		debug!("controller initialized - testing ports");

		let mut port1 = true;
		let mut port2 = true;
		outb(command_port, Command::EnablePortTwo as u8);
		outb(command_port, Command::ReadConfigByte as u8);
		let config = Config::from_bits_retain(inb(data_port));
		if config.contains(Config::CLK2_DISABLE) {
			info!("only one PS/2 port");
			port2 &= false;
		}
		outb(command_port, Command::DisablePortTwo as u8);
		outb(command_port, Command::WriteConfigByte as u8); // make sure IRQs are disabled
		outb(data_port, config.bits());

		outb(command_port, Command::TestPortOne as u8);
		let res = PortTestResult(inb(data_port));
		if res != PortTestResult::OK {
			warn!("port 1 failed with {res:x?}");
			port1 &= false;
		}

		if port2 {
			outb(command_port, Command::TestPortTwo as u8);
			let res = PortTestResult(inb(data_port));
			if res != PortTestResult::OK {
				warn!("port 2 failed with {res:x?}");
				port2 &= false;
			}
		}

		if !(port1 || port2) {
			warn!("no functional PS/2 ports");
			return Err(Error::NoPorts);
		}

		if port1 {
			outb(command_port, Command::EnablePortOne as u8);
			debug!("port 1 configured");
		}

		if port2 {
			outb(command_port, Command::EnablePortTwo as u8);
			debug!("port 2 configured");
		}

		Ok(Self {
			port1,
			port2,
			lock: RefCell::new(()),
			command: command_port,
			data: data_port,
		})
	}

	pub fn get_ports(&self) -> (Option<Port<'_>>, Option<Port<'_>>) {
		(
			self.port1.then(|| Port { is_second: false, controller: self }),
			self.port2.then(|| Port { is_second: true, controller: self }),
		)
	}
}
