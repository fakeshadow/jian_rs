use core::fmt::{Debug, Display, Formatter, Result};

use concurrent_queue::PushError;

use crate::pool_inner::Job;

/// An enum exposed pool internal error to public.
pub enum ThreadPoolError {
    Canceled,
    Closed(Job),
    Full(Job),
}

impl ThreadPoolError {
    /// Try to extract the original job from error.
    ///
    /// The `job` is a function trait object.
    ///
    /// # Example:
    /// ```rust
    /// let pool = jian_rs::ThreadPool::builder().build();
    ///
    /// // close the pool
    /// pool.close();
    ///
    /// // try to add new job.
    /// let result = pool.execute(||{ println!("some code"); });
    ///
    /// // job is failed with error.
    /// if let Err(e) = result {
    ///     // extract the job
    ///     let job = e.into_inner().unwrap();
    ///     // run the job manually
    ///     job();
    /// }
    /// ```
    pub fn into_inner(self) -> Option<Box<dyn FnOnce() + Send + 'static>> {
        match self {
            ThreadPoolError::Closed(job) => Some(job),
            ThreadPoolError::Full(job) => Some(job),
            ThreadPoolError::Canceled => None,
        }
    }
}

#[cfg(feature = "with-async")]
impl From<futures_channel::oneshot::Canceled> for ThreadPoolError {
    fn from(_e: futures_channel::oneshot::Canceled) -> Self {
        ThreadPoolError::Canceled
    }
}

impl From<PushError<Job>> for ThreadPoolError {
    fn from(e: PushError<Job>) -> Self {
        match e {
            PushError::Closed(t) => ThreadPoolError::Closed(t),
            PushError::Full(t) => ThreadPoolError::Full(t),
        }
    }
}

impl std::error::Error for ThreadPoolError {}

impl Debug for ThreadPoolError {
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

impl Display for ThreadPoolError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        Display::fmt(&self, f)
    }
}
