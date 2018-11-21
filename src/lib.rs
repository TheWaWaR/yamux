
// Latest Version Field value
pub const LATEST_VERSION: u8 = 0;
// Both sides assume the initial 256KB window
pub const INITIAL_WINDOW_SIZE: u32 = 256 * 1024;
// The 0 ID is reserved to represent the session.
pub const RESERVED_STREAM_ID: u32 = 0;

pub struct Session {
}

pub struct Stream {
}

#[derive(Clone, Debug)]
pub struct Header {
    version: u8,
    ty: Type,
    flags: Flags,
    stream_id: u32,
    length: u32,
}

// The type field is used to switch the frame message type. The following
//     message types are supported:
#[derive(Copy, Clone, Debug)]
#[repr(u8)]
pub enum Type {
    // Used to transmit data. May transmit zero length payloads depending on the flags.
    Data = 0x0,
    // Used to updated the senders receive window size. This is used to implement per-session flow control.
    WindowUpdate = 0x1,
    // Used to measure RTT. It can also be used to heart-beat and do keep-alives over TCP.
    Ping = 0x2,
    // Used to close a session.
    GoAway = 0x3,
}

#[derive(Copy, Clone, Debug)]
#[repr(u16)]
pub enum Flag {
    // SYN - Signals the start of a new stream. May be sent with a data or window update message. Also sent with a ping to indicate outbound.
    Syn = 0x1,
    // ACK - Acknowledges the start of a new stream. May be sent with a data or window update message. Also sent with a ping to indicate response.
    Ack = 0x2,
    // FIN - Performs a half-close of a stream. May be sent with a data message or window update.
    Fin = 0x4,
    // RST - Reset a stream immediately. May be sent with a data or window update message.
    Rst = 0x8
}

#[derive(Copy, Clone, Debug)]
pub struct Flags(u16);

impl Flags {
    pub fn new() -> Flags {
        Flags(0)
    }

    pub fn add(&mut self, flag: Flag) {
        self.0 |= flag as u16;
    }

    pub fn remove(&mut self, flag: Flag) {
        self.0 ^= flag as u16;
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
pub enum ReturnCode {
    // Normal termination
    Normal = 0x0,
    // Protocol error
    ProtocolError = 0x1,
    // Internal error
    InternalError = 0x2,
}
