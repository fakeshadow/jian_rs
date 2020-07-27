use core::fmt::{Debug, Formatter, Result as FmtResult};

use std::sync::{
    mpsc::{channel, RecvTimeoutError, Sender},
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
        // send job through channel.
        self.tx.send(Box::new(job))?;

        // read from the previous state when we increment work count
        let should_spawn = self.inner.inc_work_count();

        if should_spawn {
            self.spawn_thread();
        }

        Ok(())
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
    #[deprecated(note = "Due to code rework ThreadPool::close can not function properly for now")]
    pub fn close(&self) -> bool {
        false
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

        let min_idle = pool.inner.min_idle_count();

        (0..min_idle).for_each(|_| {
            pool.spawn_thread();
        });

        pool
    }
}

impl ThreadPool {
    // spawn new thread and return true if we successfully did that.
    fn spawn_thread(&self) -> bool {
        let inner = &*self.inner;

        let did_inc = inner.inc_active_thread();

        if did_inc {
            let mut builder = std::thread::Builder::new();
            if let Some(name) = inner.name() {
                builder = builder.name(format!("{}-worker", name));
            }
            if let Some(stack_size) = inner.stack_size() {
                builder = builder.stack_size(stack_size);
            }

            let pool = self.clone();

            builder
                .spawn(move || Worker::from(pool).handle_job())
                .unwrap_or_else(|_| {
                    inner.dec_active_thread();
                    panic!("Failed to spawn new thread");
                });
        }

        did_inc
    }
}

// A worker of the pool.
struct Worker {
    pool: ThreadPool,
}

impl Worker {
    fn handle_job(self) {
        let inner = &*self.pool.inner;

        loop {
            match inner.recv_timeout() {
                Ok(job) => {
                    job();
                }
                Err(e) => match e {
                    RecvTimeoutError::Disconnected => break,
                    RecvTimeoutError::Timeout => {
                        if inner.can_drop_idle() {
                            break;
                        }
                    }
                },
            }
        }
    }
}

impl From<ThreadPool> for Worker {
    fn from(pool: ThreadPool) -> Self {
        Worker { pool }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let pool = &self.pool;

        let should_spawn = pool.inner.dec_active_thread();

        if should_spawn {
            pool.spawn_thread();
        }
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
