# batch-lock

[![Rust](https://github.com/SF-Zhou/batch-lock/actions/workflows/rust.yml/badge.svg)](https://github.com/SF-Zhou/batch-lock/actions/workflows/rust.yml)
[![Crates.io Version](https://img.shields.io/crates/v/batch-lock)](https://crates.io/crates/batch-lock)
[![codecov](https://codecov.io/gh/SF-Zhou/batch-lock/graph/badge.svg?token=A8J0IYMX7A)](https://codecov.io/gh/SF-Zhou/batch-lock)

A thread-safe, key-based lock management system for Rust that provides fine-grained concurrent access control.

## Features

- 🔒 Thread-safe key-based locking mechanism
- 🚀 High-performance concurrent implementation using `dashmap`
- 📦 Support for both single-key and batch locking operations
- 🛡️ RAII-style lock guards for automatic lock release
- ⚡ Zero-cost abstractions with minimal overhead
- 🔧 Configurable capacity and sharding for optimal performance

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
lock_manager = "0.1"
```

## Usage

### Single Key Locking

```rust
use batch_lock::LockManager;

// Create a new lock manager
let lock_manager = LockManager::<String>::new();

// Acquire a lock
let guard = lock_manager.lock("resource_1".to_string());

// Critical section - exclusive access guaranteed
// perform operations...

// Lock is automatically released when guard is dropped
```

### Batch Locking

```rust
use batch_lock::LockManager;
use std::collections::BTreeSet;

// Create a new lock manager with custom capacity
let lock_manager = LockManager::<String>::with_capacity(1000);

// Prepare multiple keys
let mut keys = BTreeSet::new();
keys.insert("resource_1".to_string());
keys.insert("resource_2".to_string());

// Acquire locks for all keys atomically
let guard = lock_manager.batch_lock(keys);

// Critical section - exclusive access to all keys guaranteed
// perform operations...

// All locks are automatically released when guard is dropped
```

### Custom Capacity and Sharding

```rust
use batch_lock::LockManager;

// Create a lock manager with custom capacity and shard count
let lock_manager = LockManager::<String>::with_capacity_and_shard_amount(
    1000,  // capacity
    16     // number of shards
);
```

## Performance Considerations

- Uses `dashmap` for efficient concurrent access
- Lock acquisition is fair and FIFO-ordered
- Batch locks are acquired in a consistent order to prevent deadlocks
- Memory usage scales with the number of active locks
- Sharding can be tuned for optimal performance based on workload

## Thread Safety

The `LockManager` is fully thread-safe and can be safely shared across threads:

```rust
use std::sync::Arc;
use batch_lock::LockManager;

let lock_manager = Arc::new(LockManager::<String>::new());
let lock_manager_clone = lock_manager.clone();

std::thread::spawn(move || {
    let guard = lock_manager_clone.lock("shared_resource".to_string());
    // Critical section
});
```

## API Documentation

For detailed API documentation, please visit [docs.rs/batch-lock](https://docs.rs/batch-lock).

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

## License

This project is licensed under MIT or Apache-2.0.
