# batch-lock

[![Rust](https://github.com/SF-Zhou/batch-lock/actions/workflows/rust.yml/badge.svg)](https://github.com/SF-Zhou/batch-lock/actions/workflows/rust.yml)
[![Crates.io Version](https://img.shields.io/crates/v/batch-lock)](https://crates.io/crates/batch-lock)
[![codecov](https://codecov.io/gh/SF-Zhou/batch-lock/graph/badge.svg?token=A8J0IYMX7A)](https://codecov.io/gh/SF-Zhou/batch-lock)

A lock manager with batch-lock support.

## Usage

To use this library, add the following to your Cargo.toml:

```toml
[dependencies]
batch-lock = "0.1"
```

Then, you can import and use the components as follows:

```rust
use batch_lock::LockManager;

let lock_manager = LockManager::new();
let lock_guard = lock_manager.lock(1);
// key = 1 is locked.
drop(lock_guard);

use std::collections::BTreeSet;
let batch_keys = [2, 3, 5, 7].into_iter().collect::<BTreeSet<_>>();
let batch_lock_guard = lock_manager.batch_lock(batch_keys);
// 2, 3, 5, 7 are locked in batch.
drop(batch_lock_guard);
```
