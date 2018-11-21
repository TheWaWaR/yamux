extern crate crossbeam_channel as channel;
extern crate bytes;
extern crate fnv;
extern crate futures;
extern crate tokio_io;
extern crate tokio_codec;

pub mod config;
pub mod frame;
pub mod stream;
pub mod session;

// Latest Protocol Version
pub const PROTOCOL_VERSION: u8 = 0;
// Both sides assume the initial 256KB window size
pub const INITIAL_STREAM_WINDOW: u32 = 256 * 1024;
// The 0 ID is reserved to represent the session.
pub const RESERVED_STREAM_ID: u32 = 0;
