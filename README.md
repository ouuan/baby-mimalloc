# baby-mimalloc

[![Crates.io Version](https://shields.ouuan.moe/crates/v/baby-mimalloc)](https://crates.io/crates/baby-mimalloc)
[![docs.rs](https://shields.ouuan.moe/docsrs/baby-mimalloc)](https://docs.rs/baby-mimalloc/latest/baby_mimalloc/)

[Mimalloc](https://github.com/microsoft/mimalloc) implemented in Rust (not a binding to the C library) with only basic features.

Lock-free multi-threading, security features, and some performance enhancements are not implemented.

It can be used in `no_std` environments.

## Features

- **mmap** - Provide `MimallocMmap` that uses `mmap` as OS allocator for segments.
- **std_mutex** - Provide `MimallocMutexWrapper` that wraps `Mimalloc` inside `std::sync::Mutex` and implements `GlobalAlloc`.
- **spin_mutex** - Provide `MimallocMutexWrapper` that wraps `Mimalloc` inside `spin::Mutex` that can be used in `no_std` environments.
- **deferred_free** - Enable registering a hook to complete deferred free events. See the documentation of [`mi_register_deferred_free`](https://microsoft.github.io/mimalloc/group__extended.html#ga3460a6ca91af97be4058f523d3cb8ece).

## Usage

```toml
[dependencies]
baby-mimalloc = { version = "*", features = ["mmap", "std_mutex"] }
# baby-mimalloc = { version = "*", features = ["mmap", "spin_mutex"] }
```

```rust
use baby_mimalloc::{new_mimalloc_mmap_mutex, MimallocMmapMutex};

#[global_allocator]
static ALLOCATOR: MimallocMmapMutex = new_mimalloc_mmap_mutex();
```
