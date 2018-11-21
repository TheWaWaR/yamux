
use bytes::{BytesMut, Bytes};
use futures::{Async, Poll};

pub type StreamId = u32;

pub struct Stream {
    recv_window: u32,
    send_window: u32,
    id: StreamId,
    state: StreamState,
    recv_buf: BytesMut,
}

impl futures::Stream for Stream {
    type Item = Bytes;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(Async::NotReady)
    }
}

pub struct StreamHandle {
}

impl StreamHandle {
    pub fn new() -> StreamHandle {
        StreamHandle {}
    }

    pub fn write(&mut self, data: Bytes) {}
}

pub enum StreamState {
    Init,
    SynSent,
    SynReceived,
    Established,
    LocalClosing,
    RemoteClosing,
    Closed,
    Reset,
}
