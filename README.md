# jian_rs

Jian is a dynamic scaling thread pool that would adjust size with work load and would clean up idle threads.

Original code from [rust-threadpool](https://github.com/rust-threadpool/rust-threadpool/)

# Example:
```rust
use core::time::Duration;

let pool = jian_rs::ThreadPool::builder().build();
let _ = pool.execute(|| {
    println!("some code");
});
```
