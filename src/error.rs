use core::fmt::{Debug, Display, Formatter, Result};

use concurrent_queue::PushError;

/// An enum exposed pool internal error to public.
pub enum ThreadPoolError<T> {
    Canceled,
    Closed(T),
    Full(T),
}

#[cfg(feature = "with-async")]
impl<T> From<futures_channel::oneshot::Canceled> for ThreadPoolError<T> {
    fn from(_e: futures_channel::oneshot::Canceled) -> Self {
        ThreadPoolError::Canceled
    }
}

impl<T> From<PushError<T>> for ThreadPoolError<T> {
    fn from(e: PushError<T>) -> Self {
        match e {
            PushError::Closed(t) => ThreadPoolError::Closed(t),
            PushError::Full(t) => ThreadPoolError::Full(t),
        }
    }
}

impl<T> std::error::Error for ThreadPoolError<T> {}

impl<T> Debug for ThreadPoolError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        let mut fmt = f.debug_struct("ThreadPoolError");

        match self {
            ThreadPoolError::Closed(_) => fmt
                .field("cause", &"Closed")
                .field("description", &"ThreadPool is closed"),
            ThreadPoolError::Canceled => fmt
                .field("cause", &"Canceled")
                .field("description", &"Future is canceled. This could be caused either by caller drop the future before it resolved or a thread panic when executing the future"),
            ThreadPoolError::Full(_) => fmt
                .field("cause", &"Full")
                .field("description", &"ThreadPool is full"),
        };

        fmt.finish()
    }
}

impl<T> Display for ThreadPoolError<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(&self, f)
    }
}
