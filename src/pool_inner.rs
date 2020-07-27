use core::sync::atomic::{AtomicUsize, Ordering};
use core::time::Duration;

use std::sync::{mpsc::Receiver, Mutex};

use crate::builder::Builder;
use crate::ThreadPoolError;

pub(crate) struct ThreadPoolInner {
    name: Option<String>,
    // a MPSC channel receiver for storing jobs.
    rx: Mutex<Receiver<Job>>,
    // 3 counters packed in one usize. With the following bit flag:
    // { work: 0, is_closed:0, active_threads: 0 }
    // active_threads would be in a range from 0 - max_threads.
    // is_closed would either be 1(closed) or 0(not closed).
    // work_count would start from 0 and inc/dec with the jobs queue's push/pop
    counter: AtomicUsize,

    close_bit: usize,

    // work unit.
    one_work: usize,
    stack_size: Option<usize>,
    recv_timeout: Option<Duration>,
    min_threads: usize,
    max_threads: usize,
}

pub(crate) type Job = Box<dyn FnOnce() + Send + 'static>;

impl ThreadPoolInner {
    pub(crate) fn new(builder: Builder, rx: Receiver<Job>) -> Self {
        let max_threads = builder.max_threads.unwrap_or_else(num_cpus::get);

        assert!(max_threads < core::usize::MAX / 2, "Too many threads");

        // compute one_work unit.
        let close_bit = (max_threads + 1).next_power_of_two();
        let one_work = close_bit * 2;

        let recv_timeout = if builder.idle_timeout == Duration::from_secs(0) {
            None
        } else {
            Some(builder.idle_timeout)
        };

        ThreadPoolInner {
            name: builder.thread_name,
            rx: Mutex::new(rx),
            counter: AtomicUsize::new(0),
            close_bit,
            one_work,
            stack_size: builder.thread_stack_size,
            recv_timeout,
            min_threads: builder.min_threads,
            max_threads,
        }
    }

    pub(crate) fn recv(&self) -> Result<Job, ThreadPoolError> {
        let rx = self.rx.lock().unwrap();

        Ok(match self.recv_timeout {
            Some(dur) => rx.recv_timeout(dur).map(|job| {
                self.dec_work_count();
                job
            })?,
            None => rx.recv().map(|job| {
                self.dec_work_count();
                job
            })?,
        })
    }

    pub(crate) fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    pub(crate) fn min_idle_count(&self) -> usize {
        self.min_threads
    }

    pub(crate) fn stack_size(&self) -> Option<usize> {
        self.stack_size
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
        let counter = self.counter.fetch_sub(1, Ordering::Release);

        let active = self.active_count(counter);
        let is_open = self.is_open(counter);

        ((active - 1) < self.min_threads) && is_open
    }

    // return true if we are able to drop the worker without going below min_idle.
    pub(crate) fn can_drop_idle(&self) -> bool {
        let counter = self.counter.load(Ordering::SeqCst);

        self.active_count(counter) > self.min_threads
    }

    // We return if we should spawn new thread.
    pub(crate) fn inc_work_count(&self) -> bool {
        let counter = self.counter.fetch_add(self.one_work, Ordering::Relaxed);

        let active = self.active_count(counter);
        let work = self.work_count(counter);

        work > 0 && active < self.max_threads
    }

    fn dec_work_count(&self) {
        self.counter.fetch_sub(self.one_work, Ordering::Relaxed);
    }

    pub(crate) fn thread_count(&self) -> (usize, usize) {
        let counter = self.counter.load(Ordering::Relaxed);

        (self.active_count(counter), self.max_threads)
    }

    // close the pool inner and return true if this call is a success.
    pub(crate) fn close(&self) -> bool {
        let counter = self.counter.fetch_or(self.close_bit, Ordering::SeqCst);

        self.is_open(counter)
    }
}

impl ThreadPoolInner {
    fn active_count(&self, counter: usize) -> usize {
        counter & (self.close_bit - 1)
    }

    fn work_count(&self, counter: usize) -> usize {
        counter & !(self.one_work - 1)
    }

    fn is_open(&self, counter: usize) -> bool {
        counter & self.close_bit == 0
    }
}
