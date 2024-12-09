use crate::Mimalloc;
use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_void;
use core::ptr::null_mut;
use libc::{mmap, munmap, sysconf};
use libc::{MAP_ANONYMOUS, MAP_FAILED, MAP_PRIVATE, PROT_READ, PROT_WRITE, _SC_PAGE_SIZE};

/// A simple `mmap`-based allocator that can be used to power [`Mimalloc`].
///
/// It is only used to allocate large chunks of memory and is not suitable for general malloc.
#[derive(Default)]
pub struct MmapAlloc;

/// [`Mimalloc`] powered by `mmap` ([`MmapAlloc`]).
pub type MimallocMmap = Mimalloc<MmapAlloc>;

/// Create a new [`MimallocMmap`] instance by a `const fn`.
pub const fn new_mimalloc_mmap() -> MimallocMmap {
    Mimalloc::with_os_allocator(MmapAlloc)
}

unsafe fn mmap_anoymous(size: usize) -> *mut c_void {
    mmap(
        null_mut(),
        size,
        PROT_READ | PROT_WRITE,
        MAP_PRIVATE | MAP_ANONYMOUS,
        -1,
        0,
    )
}

unsafe impl GlobalAlloc for MmapAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // `mmap` and `munmap` requires addresses to be aligned to page size
        debug_assert!(size % sysconf(_SC_PAGE_SIZE) as usize == 0);
        debug_assert!(align % sysconf(_SC_PAGE_SIZE) as usize == 0);

        // try mapping exactly `size` at first
        let p = mmap_anoymous(size);
        if p == MAP_FAILED {
            return null_mut();
        }

        if p as usize % align == 0 {
            // aligned
            return p.cast();
        }
        // not aligned
        munmap(p, size);

        // over allocate to ensure alignment
        let start = mmap_anoymous(size + align - 1);
        if start == MAP_FAILED {
            return null_mut();
        }

        let offset = start.align_offset(align);
        let aligned = start.add(offset);
        if offset != 0 {
            munmap(start, offset);
        }
        if offset != align - 1 {
            let end = aligned.add(size);
            munmap(end, align - 1 - offset);
        }
        aligned.cast()
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        munmap(ptr.cast(), layout.size());
    }
}
