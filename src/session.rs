
use crate::{
    StreamId,
    RESERVED_STREAM_ID,
    config::Config,
    stream::{Stream, StreamEvent, StreamState},
    frame::{Frame, Type, Header, Flags, Flag, GoAwayCode, FrameCodec},
};

use std::io;
use std::time::Instant;
use std::collections::{VecDeque, BTreeMap};

use fnv::{FnvHashMap, FnvHashSet};
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
    // Framed low level raw stream
    framed_stream: Framed<T, FrameCodec>,

	  // shutdown is used to safely close a session
    shutdown: bool,

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
    pings: BTreeMap<u32, Instant>,
    ping_id: u32,

    stream_buf: Vec<Stream>,
    // streams maps a stream id to a sender of stream,
    streams: FnvHashMap<StreamId, Sender<Frame>>,
    // inflight has an entry for any outgoing stream that has not yet been established.
    inflight: FnvHashSet<StreamId>,
    // The Stream not yet been polled
    pending: VecDeque<Stream>,

    // For receive events from sub streams (for clone to new stream)
    event_sender: Sender<StreamEvent>,
    // For receive events from sub streams
    event_receiver: Receiver<StreamEvent>,

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
        let framed_stream = Framed::new(raw_stream, FrameCodec::default());

        Session {
            framed_stream,
            shutdown: false,
            remote_go_away: false,
            local_go_away: false,
            next_stream_id,
            config,
            pings: BTreeMap::default(),
            ping_id: 0,
            stream_buf: Vec::new(),
            streams: FnvHashMap::default(),
            inflight: FnvHashSet::default(),
            pending: VecDeque::default(),
            event_sender,
            event_receiver,
        }
    }

    pub fn shutdown() {}

    pub fn close(&mut self) {}

    // Send all pending frames to remote streams
    fn flush(&mut self) {
        self.recv_events();
        self.framed_stream.poll_complete();
    }

    fn next_stream_id(&mut self) -> StreamId {
        let next_id = self.next_stream_id;
        self.next_stream_id += 2;
        next_id
    }

    pub fn send_go_away(&mut self) {
        self.local_go_away = true;
        let frame = Frame::new_go_away(GoAwayCode::Normal);
        self.send_frame(frame);
    }

    pub fn open_stream(&mut self) -> Stream {
        let stream = self.create_stream(StreamState::Init);
        self.inflight.insert(stream.id());
        stream
    }

    fn create_stream(&mut self, state: StreamState) -> Stream {
        let next_id = self.next_stream_id();
        let (frame_sender, frame_receiver) = channel::bounded(8);
        self.streams.insert(next_id, frame_sender);
        let mut stream = Stream::new(
            next_id,
            self.event_sender.clone(),
            frame_receiver,
            state,
        );
        stream.send_window_update();
        stream
    }

    // FIXME: framed_stream.poll_complete
    // FIXME: add those frames to a pending list?
    fn send_frame(&mut self, frame: Frame) {
        self.framed_stream.start_send(frame);
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
    fn handle_stream_message(&mut self, frame: Frame) {
        if frame.flags().contains(Flag::Syn) {
            let _ = self.create_stream(StreamState::SynReceived);
        }
        let stream_id = frame.stream_id();
        let disconnected = {
            if let Some(frame_sender) = self.streams.get(&stream_id) {
                frame_sender.send(frame).is_err()
            } else {
                // TODO: stream already closed ?
                false
            }
        };
        if disconnected {
            self.streams.remove(&stream_id);
        }
    }

    fn handle_ping(&mut self, frame: Frame) {
        let flags = frame.flags();
        if flags.contains(Flag::Syn) {
            // Send ping back
            let flags = Flags::from(Flag::Ack);
            let frame = Frame::new_ping(flags, frame.length());
            self.send_frame(frame);
        } else if flags.contains(Flag::Ack) {
            self.pings.remove(&frame.length());
        } else {
            // TODO: unexpected case, send a GoAwayCode::ProtocolError ?
        }
    }

    fn handle_go_away(&mut self, frame: Frame) {
        match GoAwayCode::from(frame.length()) {
            GoAwayCode::Normal => {
                self.remote_go_away = true;
            }
            GoAwayCode::ProtocolError => {
                // TODO: report error
            }
            GoAwayCode::InternalError => {
                // TODO: report error
            }
        }
    }

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
                self.send_frame(frame);
            }
            StreamEvent::StateChanged((stream_id, state)) => {
                match state {
                    StreamState::Closed => {
                        self.streams.remove(&stream_id);
                    }
                    StreamState::Established => {
                        self.inflight.remove(&stream_id);
                    }
                    // For further functions
                    _ => {}
                }
            }
            StreamEvent::Flush((_stream_id, responsor)) => {
                self.flush();
                let _ = responsor.send(());
            }
        }
    }

    // Receive events from sub streams
    // TODO: should handle error
    fn recv_events(&mut self) {
        loop {
            match self.event_receiver.try_recv() {
                Ok(event) => self.handle_event(event),
                Err(TryRecvError::Empty) => {
                    break;
                },
                Err(TryRecvError::Disconnected) => {
                    // Since session hold one event sender,
                    // the channel can not be disconnected.
                    unreachable!()
                },
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
        let is_eof = self.recv_frames()?.is_ready();
        self.recv_events();
        if is_eof {
            Ok(Async::Ready(None))
        } else {
            Ok(Async::NotReady)
        }
    }
}
