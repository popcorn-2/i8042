#![deny(warnings)]

use proto::client::BusNodeTr;
use std::os::popcorn::handle::BorrowedHandle;
use log::{debug, error, info, warn, Level};
use crate::controller::Port;

mod controller;
mod keyboard;
mod mouse;

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

	let controller = controller::Controller::new(COMMAND_PORT, DATA_PORT)?;
	let (port1, port2) = controller.get_ports();

	if let Some(port1) = port1 {
		match init_port(port1) {
			Ok(device) => {
				let _handle = handle.create_child("0");
				if let Device::Keyboard(kbd) = device {
					kbd.main_loop();
				}
			},
			Err(e) => warn!("port1 failed to init ({e:?})"),
		}
	}

	if let Some(port2) = port2 {
		match init_port(port2) {
			Ok(_) => {
				let _handle = handle.create_child("1");
			},
			Err(e) => warn!("port1 failed to init ({e:?})"),
		}
	}

	Ok(())
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

fn init_port(port: Port) -> Result<Device<'_>, PortInitError> {
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
