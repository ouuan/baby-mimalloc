use crate::*;

/// Handle to complete deferred free in [`DeferredFreeHook`].
pub struct DeferredFreeHandle<'a, A: GlobalAlloc> {
    pub(crate) heap: &'a mut Heap,
    pub(crate) os_alloc: &'a A,
}

impl<A: GlobalAlloc> DeferredFreeHandle<'_, A> {
    /// Deallocate the block of memory at the given `ptr`.
    ///
    /// # Safety
    ///
    /// See [`GlobalAlloc::dealloc`].
    pub unsafe fn free(&mut self, ptr: *mut u8) {
        self.heap.free(ptr, self.os_alloc);
    }
}

/// Hook to complete deferred free when the allocator needs more memory.
/// See [`DeferredFreeHandle`] and [`Mimalloc::register_deferred_free`].
pub type DeferredFreeHook<A> = fn(handle: &mut DeferredFreeHandle<A>, force: bool, heartbeat: u64);
