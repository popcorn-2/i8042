#![allow(unused)]

pub mod server {
	include!(concat!(env!("OUT_DIR"), "/protocol.gen.rs"));
}

pub mod client {
	protocol! {
        pub protocol HidKeyboard = 0x1003 {
            ctor => {}
        }
    }
}
