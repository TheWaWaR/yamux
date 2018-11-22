
use crate::{
    StreamId,
    config::Config,
    stream::{Stream, StreamEvent, StreamState},
    frame::{Frame, Type, Header, Flags, FrameCodec},
};

use std::io;

use fnv::FnvHashMap;
use channel::{self, Sender, Receiver, TryRecvError};
use futures::{
    Async,
    Poll,
    Sink,
    Stream as FutureStream,
};
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

    // For receive events from sub streams (for clone to new stream)
    event_sender: Sender<StreamEvent>,
    // For receive events from sub streams
    event_receiver: Receiver<StreamEvent>,

    // Framed low level raw stream
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
        let (event_sender, event_receiver) = channel::bounded(32);
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
            event_sender,
            event_receiver,
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
            self.event_sender.clone(),
            receiver,
        )
    }

    fn handle_frame(&mut self, frame: Frame) {
        match frame.ty() {
            Type::Data | Type::WindowUpdate => {
                self.handle_stream_message(frame);
            }
            Type::Ping => {
                self.handle_ping(frame);
            }
            Type::GoAway => {
                self.handle_go_away(frame);
            }
        }
    }
    // Send message to stream (Data/WindowUpdate)
    fn handle_stream_message(&self, frame: Frame) {}
    fn handle_ping(&mut self, frame: Frame) {}
    fn handle_go_away(&mut self, frame: Frame) {}

    // Receive frames from low level stream
    fn recv_frames(&mut self) -> Poll<(), io::Error> {
        loop {
            match self.framed_stream.poll()? {
                Async::Ready(Some(frame)) => {
                    self.handle_frame(frame);
                }
                Async::Ready(None) => {
                    return Ok(Async::Ready(()));
                }
                Async::NotReady => {
                    break;
                }
            }
        }
        Ok(Async::NotReady)
    }


    fn handle_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::SendFrame(frame) => {
                // FIXME: poll_complete
                self.framed_stream.start_send(frame);
            }
            StreamEvent::StateChanged((stream_id, state)) => {
                match state {
                    StreamState::Closed => {
                    }
                    StreamState::Established => {
                    }
                    _ => {
                    }
                }
            }
        }
    }

    // Receive events from sub streams
    // TODO: should handle error
    fn recv_events(&mut self) {
        loop {
            match self.event_receiver.try_recv() {
                Ok(event) => self.handle_event(event),
                Err(TryRecvError::Empty) => {},
                Err(TryRecvError::Disconnected) => {},
            }
        }
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
