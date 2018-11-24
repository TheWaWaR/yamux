
use std::io;

use channel::{self, Receiver, Sender, TryRecvError};
use bytes::{BytesMut, Bytes};
use futures::{Async, Poll};
use tokio_io::{AsyncRead, AsyncWrite};

use crate::{
    StreamId,
    error::Error,
    frame::{Frame, Type, Flag, Flags},
};


pub struct Stream {
    id: StreamId,
    state: StreamState,

    max_recv_window: u32,
    recv_window: u32,
    send_window: u32,
    data_buf: BytesMut,

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
        state: StreamState,
        recv_window_size: u32,
        send_window_size: u32,
    ) -> Stream {
        assert!(state == StreamState::Init || state == StreamState::SynReceived);
        Stream {
            id,
            state,
            max_recv_window: recv_window_size,
            recv_window: recv_window_size,
            send_window: send_window_size,
            data_buf: BytesMut::default(),
            event_sender,
            frame_receiver,
        }
    }

    pub fn id(&self) -> StreamId {self.id}
    pub fn state(&self) -> StreamState {self.state}
    pub fn recv_window(&self) -> u32 {self.recv_window}
    pub fn send_window(&self) -> u32 {self.send_window}

    pub fn close(&mut self) -> Result<(), Error> {
        match self.state {
            StreamState::SynSent
                | StreamState::SynReceived
                | StreamState::Established =>
            {
                self.state = StreamState::LocalClosing;
                self.send_close()?;
            }
            StreamState::RemoteClosing => {
                self.state = StreamState::Closed;
                self.send_close()?;
                let event = StreamEvent::StateChanged((self.id, self.state));
                self.send_event(event)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn send_event(&mut self, event: StreamEvent) -> Result<(), Error> {
        self.event_sender.send(event).map_err(|_| Error::SessionShutdown)
    }

    fn send_frame(&mut self, frame: Frame) -> Result<(), Error> {
        let event = StreamEvent::SendFrame(frame);
        self.send_event(event)
    }

    pub fn send_window_update(&mut self) -> Result<(), Error> {
        let buf_len = self.data_buf.len() as u32;
        let delta = self.max_recv_window - buf_len - self.recv_window;

	      // Check if we can omit the update
        let flags = self.get_flags();
        if delta < (self.max_recv_window / 2) && flags.value() == 0 {
            return Ok(());
        }
	      // Update our window
        self.recv_window += delta;
        let frame = Frame::new_window_update(flags, self.id, delta);
        self.send_frame(frame)
    }

    fn send_data(&mut self, data: &[u8]) -> Result<(), Error>  {
        let flags = self.get_flags();
        let frame = Frame::new_data(flags, self.id, Bytes::from(data));
        self.send_frame(frame)
    }

    fn send_close(&mut self) -> Result<(), Error> {
        let mut flags = self.get_flags();
        flags.add(Flag::Fin);
        let frame = Frame::new_window_update(flags, self.id, 0);
        self.send_frame(frame)
    }

    fn process_flags(&mut self, flags: Flags) -> Result<(), Error> {
        if flags.contains(Flag::Ack) {
            if self.state == StreamState::SynSent {
                self.state = StreamState::SynReceived;
            }
        }
        let mut close_stream = false;
        if flags.contains(Flag::Fin) {
            match self.state {
                StreamState::Init
                    | StreamState::SynSent
                    | StreamState::SynReceived
                    | StreamState::Established =>
                {
                    self.state = StreamState::RemoteClosing;
                }
                StreamState::LocalClosing => {
                    self.state = StreamState::Closed;
                    close_stream = true;
                }
                _ => return Err(Error::UnexpectedFlag)
            }
        }
        if flags.contains(Flag::Rst) {
            self.state = StreamState::Reset;
            close_stream = true;
        }

        if close_stream {
            let event = StreamEvent::StateChanged((self.id, self.state));
            self.send_event(event);
        }
        Ok(())
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

    fn handle_frame(&mut self, frame: Frame) -> Result<(), Error> {
        match frame.ty() {
            Type::WindowUpdate => {
                self.handle_window_update(frame)?;
            }
            Type::Data => {
                self.handle_data(frame)?;
            }
            _ => {
                return Err(Error::InvalidMsgType);
            },
        }
        Ok(())
    }

    fn handle_window_update(&mut self, frame: Frame) -> Result<(), Error> {
        self.process_flags(frame.flags())?;
        self.send_window += frame.length();
        Ok(())
    }

    fn handle_data(&mut self, frame: Frame) -> Result<(), Error> {
        self.process_flags(frame.flags())?;
        let length = frame.length();
        if length > self.recv_window {
            return Err(Error::RecvWindowExceeded);
        }

        let (_, body) = frame.into_parts();
        if let Some(data) = body {
            self.data_buf.extend_from_slice(&data);
        }
        self.recv_window -= length;
        Ok(())
    }

    fn recv_frames(&mut self) -> Poll<(), Error> {
        loop {
            match self.state {
                StreamState::LocalClosing
                    | StreamState::RemoteClosing
                    | StreamState::Closed =>
                {
                    // TODO: return error
                }
                StreamState::Reset => {
                    // TODO: return error
                }
                _ => {}
            }

            match self.frame_receiver.try_recv() {
                Ok(frame) => self.handle_frame(frame)?,
                Err(TryRecvError::Empty) => {
                    return Ok(Async::NotReady);
                },
                Err(TryRecvError::Disconnected) => {
                    return Err(Error::SessionShutdown);
                },
            }
        }
    }
}

impl io::Read for Stream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // TODO: error handling
        self.recv_frames();
        let n = ::std::cmp::min(buf.len(), self.data_buf.len());
        let b = self.data_buf.split_to(n);
        buf.copy_from_slice(&b);
        self.send_window_update();
        Ok(n)
    }
}

impl io::Write for Stream {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // TODO: error handling
        self.recv_frames();
        if self.send_window == 0 {
            return Err(io::ErrorKind::WouldBlock.into());
        }
        let n = ::std::cmp::min(self.send_window as usize, buf.len());
        let data = &buf[0..n];
        self.send_data(data);
        self.send_window -= n as u32;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        // TODO: error handling
        self.recv_frames();
        let (sender, receiver) = channel::bounded(1);
        let event = StreamEvent::Flush((self.id, sender));
        match self.send_event(event) {
            Err(_) => Err(io::Error::new(io::ErrorKind::ConnectionReset, "")),
            Ok(()) => {
                let _ = receiver.recv();
                Ok(())
            }
        }
    }
}

impl AsyncRead for Stream {}

impl AsyncWrite for Stream {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        let frame = Frame::new_window_update(
            Flags::from(Flag::Rst),
            self.id,
            0
        );
        self.send_frame(frame);
        self.recv_frames();
        Ok(Async::Ready(()))
    }
}

pub enum StreamEvent {
    SendFrame(Frame),
    StateChanged((StreamId, StreamState)),
    // Flush stream's frames to remote stream, with a channel for sync
    Flush((StreamId, Sender<()>))
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
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
