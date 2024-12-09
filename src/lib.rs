/*
 * Copyright 2024 Yufan You
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! [Mimalloc](https://github.com/microsoft/mimalloc) implemented in Rust
//! (not a binding to the C library) with only basic features.
//!
//! Lock-free multi-threading, security features, and some performance enhancements are not
//! implemented.
//!
//! It can be used in `no_std` environments.
//!
//! # Crate Features
//!
//! - **mmap** - Provide [`MimallocMmap`] that uses `mmap` as OS allocator for segments.
//! - **std_mutex** - Provide [`MimallocMutexWrapper`] that wraps [`Mimalloc`] inside
//!   [`std::sync::Mutex`] and implements [`GlobalAlloc`].
//! - **spin_mutex** - Provide [`MimallocMutexWrapper`] that wraps [`Mimalloc`] inside
//!   [`spin::Mutex`] that can be used in `no_std` environments.
//! - **deferred_free** - Enable registering a hook to complete deferred free events.
//!   See the documentation of [`mi_register_deferred_free`](https://microsoft.github.io/mimalloc/group__extended.html#ga3460a6ca91af97be4058f523d3cb8ece).

#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![cfg_attr(not(feature = "std"), no_std)]

mod constants;
mod heap;
mod list;
mod page;
mod segment;
mod utils;

use core::alloc::{GlobalAlloc, Layout};
use heap::Heap;

/* wrapper around `heap::Heap` to defined the public API. */

/// The main allocator object.
///
/// `A` is the type of the OS allocator for segments.
///
/// To use it as the [`global_allocator`], wrap it inside a lock and implement [`GlobalAlloc`].
/// See [`MimallocMutexWrapper`].
#[derive(Default)]
pub struct Mimalloc<A: GlobalAlloc> {
    heap: Heap,
    os_alloc: A,
    #[cfg(feature = "deferred_free")]
    deferred_free_hook: Option<DeferredFreeHook<A>>,
}

#[cfg(feature = "deferred_free")]
pub mod deferred_free;
#[cfg(feature = "deferred_free")]
use deferred_free::*;

unsafe impl<A: GlobalAlloc> Send for Mimalloc<A> {}

impl<A: GlobalAlloc> Mimalloc<A> {
    /// Create a new [`Mimalloc`] instance with an OS allocator.
    pub const fn with_os_allocator(os_alloc: A) -> Self {
        Self {
            heap: Heap::new(),
            os_alloc,
            #[cfg(feature = "deferred_free")]
            deferred_free_hook: None,
        }
    }

    #[cfg(feature = "deferred_free")]
    /// Register a hook to complete deferred free when the allocator needs more memory.
    /// A new hook replaces the old one.
    ///
    /// See the documentation of
    /// [`mi_register_deferred_free`](https://microsoft.github.io/mimalloc/group__extended.html#ga3460a6ca91af97be4058f523d3cb8ece)
    /// (the extra `arg` is not supported).
    pub const fn register_deferred_free(&mut self, hook: DeferredFreeHook<A>) {
        self.deferred_free_hook = Some(hook);
    }

    /// Collect free memory.
    pub fn collect(&mut self) {
        self.heap.collect(&self.os_alloc);
    }

    /// [`GlobalAlloc::alloc`] but requires a mutable reference `&mut self`.
    ///
    /// # Safety
    ///
    /// See [`GlobalAlloc::alloc`].
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        self.heap.malloc_aligned(
            layout.size(),
            layout.align(),
            &self.os_alloc,
            #[cfg(feature = "deferred_free")]
            self.deferred_free_hook,
        )
    }

    /// [`GlobalAlloc::dealloc`] but requires a mutable reference `&mut self`.
    ///
    /// # Safety
    ///
    /// See [`GlobalAlloc::dealloc`].
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, _: Layout) {
        self.heap.free(ptr, &self.os_alloc)
    }
}

impl<A: GlobalAlloc> Drop for Mimalloc<A> {
    fn drop(&mut self) {
        self.collect();
    }
}

#[cfg(feature = "mmap")]
mod mmap;
#[cfg(feature = "mmap")]
pub use mmap::{new_mimalloc_mmap, MimallocMmap, MmapAlloc};

#[cfg(all(not(docsrs), feature = "std_mutex", feature = "spin_mutex"))]
compile_error!("Only one of 'std_mutex' and 'spin_mutex' features can be enabled");

#[cfg(any(feature = "std_mutex", feature = "spin_mutex"))]
mod mutex;
#[cfg(any(feature = "std_mutex", feature = "spin_mutex"))]
pub use mutex::MimallocMutexWrapper;

#[cfg(all(feature = "mmap", any(feature = "std_mutex", feature = "spin_mutex")))]
/// Wrapper around [`Mimalloc`] with `mmap` allocator and mutex.
pub type MimallocMmapMutex = MimallocMutexWrapper<MmapAlloc>;
#[cfg(all(feature = "mmap", any(feature = "std_mutex", feature = "spin_mutex")))]
/// Create a new [`MimallocMmapMutex`] instance by a `const fn`.
pub const fn new_mimalloc_mmap_mutex() -> MimallocMmapMutex {
    MimallocMutexWrapper::with_os_allocator(MmapAlloc)
}
