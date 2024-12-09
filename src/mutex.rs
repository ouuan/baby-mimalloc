use crate::Mimalloc;
use core::alloc::{GlobalAlloc, Layout};

#[cfg(feature = "spin_mutex")]
use spin::{Mutex, MutexGuard};
#[cfg(not(feature = "spin_mutex"))]
use std::sync::{Mutex, MutexGuard};

/// Wrap [`Mimalloc`] inside a [`Mutex`] and implement [`GlobalAlloc`].
#[derive(Default)]
pub struct MimallocMutexWrapper<A: GlobalAlloc>(Mutex<Mimalloc<A>>);

impl<A: GlobalAlloc> MimallocMutexWrapper<A> {
    /// See [`Mimalloc::with_os_allocator`].
    pub const fn with_os_allocator(os_alloc: A) -> Self {
        Self(Mutex::new(Mimalloc::with_os_allocator(os_alloc)))
    }

    #[cfg(feature = "deferred_free")]
    /// See [`Mimalloc::register_deferred_free`].
    pub fn register_deferred_free(&self, hook: crate::DeferredFreeHook<A>) {
        self.allocator().register_deferred_free(hook);
    }

    /// See [`Mimalloc::collect`].
    pub fn collect(&self) {
        self.allocator().collect();
    }

    fn allocator(&self) -> MutexGuard<Mimalloc<A>> {
        #[cfg(feature = "spin_mutex")]
        {
            self.0.lock()
        }
        #[cfg(not(feature = "spin_mutex"))]
        {
            self.0.lock().expect("failed to lock the allocator")
        }
    }
}

unsafe impl<A: GlobalAlloc> GlobalAlloc for MimallocMutexWrapper<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocator().alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.allocator().dealloc(ptr, layout)
    }
}
