#![feature(test)]

extern crate test;

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use test::Bencher;
use threadpool::{Builder, ThreadPool};

fn pool() -> &'static Mutex<ThreadPool> {
    static POOL: OnceCell<Mutex<ThreadPool>> = OnceCell::new();
    POOL.get_or_init(|| {
        let cpu = num_cpus::get() * 5;
        let pool = Builder::new().num_threads(cpu).build();

        Mutex::new(pool)
    })
}

#[bench]
fn single_thread(b: &mut Bencher) {
    bench_inner(b, false, 100, None);
}

#[bench]
fn single_thread_1_micro_task(b: &mut Bencher) {
    bench_inner(b, false, 20, Some(Duration::from_micros(1)));
}

#[bench]
fn multi_threads(b: &mut Bencher) {
    bench_inner(b, true, 5, None);
}

#[bench]
fn multi_threads_1_micro_task(b: &mut Bencher) {
    bench_inner(b, true, 2, Some(Duration::from_micros(1)));
}

fn bench_inner(b: &mut Bencher, multi_thread: bool, round: usize, delay: Option<Duration>) {
    let cpu = num_cpus::get();

    let max = if multi_thread { cpu } else { 1 };

    b.iter(move || {
        (0..max)
            .map(move |_| {
                std::thread::spawn(move || {
                    thread_local! {
                        static POOL_LOCAL: ThreadPool = pool().lock().clone();
                    }

                    actix_rt::System::new("threadpool").block_on(async move {
                        let result = Arc::new(AtomicUsize::new(0));

                        let result1 = result.clone();
                        let futures = (0..cpu * round)
                            .map(move |_| {
                                let result1 = result1.clone();
                                let (tx, rx) = futures::channel::oneshot::channel();
                                POOL_LOCAL.with(|pool| {
                                    pool.execute(move || {
                                        if let Some(delay) = delay {
                                            std::thread::sleep(delay);
                                        }
                                        result1.fetch_add(1, Ordering::SeqCst);
                                        let _ = tx.send(());
                                    });
                                });

                                rx
                            })
                            .collect::<Vec<_>>();

                        let _ = futures::future::join_all(futures).await;

                        assert_eq!(cpu * round, result.load(Ordering::SeqCst));
                    });
                })
            })
            .for_each(|handler| handler.join().unwrap());
    });
}
