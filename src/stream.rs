
use std::io;

use channel::{Receiver, Sender, TryRecvError};
use bytes::{BytesMut, Bytes};
use futures::{Async, Poll};
use tokio_io::{AsyncRead, AsyncWrite};

use crate::{
    INITIAL_STREAM_WINDOW,
    StreamId,
    frame::{Frame, Header, Type, Flag, Flags},
};


pub struct Stream {
    id: StreamId,
    state: StreamState,

    recv_window: u32,
    send_window: u32,

    // Send stream event to parent session
    event_sender: Sender<StreamEvent>,

    // Receive frame of current stream from parent session
    // (if the sender closed means session closed the stream should close too)
    frame_receiver: Receiver<Frame>,
}

impl Stream {
    pub fn new(
        id: StreamId,
        event_sender: Sender<StreamEvent>,
        frame_receiver: Receiver<Frame>,
    ) -> Stream {
        Stream {
            recv_window: INITIAL_STREAM_WINDOW,
            send_window: INITIAL_STREAM_WINDOW,
            id,
            state: StreamState::Init,
            event_sender,
            frame_receiver,
        }
    }

    pub fn id(&self) -> StreamId {self.id}
    pub fn state(&self) -> StreamState {self.state}
    pub fn recv_window(&self) -> u32 {self.recv_window}
    pub fn send_window(&self) -> u32 {self.send_window}

    pub fn close(&mut self) {
        match self.state {
            StreamState::SynSent | StreamState::SynReceived | StreamState::Established => {
                self.state = StreamState::LocalClosing;
                self.send_close();
            }
            StreamState::RemoteClosing => {
                self.state = StreamState::Closed;
                self.send_close();
                let event = StreamEvent::StateChanged((self.id, self.state));
                if let Err(_) = self.event_sender.send(event) {
                    self.session_gone();
                }
            }
            _ => {}
        }
    }

    fn send_close(&mut self) {
        let mut flags = self.get_flags();
        flags.add(Flag::Fin);
        let frame = Frame::new(Type::WindowUpdate, flags, self.id, 0);
        let event = StreamEvent::SendFrame(frame);
        if let Err(_) = self.event_sender.send(event) {
            self.session_gone();
        }
    }

    // FIXME: what happened when parent session gone?
    fn session_gone(&mut self) {
    }

    fn get_flags(&mut self) -> Flags {
        match self.state {
            StreamState::Init => {
                self.state = StreamState::SynSent;
                Flags::from(Flag::Syn)
            }
            StreamState::SynReceived => {
                self.state = StreamState::Established;
                Flags::from(Flag::Ack)
            }
            _ => Flags::default()
        }
    }

    fn handle_frame(&mut self, frame: Frame) {
    }

    fn recv_frames(&mut self) {
        loop {
            match self.frame_receiver.try_recv() {
                Ok(frame) => self.handle_frame(frame),
                Err(TryRecvError::Empty) => {},
                Err(TryRecvError::Disconnected) => {},
            }
        }
    }
}

impl io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Ok(0)
    }
}

impl io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        Ok(0)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl AsyncRead for Stream {}

impl AsyncWrite for Stream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        Ok(Async::NotReady)
    }
}

pub enum StreamEvent {
    SendFrame(Frame),
    StateChanged((StreamId, StreamState)),
}

#[derive(Debug, Copy, Clone)]
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
