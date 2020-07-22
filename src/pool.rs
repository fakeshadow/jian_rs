use core::fmt::{Debug, Formatter, Result as FmtResult};

use std::sync::Arc;
use std::thread;

use concurrent_queue::PopError;

use crate::builder::Builder;
use crate::pool_inner::ThreadPoolInner;
use crate::thread_parking::ThreadParker;
use crate::ThreadPoolError;

/// Abstraction of a thread pool for basic parallelism.
pub struct ThreadPool {
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
        let inner = &*self.inner;

        // push job to queue.
        inner.push(Box::new(job))?;

        if inner.inc_work_count() > 0 {
            self.spawn_thread();
        }

        inner.try_unpark_one();

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
            if !tx.is_canceled() {
                let _ = tx.send(job());
            }
        }))?;

        Ok(rx.await?)
    }

    /// Close the pool and return `true` if this call is successful
    pub fn close(&self) -> bool {
        let did_close = self.inner.close();

        self.inner.unpark_all();

        did_close
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

impl Drop for ThreadPool {
    fn drop(&mut self) {
        while let Ok(job) = self.inner.pop() {
            // ToDo: for now if we panic then the drop would be interrupted
            job();
        }
    }
}

impl From<Builder> for ThreadPool {
    fn from(builder: Builder) -> Self {
        let inner = Arc::new(builder.into());

        let pool = ThreadPool { inner };

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

        let did_inc = inner.inc_thread_count();

        if did_inc {
            let mut builder = thread::Builder::new();
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
                    inner.dec_thread_count();
                    panic!("Failed to spawn new thread");
                });
        }

        did_inc
    }
}

// A worker of the pool.
struct Worker {
    pool: ThreadPool,
    parker: ThreadParker,
}

impl Worker {
    fn handle_job(self) {
        let inner = &*self.pool.inner;
        let parker = &self.parker;

        loop {
            match inner.pop() {
                Ok(job) => {
                    job();
                }
                Err(e) => match e {
                    // We don't need a graceful shut down as the queue is already closed AND empty
                    PopError::Closed => break,
                    PopError::Empty => {
                        // If we for some reason lost the unparker then we must exit before we
                        // parking.
                        if !inner.notify_parking(parker) {
                            break;
                        };

                        inner.have_park();

                        // park and wait for notify or timeout
                        if parker.park_timeout() {
                            // We are unparking because of timeout. try to pop job one more time and
                            // exit on error
                            match inner.pop() {
                                Ok(job) => {
                                    job();
                                    // We got job again so continue the loop.
                                }
                                Err(_) => break,
                            }
                        }
                    }
                },
            }
        }
    }
}

impl From<ThreadPool> for Worker {
    fn from(pool: ThreadPool) -> Self {
        let inner = &*pool.inner;

        // It's important we construct worker in newly spawned thread.
        let parker = inner.new_parking(inner.park_timeout());

        Worker { pool, parker }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let pool = &self.pool;
        let parker = &self.parker;

        let should_spawn = pool.inner.remove_parking(parker).dec_thread_count();

        if should_spawn {
            pool.spawn_thread();
        }
    }
}

impl Clone for ThreadPool {
    /// Cloning a pool will create a new instance to the pool.
    fn clone(&self) -> ThreadPool {
        ThreadPool {
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
