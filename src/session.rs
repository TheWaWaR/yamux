
use crate::{
    config::Config,
    stream::{StreamId, Stream, StreamHandle},
};

use fnv::FnvHashMap;
// use channel::select;
use futures::{Async, Poll};
use tokio_io::{AsyncRead, AsyncWrite};


pub struct Session<T> {
	  // remoteGoAway indicates the remote side does
	  // not want futher connections. Must be first for alignment.
    remote_go_away: bool,

	  // localGoAway indicates that we should stop
	  // accepting futher connections. Must be first for alignment.
    local_go_away: bool,

	  // nextStreamID is the next stream we should
	  // send. This depends if we are a client/server.
    next_stream_id: StreamId,

	  // config holds our configuration
    config: Config,

	  // pings is used to track inflight pings
    pings: FnvHashMap<u32, ()>,
    ping_id: u32,

	  // streams maps a stream id to a stream,
    streams: FnvHashMap<StreamId, ()>,
    // inflight has an entry for any outgoing stream that has not yet been established.
    inflight: FnvHashMap<StreamId, ()>,

    stream: T,
}


impl<T> Session<T>
    where T: AsyncRead + AsyncWrite
{
    pub fn new() {}
    pub fn shutdown() {}
    pub fn close() {}
    pub fn flush() {}
    pub fn open_stream() -> StreamHandle {
        StreamHandle::new()
    }
}


impl<T> futures::Stream for Session<T>
where T: AsyncRead + AsyncWrite
{
    type Item = StreamHandle;
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(Async::NotReady)
    }
}
