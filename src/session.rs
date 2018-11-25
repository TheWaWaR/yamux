use std::io;
use std::time::Instant;
use std::collections::{VecDeque, BTreeMap};

use log::{error, warn, info, debug, trace};
use fnv::{FnvHashMap, FnvHashSet};
use futures::{
    try_ready,
    task,
    Async,
    AsyncSink,
    Poll,
    Sink,
    Stream,
    sync::mpsc::{channel, Sender, Receiver},
};
use tokio_io::{AsyncRead, AsyncWrite};
use tokio_codec::{Framed};

use crate::{
    StreamId,
    error::Error,
    config::Config,
    stream::{StreamHandle, StreamEvent, StreamState},
    frame::{Frame, Type, Flags, Flag, GoAwayCode, FrameCodec},
};


pub struct Session<T> {
    // Framed low level raw stream
    framed_stream: Framed<T, FrameCodec>,

    // Got EOF from low level raw stream
    eof: bool,

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
    ty: SessionType,

    // config holds our configuration
    config: Config,

    // pings is used to track inflight pings
    pings: BTreeMap<u32, Instant>,
    ping_id: u32,
    // Last ping time
    last_ping_at: Option<Instant>,

    // streams maps a stream id to a sender of stream,
    streams: FnvHashMap<StreamId, Sender<Frame>>,
    // inflight has an entry for any outgoing stream that has not yet been established.
    inflight: FnvHashSet<StreamId>,
    // The StreamHandle not yet been polled
    pending_streams: VecDeque<StreamHandle>,

    pending_frames: VecDeque<Frame>,

    // For receive events from sub streams (for clone to new stream)
    event_sender: Sender<StreamEvent>,
    // For receive events from sub streams
    event_receiver: Receiver<StreamEvent>,

}

#[derive(Debug)]
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
        let (event_sender, event_receiver) = channel(32);
        let framed_stream = Framed::new(raw_stream, FrameCodec::default());

        Session {
            framed_stream,
            eof: false,
            shutdown: false,
            remote_go_away: false,
            local_go_away: false,
            next_stream_id,
            ty,
            config,
            pings: BTreeMap::default(),
            ping_id: 0,
            last_ping_at: None,
            streams: FnvHashMap::default(),
            inflight: FnvHashSet::default(),
            pending_streams: VecDeque::default(),
            pending_frames: VecDeque::default(),
            event_sender,
            event_receiver,
        }
    }

    pub fn new_server(raw_stream: T, config: Config) -> Session<T> {
        Self::new(raw_stream, config, SessionType::Server)
    }

    pub fn new_client(raw_stream: T, config: Config) -> Session<T> {
        Self::new(raw_stream, config, SessionType::Client)
    }

    // shutdown is used to close the session and all streams.
    // Attempts to send a GoAway before closing the connection.
    pub fn shutdown(&mut self) -> Poll<(), io::Error>{
        if self.shutdown {
            return Ok(Async::Ready(()));
        }
        self.shutdown = true;
        self.send_go_away()
    }

    // Send all pending frames to remote streams
    fn flush(&mut self) -> Result<(), io::Error> {
        self.recv_events()?;
        self.framed_stream.poll_complete()?;
        Ok(())
    }

    fn is_dead(&self) -> bool {
        self.shutdown || self.eof
    }

    fn send_ping(&mut self, ping_id: Option<u32>) -> Poll<u32, io::Error> {
        let (flag, ping_id) = match ping_id {
            Some(ping_id) => (Flag::Ack, ping_id),
            None => {
                self.ping_id = self.ping_id.overflowing_add(1).0;
                (Flag::Syn, self.ping_id)
            }
        };
        let frame = Frame::new_ping(Flags::from(flag), ping_id);
        self.send_frame(frame).map(|_| Async::Ready(ping_id))
    }

    // GoAway can be used to prevent accepting further
    // connections. It does not close the underlying conn.
    pub fn send_go_away(&mut self) -> Poll<(), io::Error>{
        self.local_go_away = true;
        let frame = Frame::new_go_away(GoAwayCode::Normal);
        self.send_frame(frame)
    }

    pub fn open_stream(&mut self) -> Result<StreamHandle, Error> {
        if self.is_dead() {
            Err(Error::SessionShutdown)
        } else if self.remote_go_away {
            Err(Error::RemoteGoAway)
        } else {
            let stream = self.create_stream(None);
            self.inflight.insert(stream.id());
            Ok(stream)
        }
    }

    fn keep_alive(&mut self) -> Poll<(), io::Error> {
        if self.is_dead() {
            return Ok(Async::Ready(()));
        }

        let now = Instant::now();
        let ping_at = match self.last_ping_at {
            Some(ping_at) => {
                if now >= ping_at + self.config.keepalive_interval {
                    Some(now)
                } else {
                    None
                }
            },
            None => Some(now),
        };
        if let Some(now) = ping_at {
            let ping_id = try_ready!(self.send_ping(None));
            debug!("[{:?}] sent keep_alive ping ({:?})", self.ty, ping_id);
            self.pings.insert(ping_id, now);
            self.last_ping_at = Some(now);
        }
        Ok(Async::Ready(()))
    }

    fn create_stream(&mut self, stream_id: Option<StreamId>) -> StreamHandle {
        let (stream_id, state) = match stream_id {
            Some(stream_id) => {
                (stream_id, StreamState::SynReceived)
            }
            None => {
                let next_id = self.next_stream_id;
                self.next_stream_id += 2;
                (next_id, StreamState::Init)
            }
        };
        let (frame_sender, frame_receiver) = channel(8);
        self.streams.insert(stream_id, frame_sender);
        let mut stream = StreamHandle::new(
            stream_id,
            self.event_sender.clone(),
            frame_receiver,
            state,
            self.config.max_stream_window_size,
            self.config.max_stream_window_size,
        );
        if let Err(err) = stream.send_window_update() {
            debug!("[{:?}] stream.send_window_update error={:?}", self.ty, err);
        }
        stream
    }

    fn send_frame(&mut self, frame: Frame) -> Poll<(), io::Error> {
        debug!("[{:?}] Session::send_frame({:?})", self.ty, frame);
        self.pending_frames.push_back(frame);
        while let Some(frame) = self.pending_frames.pop_front() {
            if self.is_dead() {
                break;
            }

            match self.framed_stream.start_send(frame.clone()) {
                Ok(AsyncSink::NotReady(frame)) => {
                    debug!("[{:?}] framed_stream NotReady, frame: {:?}", self.ty, frame);
                    self.pending_frames.push_front(frame);
                    return Ok(Async::NotReady)
                }
                Ok(AsyncSink::Ready) => {
                    debug!("[{:?}] framed_stream sent, frame: {:?}", self.ty, frame);
                },
                Err(err) => {
                    debug!("[{:?}] framed_stream error: {:?}", self.ty, err);
                    return Err(err);
                }
            }
        }
        debug!("[{:?}] Session::send_frame() finished", self.ty);
        Ok(Async::Ready(()))
    }

    fn handle_frame(&mut self, frame: Frame) -> Result<(), io::Error> {
        debug!("[{:?}] Session::handle_frame({:?})", self.ty, frame);
        match frame.ty() {
            Type::Data | Type::WindowUpdate => {
                self.handle_stream_message(frame)?;
            }
            Type::Ping => {
                self.handle_ping(frame)?;
            }
            Type::GoAway => {
                self.handle_go_away(frame);
            }
        }
        Ok(())
    }

    // Send message to stream (Data/WindowUpdate)
    fn handle_stream_message(&mut self, frame: Frame) -> Result<(), io::Error> {
        let stream_id = frame.stream_id();
        if frame.flags().contains(Flag::Syn) {
            if self.local_go_away {
                let flags = Flags::from(Flag::Rst);
                let frame = Frame::new_window_update(flags, stream_id, 0);
                self.send_frame(frame)?;
                // TODO: should report error?
                return Ok(());
            }
            debug!("[{:?}] Accept a stream id={}", self.ty, stream_id);
            let stream = self.create_stream(Some(stream_id));
            self.pending_streams.push_back(stream);
        }
        let disconnected = {
            if let Some(frame_sender) = self.streams.get_mut(&stream_id) {
                // TODO: handle error
                frame_sender.try_send(frame).is_err()
            } else {
                // TODO: stream already closed ?
                false
            }
        };
        if disconnected {
            self.streams.remove(&stream_id);
        }
        Ok(())
    }

    fn handle_ping(&mut self, frame: Frame) -> Result<(), io::Error> {
        let flags = frame.flags();
        if flags.contains(Flag::Syn) {
            // Send ping back
            self.send_ping(Some(frame.length()))?;
        } else if flags.contains(Flag::Ack) {
            self.pings.remove(&frame.length());
        } else {
            // TODO: unexpected case, send a GoAwayCode::ProtocolError ?
        }
        Ok(())
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
            if self.is_dead() {
                return Ok(Async::Ready(()));
            }

            match self.framed_stream.poll() {
                Ok(Async::Ready(Some(frame))) => {
                    self.handle_frame(frame)?;
                }
                Ok(Async::Ready(None)) => {
                    self.eof = true;
                }
                Ok(Async::NotReady) => {
                    debug!("[{:?}] recv frames NotReady", self.ty);
                    return Ok(Async::NotReady);
                }
                Err(err) => {
                    warn!("[{:?}] Session recv_frames error: {:?}", self.ty, err);
                    return Err(err);
                }
            }
        }
    }

    fn handle_event(&mut self, event: StreamEvent) -> Result<(), io::Error> {
        debug!("[{:?}] Session::handle_event({:?})", self.ty, event);
        match event {
            StreamEvent::SendFrame(frame) => {
                self.send_frame(frame)?;
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
                self.flush()?;
                let _ = responsor.send(());
            }
        }
        Ok(())
    }

    // Receive events from sub streams
    // TODO: should handle error
    fn recv_events(&mut self) -> Poll<(), io::Error>{
        loop {
            if self.is_dead() {
                return Ok(Async::Ready(()));
            }

            match self.event_receiver.poll() {
                Ok(Async::Ready(Some(event))) => self.handle_event(event)?,
                Ok(Async::Ready(None)) => {
                    // Since session hold one event sender,
                    // the channel can not be disconnected.
                    unreachable!()
                }
                Ok(Async::NotReady) => {
                    debug!("[{:?}] recv events NotReady", self.ty);
                    return Ok(Async::NotReady);
                },
                Err(()) => {
                    // TODO: When would happend?
                },
            }
        }
    }
}


impl<T> Stream for Session<T>
where T: AsyncRead + AsyncWrite
{
    type Item = StreamHandle;
    type Error = io::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        debug!("[{:?}] Session::poll()", self.ty);
        loop {
            if self.is_dead() {
                return Ok(Async::Ready(None));
            }

            if self.config.enable_keepalive {
                self.keep_alive()?;
            }

            let mut recv_frames_not_ready = false;
            let mut recv_events_not_ready = false;
            match self.recv_frames()? {
                Async::Ready(_) => {},
                Async::NotReady => {
                    debug!("[{:?}] recv_frames NotReady", self.ty);
                    recv_frames_not_ready = true;
                }
            }
            // Ignore Async::NotReady from recv_events()
            match self.recv_events()? {
                Async::Ready(_) => {},
                Async::NotReady => {
                    debug!("[{:?}] recv_events NotReady", self.ty);
                    recv_events_not_ready = true;
                }
            }

            if let Some(stream) = self.pending_streams.pop_front() {
                return Ok(Async::Ready(Some(stream)));
            }

            if recv_frames_not_ready && recv_events_not_ready {
                return Ok(Async::NotReady);
            }

        }
    }
}
