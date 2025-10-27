use log::{error, info};
use pc_keyboard::{KeyEvent, KeyState, ScancodeSet, ScancodeSet2};
use crate::controller::Port;

pub struct Keyboard<'p> {
	port: Port<'p>,
}

impl Keyboard<'_> {
	pub fn new(port: Port<'_>) -> Result<Keyboard<'_>, ()> {
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

	pub fn main_loop(&self, channel: async_channel::Sender<u16>) -> ! {
		let mut scancodes = ScancodeSet2::new();
		loop {
			if let Ok(packet) = self.port.read() {
				match scancodes.advance_state(packet) {
					Ok(Some(KeyEvent { code, state })) => {
						dbg!(code, state);
						let base = code as u16;
						match state {
							KeyState::Up => { let _ = channel.send_blocking(base | (1 << 15)); },
							KeyState::Down => { let _ = channel.send_blocking(base); },
							KeyState::SingleShot => {
								let _ = channel.send_blocking(base);
								let _ = channel.send_blocking(base | (1 << 15));
							},
						}
					}
					Ok(None) => { /* multi-byte scancode */ },
					Err(e) => error!("decode error {e:?}"),
				}
			}
		}
	}
}
