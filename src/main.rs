#![feature(popcorn_protocol)]
#![feature(macro_metavar_expr_concat)]

#![deny(warnings)]

use proto::client::BusNodeTr;
use std::os::popcorn::handle::{AsRawHandle, BorrowedHandle};
use std::sync::Arc;
use std::thread;
use log::{debug, error, info, warn, Level};
use crate::controller::Port;

extern crate alloc;

#[macro_use]
mod macros;
mod controller;
mod keyboard;
mod mouse;
mod server;

#[macro_export]
macro_rules! newtype_enum {
    (
        $(#[$type_attrs:meta])*
        $visibility:vis enum $type:ident : $base_integer:ty => $(#[$impl_attrs:meta])* {
            $(
                $(#[$variant_attrs:meta])*
                $variant:ident = $value:expr,
            )*
        }
    ) => {
        $(#[$type_attrs])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Eq, PartialEq)]
        $visibility struct $type(pub $base_integer);

        $(#[$impl_attrs])*
        #[allow(unused)]
        impl $type {
            $(
                $(#[$variant_attrs])*
                pub const $variant: $type = $type($value);
            )*
        }

        #[allow(unused)]
        impl core::fmt::Debug for $type {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                match *self {
                    // Display variants by their name, like Rust enums do
                    $(
                        $type::$variant => write!(f, stringify!($variant)),
                    )*

                    // Display unknown variants in tuple struct format
                    $type(unknown) => {
                        write!(f, "{}({})", stringify!($type), unknown)
                    }
                }
            }
        }
    }
}

const DATA_PORT: u16 = 0x60;
const COMMAND_PORT: u16 = 0x64;

fn main() -> Result<(), controller::Error> {
    let Some(bus_handle) = std::os::popcorn::env::get_handle::<proto::client::BusNode>("driver.node") else {
        panic!("`{}` could not find parent bus. Perhaps it wasn't launched through Device Manager?", env!("CARGO_PKG_NAME"));
    };

    driver_main(bus_handle)
}

fn driver_main(handle: BorrowedHandle<'_, proto::client::BusNode>) -> Result<(), controller::Error> {
	println!("started PS/2 driver");
	simple_logger::init_with_level(Level::Debug).unwrap();

	let controller = Box::leak(Box::new(controller::Controller::new(COMMAND_PORT, DATA_PORT)?));
	let (port1, _port2) = controller.get_ports();

	let (kbd_send, kbd_recv) = async_channel::bounded(32);

	let mut srv = server::ServerHandler::new()
			.expect("unable to start server");

	if let Some(port1) = port1 {
		match init_port(port1) {
			Ok(device) => {
				let device_handle = handle.create_child("0")
						.expect("unable to create device");
				if let Device::Keyboard(kbd) = device {
					let keyboard_handle = srv.inner_mut()
							.add_keyboard(kbd_recv)
							.expect("unable to add keyboard");
					let keyboard_handle = core::mem::ManuallyDrop::new(keyboard_handle);
					device_handle.attach_device(keyboard_handle.as_raw_handle().0)
							.expect("unable to add keyboard handle");
					thread::Builder::new()
							.name("i8042-kbd".to_owned())
							.spawn(move || kbd.main_loop(kbd_send))
							.expect("failed to spawn keyboard thread");
				}
			},
			Err(e) => warn!("port1 failed to init ({e:?})"),
		}
	}

	/*
	HACK: once mutex fixed, uncomment this - right now fails since it tries to lock the mutex at the same time as kbd thread

	if let Some(port2) = port2 {
		match init_port(port2) {
			Ok(_) => {
				let _handle = handle.create_child("1");
			},
			Err(e) => warn!("port1 failed to init ({e:?})"),
		}
	}
	 */

	Arc::new(srv).run()
}

#[derive(Debug)]
enum PortInitError {
	NoDevice,
	SelfTestFailed,
	DeviceInitFailed,
}

enum Device<'p> {
	Keyboard(keyboard::Keyboard<'p>),
	Mouse(mouse::Mouse),
}

fn init_port(port: Port<'_>) -> Result<Device<'_>, PortInitError> {
	const RESET: u8 = 0xFF;
	const DISABLE: u8 = 0xF5;
	const IDENTIFY: u8 = 0xF2;

	// use response_count = 2 because mice will reply with ID after the 0xAA byte
	let response = port.transaction(&[RESET], 2).map_err(|_| PortInitError::NoDevice)?;
	if response[0] != 0xAA {
		error!("keyboard failed self with error code {}", response[0]);
		return Err(PortInitError::SelfTestFailed);
	}

	debug!("disable scanning and identify device for {port:?}");
	port.transaction(&[DISABLE], 0).map_err(|_| PortInitError::NoDevice)?;
	let id = port.transaction(&[IDENTIFY], 2).map_err(|_| PortInitError::NoDevice)?;
	debug!("found device with id {id:#x?}");

	match &*id {
		[0x00] => {
			info!("found mouse");
			Ok(Device::Mouse(mouse::Mouse::new(port, false, false)))
		}
		[0x03] => {
			info!("found mouse with scrollwheel");
			Ok(Device::Mouse(mouse::Mouse::new(port, true, false)))
		}
		[0x04] => {
			info!("found 5 button mouse");
			Ok(Device::Mouse(mouse::Mouse::new(port, true, true)))
		}
		kbd => {
			info!("found keyboard {kbd:x?}");
			Ok(Device::Keyboard(keyboard::Keyboard::new(port).map_err(|_| PortInitError::DeviceInitFailed)?))
		}
	}
}
