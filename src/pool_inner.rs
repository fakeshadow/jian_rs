use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use cache_padded::CachePadded;
use concurrent_queue::{ConcurrentQueue, PopError, PushError};
use parking_lot::Mutex;

use crate::builder::Builder;
use crate::thread_parking::{ThreadParker, ThreadUnparker};

pub(crate) struct ThreadPoolInner {
    name: Option<String>,

    // a MPMC queue for storing jobs.
    jobs: ConcurrentQueue<Box<dyn FnOnce() + Send + 'static>>,

    // a lock for spawned threads and the unparker of them for wake up and unblock them.
    unparker: Mutex<Vec<ThreadUnparker>>,

    // Current active threads count.
    active_threads: CachePadded<AtomicUsize>,

    // A state where 0 means there is no parked thread and 1 as opposite
    have_park: CachePadded<AtomicUsize>,

    // A counter indicate how many workers are in queue.
    work_count: CachePadded<AtomicUsize>,

    stack_size: Option<usize>,

    // a thread would exit if it parked for too long
    park_timeout: Duration,

    min_threads: usize,
    max_threads: usize,
}

impl From<Builder> for ThreadPoolInner {
    fn from(builder: Builder) -> Self {
        let max_threads = builder.max_threads.unwrap_or_else(num_cpus::get);

        ThreadPoolInner {
            name: builder.thread_name,
            jobs: ConcurrentQueue::unbounded(),
            unparker: Mutex::new(Vec::with_capacity(max_threads)),
            active_threads: CachePadded::new(AtomicUsize::new(0)),
            have_park: CachePadded::new(AtomicUsize::new(0)),
            work_count: CachePadded::new(AtomicUsize::new(0)),
            stack_size: builder.thread_stack_size,
            park_timeout: builder.idle_timeout,
            min_threads: builder.min_threads.unwrap_or(0),
            max_threads,
        }
    }
}

pub(crate) type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPoolInner {
    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn min_idle_count(&self) -> usize {
        self.min_threads
    }

    pub(crate) fn stack_size(&self) -> Option<usize> {
        self.stack_size
    }

    pub(crate) fn park_timeout(&self) -> Duration {
        self.park_timeout
    }

    pub(crate) fn push<F>(&self, job: F) -> Result<(), PushError<Job>>
    where
        F: FnOnce() + Send + 'static,
    {
        self.jobs.push(Box::new(job))
    }

    pub(crate) fn pop(&self) -> Result<Job, PopError> {
        self.jobs.pop().map(|job| {
            self.dec_work_count();
            job
        })
    }

    pub(crate) fn close(&self) -> bool {
        self.jobs.close()
    }

    // increment thread count and return false if we already hit max pool size.
    pub(crate) fn inc_thread_count(&self) -> bool {
        let mut active = self.active_threads.load(Ordering::Relaxed);

        loop {
            if active == self.max_threads {
                return false;
            }

            match self.active_threads.compare_exchange_weak(
                active,
                active + 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(a) => active = a,
            }
        }
    }

    // return if we should spawn new with this decrement.
    pub(crate) fn dec_thread_count(&self) -> bool {
        let count = self.active_threads.fetch_sub(1, Ordering::AcqRel);

        ((count - 1) < self.min_threads) && !self.jobs.is_closed()
    }

    pub(crate) fn inc_work_count(&self) -> usize {
        self.work_count.fetch_add(1, Ordering::Acquire)
    }

    fn dec_work_count(&self) {
        self.work_count.fetch_sub(1, Ordering::Release);
    }

    pub(crate) fn have_park(&self) {
        self.have_park.fetch_or(1, Ordering::Acquire);
    }

    pub(crate) fn thread_count(&self) -> (usize, usize) {
        let active = self.active_threads.load(Ordering::Relaxed);

        (active, self.max_threads)
    }

    pub(crate) fn new_parking(&self, dur: Duration) -> ThreadParker {
        let (u, p) = crate::thread_parking::pair(dur);
        self.unparker.lock().push(u);
        p
    }

    pub(crate) fn remove_parking(&self, parker: &ThreadParker) -> &Self {
        let mut guard = self.unparker.lock();

        guard.retain(|unparker| unparker.id() != parker.id());

        self
    }

    pub(crate) fn notify_parking(&self, parker: &ThreadParker) -> bool {
        let mut guard = self.unparker.lock();

        for unparker in guard.iter_mut() {
            if unparker.id() == parker.id() {
                // We only change the state to park.
                // The real park can only be called with parker and not unparker.
                unparker.set_park();
                return true;
            }
        }

        false
    }

    pub(crate) fn try_unpark_one(&self) {
        if self.have_park.load(Ordering::Acquire) != 0 {
            self.unpark_one();
        }
    }

    pub(crate) fn unpark_one(&self) {
        let mut guard = self.unparker.lock();

        for unparker in guard.iter_mut() {
            if unparker.try_unpark() {
                return;
            }
        }

        // We have no parker in parking state.
        self.have_park.store(0, Ordering::Release);
    }

    pub(crate) fn unpark_all(&self) {
        let mut guard = self.unparker.lock();

        for unparker in guard.iter_mut() {
            unparker.try_unpark();
        }

        self.have_park.store(0, Ordering::Release);
    }
}
