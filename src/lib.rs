// Copyright 2014 The Rust Project Developers. See the COPYRIGHT
// file at the top-level directory of this distribution and at
// http://rust-lang.org/COPYRIGHT.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Jian is a dynamic scaling thread pool that would adjust size with work load and would clean up
//! idle threads.
//!
//! Original code from [rust-threadpool](https://github.com/rust-threadpool/rust-threadpool/)
//!
//! # Example:
//! ```rust
//! use core::time::Duration;
//!
//! use jian_rs::{ThreadPool, ThreadPoolError};
//!
//! let pool = ThreadPool::builder().build();
//!
//! let result: Result<(), ThreadPoolError> = pool.execute(|| {
//!     println!("some code");
//! });
//! ```
//!
//! # Features
//! | Feature | Description | Extra dependencies | Default |
//! | ------- | ----------- | ------------------ | ------- |
//! | `default` | None | [cache-padded](https://crates.io/crates/cache-padded)<br>[concurrent-queue](https://crates.io/crates/concurrent-queue)<br>[num_cpus](https://crates.io/crates/num_cpus)<br>[parking](https://crates.io/crates/parking)<br>[parking_lot](https://crates.io/crates/parking_lot) | yes |
//! | `with-async` | Enable await on execute result asynchronously. | [futures-channel](https://crates.io/crates/futures-channel) | yes |

pub(crate) mod builder;
pub(crate) mod error;
pub(crate) mod pool;
pub(crate) mod pool_inner;
pub(crate) mod thread_parking;

pub use crate::{builder::Builder, error::ThreadPoolError, pool::ThreadPool};
