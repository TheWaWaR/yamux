
use crate::{
    StreamId,
    config::Config,
    stream::{Stream, StreamState},
    frame::{Frame, Header, Flags, FrameCodec},
};

use std::io;

use fnv::FnvHashMap;
use channel::{self, Sender, Receiver};
use futures::{Async, Poll};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_codec::{Framed};


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

    stream_buf: Vec<Stream>,
	  // streams maps a stream id to a sender of stream,
    streams: FnvHashMap<StreamId, Sender<Frame>>,
    // inflight has an entry for any outgoing stream that has not yet been established.
    inflight: FnvHashMap<StreamId, Sender<Frame>>,

    // For receive frames from sub streams (for clone to new stream)
    frame_sender: Sender<Frame>,
    // For receive frames from sub streams
    frame_receiver: Receiver<Frame>,

    // For receive state change from sub streams (for clone to new stream)
    state_sender: Sender<(StreamId, StreamState)>,
    // For receive state change from sub streams
    state_receiver: Receiver<(StreamId, StreamState)>,

    // Framed stream
    framed_stream: Framed<T, FrameCodec>,
}

pub enum SessionType {
    Client,
    Server,
}

impl<T> Session<T>
    where T: AsyncRead + AsyncWrite
{
    pub fn new(raw_stream: T, config: Config, ty: SessionType) -> Session<T> {
        let next_stream_id = match ty {
            SessionType::Client => 1,
            SessionType::Server => 2,
        };
        let (frame_sender, frame_receiver) = channel::bounded(16);
        let (state_sender, state_receiver) = channel::bounded(8);
        let framed_stream = Framed::new(raw_stream, FrameCodec::new());

        Session {
            remote_go_away: false,
            local_go_away: false,
            next_stream_id,
            config,
            pings: FnvHashMap::default(),
            ping_id: 0,
            stream_buf: Vec::new(),
            streams: FnvHashMap::default(),
            inflight: FnvHashMap::default(),
            frame_sender,
            frame_receiver,
            state_sender,
            state_receiver,
            framed_stream,
        }
    }

    pub fn shutdown() {}

    pub fn close(&mut self) {}

    pub fn flush() {}

    fn next_stream_id(&mut self) -> StreamId {
        let next_id = self.next_stream_id;
        self.next_stream_id += 2;
        next_id
    }

    pub fn open_stream(&mut self) -> Stream {
        let (sender, receiver) = channel::bounded(8);
        let next_id = self.next_stream_id();
        self.inflight.insert(next_id, sender);
        Stream::new(
            next_id,
            self.frame_sender.clone(),
            self.state_sender.clone(),
            receiver,
        )
    }

    fn recv_from_raw_stream(&mut self) -> Poll<(), io::Error> {
        Ok(Async::NotReady)
    }

    fn recv_from_sub_streams(&mut self) {
    }
}


impl<T> futures::Stream for Session<T>
where T: AsyncRead + AsyncWrite
{
    type Item = Stream;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        Ok(Async::NotReady)
    }
}
