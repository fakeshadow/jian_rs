use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;

#[cfg(feature = "with-async")]
use {
    core::sync::atomic::AtomicUsize,
    futures::stream::{FuturesUnordered, StreamExt},
};

use std::sync::Arc;
use std::thread;

use jian_rs::ThreadPool;

#[test]
fn init() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(7)
        .thread_name("test-pool")
        .idle_timeout(Duration::from_secs(5))
        .build();

    let state = pool.state();

    assert_eq!(state.active_threads, 7);
    assert_eq!(state.max_threads, 12);
    assert_eq!(state.name, Some("test-pool"));
}

#[cfg(feature = "with-async")]
#[actix_rt::test]
async fn async_work() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(1)
        .thread_name("test-pool")
        .build();

    let counter = Arc::new(AtomicUsize::new(0));

    let blocked = Arc::new(AtomicBool::new(false));

    let futs = (0..1024)
        .fold(FuturesUnordered::new(), |f, _| {
            let counter = counter.clone();
            let pool = pool.clone();

            f.push(async move {
                let _ = pool
                    .execute_async(move || {
                        thread::sleep(Duration::from_millis(1));
                        counter.fetch_add(1, Ordering::Release);
                        1u8
                    })
                    .await;
            });

            f
        })
        .collect::<Vec<_>>();

    let blocked2 = blocked.clone();

    // spawn an async task to see if we are blocked by the thread pool.
    actix_rt::spawn(async move {
        let _ = futs.await;
        actix_rt::time::delay_for(Duration::from_millis(1)).await;
        // after all pool::execute_async we write blocked to true.
        blocked2.store(true, Ordering::Release);
    });

    actix_rt::time::delay_for(Duration::from_millis(60)).await;
    // we are still false so we are not blocked by thread pool
    assert_eq!(false, blocked.load(Ordering::Acquire));

    actix_rt::time::delay_for(Duration::from_millis(60)).await;
    // thread pool finished and we have true.
    assert_eq!(true, blocked.load(Ordering::Acquire));

    actix_rt::time::delay_for(Duration::from_millis(500)).await;

    // all pool::execute_async are success
    assert_eq!(1024, counter.load(Ordering::Acquire));

    let state = pool.state();

    // check the max thread count
    assert_eq!(12, state.active_threads);
}

#[cfg(feature = "with-async")]
#[actix_rt::test]
async fn async_sequential() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(1)
        .thread_name("test-pool")
        .build();

    let counter = Arc::new(AtomicUsize::new(0));

    for _ in 0..1024 {
        let counter = counter.clone();
        let _ = pool
            .execute_async(move || {
                std::thread::sleep(Duration::from_millis(1));
                counter.fetch_add(1, Ordering::Release);
                1u8
            })
            .await;
    }

    assert_eq!(1024, counter.load(Ordering::Acquire));

    let state = pool.state();

    // chained async/await are sequential so we remain on one thread.
    assert_eq!(1, state.active_threads);
}

#[test]
fn recycle() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(3)
        .thread_name("test-pool")
        .idle_timeout(Duration::from_secs(2))
        .build();

    (0..128).for_each(|_| {
        let _ = pool.execute(|| {
            thread::sleep(Duration::from_millis(1));
        });
    });

    let state = pool.state();
    assert_eq!(12, state.active_threads);

    thread::sleep(Duration::from_secs(3));

    // threads have been idle for 3 seconds so we go back to min_threads.

    let state = pool.state();
    assert_eq!(3, state.active_threads);
}

#[test]
fn close() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(3)
        .thread_name("test-pool")
        .build();

    (0..128).for_each(|_| {
        let _ = pool.execute(|| {
            thread::sleep(Duration::from_millis(1));
        });
    });

    pool.close();

    thread::sleep(Duration::from_secs(1));

    // close the pool would notify all threads to unpark and exit.

    let state = pool.state();
    assert_eq!(0, state.active_threads);

    let executed = Arc::new(AtomicBool::new(false));

    let executed_clone = executed.clone();

    let res = pool.execute(move || {
        executed_clone.store(true, Ordering::SeqCst);
    });

    assert_eq!(false, executed.load(Ordering::SeqCst));

    // job would return in error if the pool is closed.
    if let Err(e) = res {
        let job = e.into_inner().unwrap();

        job();
    }

    assert_eq!(true, executed.load(Ordering::SeqCst));
}

#[test]
fn panic_recover() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(3)
        .thread_name("test-pool")
        .build();

    let _ = pool.execute(|| {
        panic!("This is a on purpose panic for testing panic recovery");
    });

    thread::sleep(Duration::from_millis(100));

    // we spawn new thread when a panic happen(if we are going below min_threads)
    let state = pool.state();
    assert_eq!(3, state.active_threads);

    (0..128).for_each(|_| {
        let _ = pool.execute(|| {
            thread::sleep(Duration::from_millis(1));
        });
    });

    let _ = pool.execute(|| {
        panic!("This is a on purpose panic for testing panic recovery");
    });

    thread::sleep(Duration::from_millis(100));

    // We didn't try to spawn new thread after previous panic
    // because we are still above the min_threads count
    let state = pool.state();
    assert_eq!(11, state.active_threads);
}

#[test]
fn no_eager_spawn() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .thread_name("test-pool")
        .build();

    let state = pool.state();
    assert_eq!(0, state.active_threads);

    let _a = pool.execute(|| {
        thread::sleep(Duration::from_millis(1));
    });

    thread::sleep(Duration::from_millis(100));

    let _a = pool.execute(|| {
        thread::sleep(Duration::from_millis(1));
    });

    thread::sleep(Duration::from_millis(100));

    let state = pool.state();

    assert_eq!(1, state.active_threads);
}
