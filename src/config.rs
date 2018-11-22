
use crate::{
    INITIAL_STREAM_WINDOW,
    DEFAULT_ACCEPT_BACKLOG,
    DEFAULT_KEEPALIVE_INTERVAL,
    DEFAULT_WRITE_TIMEOUT,
};

use std::time::Duration;


pub struct Config {
    // AcceptBacklog is used to limit how many streams may be
    // waiting an accept.
    accept_backlog: u32,

    // EnableKeepalive is used to do a period keep alive
    // messages using a ping.
    enable_keepalive: bool,

    // KeepAliveInterval is how often to perform the keep alive
    keepalive_interval: Duration,

    // ConnectionWriteTimeout is meant to be a "safety valve" timeout after
    // we which will suspect a problem with the underlying connection and
    // close it. This is only applied to writes, where's there's generally
    // an expectation that things will move along quickly.
    connection_write_timeout: Duration,

    // MaxStreamWindowSize is used to control the maximum
    // window size that we allow for a stream.
    max_stream_window_size: u32,
}

impl ::std::default::Default for Config {
    fn default() -> Config {
        Config {
            accept_backlog: DEFAULT_ACCEPT_BACKLOG,
            enable_keepalive: true,
            keepalive_interval: DEFAULT_KEEPALIVE_INTERVAL,
            connection_write_timeout: DEFAULT_WRITE_TIMEOUT,
            max_stream_window_size: INITIAL_STREAM_WINDOW,
        }
    }
}
