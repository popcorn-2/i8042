use std::io;
use std::os::popcorn::handle::{AsHandle, OwnedHandle};
use std::os::popcorn::proto::Error;
use std::path::Path;
use std::sync::OnceLock;
use executor::io::popcorn::AsyncOwnedHandle;
use popcorn_server::{DispatchTable, ProtocolVisitor, ReturnHandle, Server, SyncTr as _};
use crate::server::proto::server::HidKeyboard;

mod proto;

pub struct ServerHandler {
	handle: AsyncOwnedHandle<popcorn_server::Sync>,
	keyboard_channel: OnceLock<async_channel::Receiver<u16>>,
}

impl ServerHandler {
	pub fn new() -> io::Result<Server<ServerHandler>> {
		let handle = Server::new(
			":",
			|handle| ServerHandler {
				handle,
				keyboard_channel: OnceLock::new(),
			},
		);

		handle
	}

	pub fn add_keyboard(&mut self, queue: async_channel::Receiver<u16>) -> io::Result<OwnedHandle<proto::client::HidKeyboard>> {
		if self.keyboard_channel.set(queue).is_err() {
			return Err(io::Error::new(
				io::ErrorKind::AlreadyExists,
				"keyboard already created",
			))
		}
		self.handle.as_handle().forge::<proto::client::HidKeyboard>(1)
	}
}

#[derive(Default)]
pub struct CtorCtx;

impl popcorn_server::CtorContext for CtorCtx {
	fn visitors(&self) -> &'static ProtocolVisitor<Self> {
		static VISITORS: OnceLock<ProtocolVisitor<CtorCtx>> = OnceLock::new();

		VISITORS.get_or_init(ProtocolVisitor::new)
	}
}

impl popcorn_server::ServerHandler for ServerHandler {
	type CtorContext = CtorCtx;

	async fn ctor(&self, _endpoint: &Path, _ctx: Self::CtorContext) -> Result<ReturnHandle, Error> {
		Err(Error::UnsupportedProtocol)
	}

	async fn destroy(&self, _handle: isize) -> Result<(), Error> {
		Err(Error::UnsupportedProtocol)
	}

	fn dispatch_table(&self) -> &'static DispatchTable {
		static DISPATCH: OnceLock<DispatchTable> = OnceLock::new();

		DISPATCH.get_or_init(|| DispatchTable::new()
				.add_vtable(<Self as HidKeyboard>::__vtable())
		)
	}

	fn handle(&self) -> &AsyncOwnedHandle<popcorn_server::Sync> {
		&self.handle
	}
}

impl HidKeyboard for ServerHandler {
	async fn get_scancode(&self, handle: isize) -> Result<usize, Error> {
		if handle != 1 { return Err(Error::InvalidHandle); }
		let Some(channel) = self.keyboard_channel.get() else {
			return Err(Error::InvalidHandle);
		};

		channel.recv()
				.await
				.map(|v| v as usize)
				.map_err(|_| Error::DeadServer)
	}

	async fn new_from(&self, _endpoint: &Path, _handle: OwnedHandle<()>) -> Result<ReturnHandle, Error> {
		Err(Error::UnsupportedProtocol)
	}
}
