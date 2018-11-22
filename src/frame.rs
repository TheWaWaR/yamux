use std::io;
use bytes::{Bytes, BytesMut};
use tokio_codec::{Encoder, Decoder};

use crate::{
    PROTOCOL_VERSION,
    StreamId,
};

pub struct Frame {
    header: Header,
    body: Option<Bytes>,
}

impl Frame {
    pub fn new(ty: Type, flags: Flags, stream_id: StreamId, length: u32) -> Frame {
        let version = PROTOCOL_VERSION;
        let header = Header {version, ty, flags, stream_id, length};
        Frame {
            header,
            body: None,
        }
    }

    pub fn set_body(&mut self, body: Option<Bytes>) {
        self.body = body
    }

    pub fn ty(&self) -> Type {
        self.header.ty
    }

    pub fn stream_id(&self) -> StreamId {
        self.header.stream_id
    }

    pub fn flags(&self) -> Flags {
        self.header.flags
    }

    pub fn length(&self) -> u32 {
        self.header.length
    }
}

#[derive(Clone, Debug)]
pub struct Header {
    version: u8,
    ty: Type,
    flags: Flags,
    stream_id: StreamId,
    length: u32,
}

// The type field is used to switch the frame message type.
// The following message types are supported:
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Type {
    // Used to transmit data.
    // May transmit zero length payloads depending on the flags.
    Data = 0x0,
    // Used to updated the senders receive window size.
    // This is used to implement per-session flow control.
    WindowUpdate = 0x1,
    // Used to measure RTT.
    // It can also be used to heart-beat and do keep-alives over TCP.
    Ping = 0x2,
    // Used to close a session.
    GoAway = 0x3,
}

#[derive(Copy, Clone, Debug)]
#[repr(u16)]
pub enum Flag {
    // SYN - Signals the start of a new stream.
    //   May be sent with a data or window update message.
    //   Also sent with a ping to indicate outbound.
    Syn = 0x1,

    // ACK - Acknowledges the start of a new stream.
    //   May be sent with a data or window update message.
    //   Also sent with a ping to indicate response.
    Ack = 0x2,

    // FIN - Performs a half-close of a stream.
    //   May be sent with a data message or window update.
    Fin = 0x4,

    // RST - Reset a stream immediately.
    //   May be sent with a data or window update message.
    Rst = 0x8
}

impl From<Flag> for Flags {
    fn from(value: Flag) -> Flags {
        Flags(value as u16)
    }
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Flags(u16);

impl Flags {
    pub fn add(&mut self, flag: Flag) {
        self.0 |= flag as u16;
    }

    pub fn remove(&mut self, flag: Flag) {
        self.0 ^= flag as u16;
    }

    pub fn has(&self, flag: Flag) -> bool {
        let flag_value = flag as u16;
        (self.0 & flag_value) == flag_value
    }

    pub fn value(&self) -> u16 {
        self.0
    }
}

// When a session is being terminated, the Go Away message should
// be sent. The Length should be set to one of the following to
// provide an error code:
#[derive(Copy, Clone, Debug)]
#[repr(u32)]
pub enum GoAwayCode {
    // Normal termination
    Normal = 0x0,
    // Protocol error
    ProtocolError = 0x1,
    // Internal error
    InternalError = 0x2,
}

pub struct FrameCodec {
}

impl FrameCodec {
    pub fn new() -> FrameCodec {
        FrameCodec {}
    }
}

pub enum FrameEncodeError {
    Io(io::Error)
}

pub enum FrameDecodeError {
    Io(io::Error)
}

impl From<io::Error> for FrameDecodeError {
    fn from(err: io::Error) -> Self {
        FrameDecodeError::Io(err)
    }
}

impl From<io::Error> for FrameEncodeError {
    fn from(err: io::Error) -> Self {
        FrameEncodeError::Io(err)
    }
}

impl Decoder for FrameCodec {
    type Item = Frame;
    type Error = FrameDecodeError;

    fn decode(
        &mut self,
        src: &mut BytesMut
    ) -> Result<Option<Self::Item>, Self::Error> {
        // TODO:
        Ok(None)
    }
}

impl Encoder for FrameCodec {
    type Item = Frame;
    type Error = FrameEncodeError;
    fn encode(
        &mut self,
        item: Self::Item,
        dst: &mut BytesMut
    ) -> Result<(), Self::Error> {
        // TODO:
        Ok(())
    }
}
