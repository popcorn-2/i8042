use crate::controller::Port;

pub struct Mouse {}

impl Mouse {
	pub fn new(_port: Port, _scrollwheel: bool, _button: bool) -> Mouse {
		Mouse {}
	}
}
