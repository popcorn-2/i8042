use log::{debug, error, info};
use pc_keyboard::{EventDecoder, HandleControl, ScancodeSet, ScancodeSet2};
use pc_keyboard::layouts::Uk105Key;
use crate::controller::Port;

pub struct Keyboard<'p> {
	port: Port<'p>,
}

impl Keyboard<'_> {
	pub fn new(port: Port) -> Result<Keyboard<'_>, ()> {
		// try and set scancode set 2
		port.transaction(&[0xF0, 2], 0)?;
		let scancode_set = port.transaction(&[0xF0, 0], 1)?;
		if scancode_set[0] != 2 {
			error!("scancode 2 not supported");
			return Err(());
		}

		// enable scanning
		port.transaction(&[0xF4], 0)?;

		info!("keyboard at {port:?} successfully configured");

		Ok(Keyboard { port })
	}

	pub fn main_loop(&self) {
		let mut scancodes = ScancodeSet2::new();
		let mut layout = EventDecoder::new(Uk105Key, HandleControl::Ignore);
		loop {
			if let Ok(packet) = self.port.read() {
				match scancodes.advance_state(packet) {
					Ok(Some(key)) => {
						let decoded = layout.process_keyevent(key.clone());
						debug!("{key:?} = {decoded:?}");
					}
					Ok(None) => { /* multi-byte scancode */ },
					Err(e) => error!("decode error {e:?}"),
				}
			}
		}
	}
}
