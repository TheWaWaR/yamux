pub mod config;
pub mod error;
pub mod frame;
pub mod session;
pub mod stream;

pub type StreamId = u32;

// Latest Protocol Version
pub const PROTOCOL_VERSION: u8 = 0;
// The 0 ID is reserved to represent the session.
pub const RESERVED_STREAM_ID: StreamId = 0;
// The header is 12 bytes
pub const HEADER_SIZE: usize = 12;
