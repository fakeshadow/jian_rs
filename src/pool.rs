use core::fmt::{Debug, Formatter, Result as FmtResult};

use std::sync::{
    mpsc::{channel, Sender},
    Arc,
};

use crate::builder::Builder;
use crate::pool_inner::{Job, ThreadPoolInner};
use crate::ThreadPoolError;

/// Abstraction of a thread pool for basic parallelism.
pub struct ThreadPool {
    tx: Sender<Job>,
    inner: Arc<ThreadPoolInner>,
}

impl ThreadPool {
    /// Creates a new thread pool builder
    ///
    /// See [`Builder`]
    pub fn builder() -> Builder {
        Default::default()
    }

    /// Executes the function `job` on a thread in the pool.
    pub fn execute<F>(&self, job: F) -> Result<(), ThreadPoolError>
    where
        F: FnOnce() + Send + 'static,
    {
        let inner = &self.inner;

        // read from the previous state when we increment work count
        let (is_closed, should_spawn) = inner.inc_work_count();

        let job = Box::new(job);

        if is_closed {
            Err(ThreadPoolError::Closed(job))
        // We don't need to dec work count as we don't send the job and/or spawn new thread.
        } else {
            // send job through channel.
            self.tx.send(job)?;

            if should_spawn {
                inner.spawn_thread(inner);
            }

            Ok(())
        }
    }

    /// execute the function `job` asynchronously.
    ///
    /// [`futures_channel::oneshot::channel`]is used to await on the result.
    ///
    /// [futures_channel::oneshot::channel]: https://docs.rs/futures-channel/0.3.5/futures_channel/oneshot/index.html
    #[cfg(feature = "with-async")]
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    pub async fn execute_async<F, R>(&self, job: F) -> Result<R, ThreadPoolError>
    where
        F: FnOnce() -> R + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = futures_channel::oneshot::channel();

        self.execute(Box::new(move || {
            let _ = tx.send(job());
        }))?;

        Ok(rx.await?)
    }

    /// Close the pool and return `true` if this call is successful
    pub fn close(&self) -> bool {
        self.inner.close()
    }

    /// Return a state of the pool.
    pub fn state(&self) -> ThreadPoolState {
        let name = self.inner.name();
        let (active_threads, max_threads) = self.inner.thread_count();

        ThreadPoolState {
            name,
            active_threads,
            max_threads,
        }
    }
}

impl From<Builder> for ThreadPool {
    fn from(builder: Builder) -> Self {
        let (tx, rx) = channel();

        let inner = ThreadPoolInner::new(builder, rx);

        let pool = ThreadPool {
            tx,
            inner: Arc::new(inner),
        };

        let inner = &pool.inner;

        let min_idle = inner.min_idle_count();

        (0..min_idle).for_each(|_| {
            inner.spawn_thread(&inner);
        });

        pool
    }
}

impl Clone for ThreadPool {
    /// Cloning a pool will create a new instance to the pool.
    fn clone(&self) -> ThreadPool {
        ThreadPool {
            tx: self.tx.clone(),
            inner: self.inner.clone(),
        }
    }
}

pub struct ThreadPoolState<'a> {
    pub name: Option<&'a str>,
    pub active_threads: usize,
    pub max_threads: usize,
}

impl Debug for ThreadPool {
    fn fmt(&self, f: &mut Formatter) -> FmtResult {
        self.state().fmt(f)
    }
}

impl Debug for ThreadPoolState<'_> {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        f.debug_struct("ThreadPoolState")
            .field("name", &self.name)
            .field("active_threads", &self.active_threads)
            .field("max_threads", &self.max_threads)
            .finish()
    }
}
