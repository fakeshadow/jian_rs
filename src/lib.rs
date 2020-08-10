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
//! pool.execute(|| {
//!     println!("some code");
//! });
//! ```
#![forbid(unsafe_code)]

pub(crate) mod builder;
pub(crate) mod error;
pub(crate) mod pool;
pub(crate) mod pool_inner;

pub use crate::{builder::Builder, error::ThreadPoolError, pool::ThreadPool};
