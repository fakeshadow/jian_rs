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
    jobs: ConcurrentQueue<Job>,

    // a lock for spawned threads and the unparker of them for wake up and unblock them.
    unparker: Mutex<Vec<ThreadUnparker>>,

    // 3 counters packed in one usize. With the following bit flag:
    // { work: 0, have_park: 0, active_threads: 0 }
    // active_threads would be in a range from 0 - max_threads.
    // have_park would be either 0 (no thread is parking) or 1(at least one thread is parking)
    // work_count would start from 0 and inc/dec with the jobs queue's push/pop
    counter: CachePadded<AtomicUsize>,

    // bitwise marker for have_park.
    park_marker: usize,

    // work unit.
    one_work: usize,

    stack_size: Option<usize>,

    // a thread would exit if it parked for too long
    park_timeout: Duration,

    min_threads: usize,
    max_threads: usize,
}

impl From<Builder> for ThreadPoolInner {
    fn from(builder: Builder) -> Self {
        let max_threads = builder.max_threads.unwrap_or_else(num_cpus::get);

        assert!(
            max_threads < (core::usize::MAX >> 1) / 2,
            "Too many threads"
        );

        // leave the last bit for have_park.
        // marker is used for bitwise operation for work and active_threads
        let park_marker = (max_threads + 1).next_power_of_two();

        // compute one_work unit and shift.
        let one_work = park_marker * 2;

        ThreadPoolInner {
            name: builder.thread_name,
            jobs: ConcurrentQueue::unbounded(),
            unparker: Mutex::new(Vec::with_capacity(max_threads)),
            counter: CachePadded::new(AtomicUsize::new(0)),
            park_marker,
            one_work,
            stack_size: builder.thread_stack_size,
            park_timeout: builder.idle_timeout,
            min_threads: builder.min_threads,
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
    pub(crate) fn inc_active_thread(&self) -> bool {
        let mut counter = self.counter.load(Ordering::Relaxed);

        loop {
            if self.active_count(counter) == self.max_threads {
                return false;
            }

            match self.counter.compare_exchange_weak(
                counter,
                counter + 1,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(a) => counter = a,
            }
        }
    }

    // return true if we should spawn new with this decrement.
    pub(crate) fn dec_active_thread(&self) -> bool {
        let counter = self.counter.fetch_sub(1, Ordering::AcqRel);

        let active = self.active_count(counter);

        ((active - 1) < self.min_threads) && !self.jobs.is_closed()
    }

    // return true if we are able to drop the worker without going below min_idle.
    pub(crate) fn can_drop_idle(&self) -> bool {
        let counter = self.counter.load(Ordering::Relaxed);

        self.active_count(counter) > self.min_threads
    }

    // We return 2 booleans:
    // if we should spawn new thread.
    // if we have parked threads.
    pub(crate) fn inc_work_count(&self) -> (bool, bool) {
        let counter = self.counter.fetch_add(self.one_work, Ordering::Relaxed);

        // ToDo: this is likely not going to happen so it's left comment for now.
        // assert!(count < (core::usize::MAX >> 1) / 2, "Too many work");

        let active = self.active_count(counter);
        let work = self.work_count(counter);
        let have_parker = self.have_parker(counter);

        (work > 0 && active < self.max_threads, have_parker)
    }

    fn dec_work_count(&self) {
        self.counter.fetch_sub(self.one_work, Ordering::Relaxed);
    }

    pub(crate) fn set_park_marker(&self) {
        self.counter.fetch_or(self.park_marker, Ordering::Release);
    }

    pub(crate) fn thread_count(&self) -> (usize, usize) {
        let counter = self.counter.load(Ordering::Relaxed);

        (self.active_count(counter), self.max_threads)
    }

    pub(crate) fn new_parking(&self) -> ThreadParker {
        let (u, p) = crate::thread_parking::pair(self.park_timeout);
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

    pub(crate) fn unpark_one(&self) {
        let mut guard = self.unparker.lock();

        for unparker in guard.iter_mut() {
            if unparker.try_unpark() {
                return;
            }
        }

        // We have no parker in parking state.
        self.no_parker();
    }

    pub(crate) fn unpark_all(&self) {
        let mut guard = self.unparker.lock();

        for unparker in guard.iter_mut() {
            unparker.try_unpark();
        }

        self.no_parker();
    }
}

impl ThreadPoolInner {
    fn active_count(&self, counter: usize) -> usize {
        counter & (self.park_marker - 1)
    }

    fn work_count(&self, counter: usize) -> usize {
        counter & !(self.park_marker - 1)
    }

    fn have_parker(&self, counter: usize) -> bool {
        counter & self.park_marker != 0
    }

    fn no_parker(&self) {
        let unpark_marker = !self.park_marker;
        self.counter.fetch_and(unpark_marker, Ordering::Relaxed);
    }
}
