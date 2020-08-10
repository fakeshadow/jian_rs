use core::time::Duration;

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
    assert_eq!(state.name, Some("test-pool-worker"));
}

#[test]
fn recycle() {
    let pool = ThreadPool::builder()
        .max_threads(12)
        .min_threads(3)
        .thread_name("test-pool")
        .idle_timeout(Duration::from_millis(300))
        .build();

    (0..1024).for_each(|_| {
        let _ = pool.execute(|| {
            thread::sleep(Duration::from_nanos(1));
        });
    });

    let state = pool.state();
    assert_eq!(12, state.active_threads);

    thread::sleep(Duration::from_secs(4));

    // threads have been idle for 3 seconds so we go back to min_threads.

    let state = pool.state();
    assert_eq!(3, state.active_threads);
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
