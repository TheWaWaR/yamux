extern crate crossbeam_channel as channel;
extern crate bytes;
extern crate byteorder;
extern crate fnv;
extern crate futures;
extern crate tokio_io;
extern crate tokio_codec;
extern crate log;

pub mod config;
pub mod error;
pub mod frame;
pub mod stream;
pub mod session;

pub type StreamId = u32;

// Latest Protocol Version
pub const PROTOCOL_VERSION: u8 = 0;
// The 0 ID is reserved to represent the session.
pub const RESERVED_STREAM_ID: StreamId = 0;
// The header is 12 bytes
pub const HEADER_SIZE: usize = 12;

