use core::fmt::{Debug, Display, Formatter, Result};

use std::sync::mpsc::{RecvError, RecvTimeoutError};

/// An enum exposed pool internal error to public.
pub enum ThreadPoolError {
    TimeOut,
    Disconnect,
}

impl From<RecvError> for ThreadPoolError {
    fn from(_e: RecvError) -> Self {
        ThreadPoolError::Disconnect
    }
}

impl From<RecvTimeoutError> for ThreadPoolError {
    fn from(e: RecvTimeoutError) -> Self {
        match e {
            RecvTimeoutError::Disconnected => ThreadPoolError::Disconnect,
            RecvTimeoutError::Timeout => ThreadPoolError::TimeOut,
        }
    }
}

impl std::error::Error for ThreadPoolError {}

impl Debug for ThreadPoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut fmt = f.debug_struct("ThreadPoolError");

        match self {
            ThreadPoolError::Disconnect => fmt
                .field("cause", &"Disconnect")
                .field("description", &"ThreadPool is closed(From receiver)"),
            ThreadPoolError::TimeOut => fmt
                .field("cause", &"TimeOut")
                .field("description", &"Wait too long incoming message"),
        };

        fmt.finish()
    }
}

impl Display for ThreadPoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(&self, f)
    }
}
