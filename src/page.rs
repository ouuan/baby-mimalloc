// NOTE: Avoid using `ptr::{add, offset_from}` when unsafe (UB). Convert to usize instead.

use crate::constants::*;
use crate::heap::Heap;
use crate::list::impl_list_item;
use crate::segment::Segment;
use crate::utils::bin_for_size;
#[cfg(feature = "deferred_free")]
use crate::DeferredFreeHook;
use core::alloc::GlobalAlloc;
use core::ptr::{null_mut, NonNull};

#[repr(align(2))]
#[derive(Clone, Copy)]
struct PageFlags {
    has_aligned: bool,
    full: bool,
}

union PageFlagUnion {
    flag_16: u16,
    flags: PageFlags,
}

pub struct Page {
    in_use: bool,
    flags: PageFlagUnion, // save a branch in `free_block`
    capacity: u16,
    reserved: u16,
    free: *mut Block,
    used: u16,
    local_free: *mut Block,
    block_size: usize,
    bin: u8,
    next: *mut Self,
    prev: *mut Self,
}

impl_list_item!(Page);

pub struct Block {
    next: *mut Self,
}

impl Page {
    pub fn malloc_fast<'a, A: GlobalAlloc>(
        mut page: NonNull<Self>,
        heap: &'a mut Heap,
        size: usize,
        os_alloc: &A,
        #[cfg(feature = "deferred_free")] deferred_free_hook: Option<DeferredFreeHook<A>>,
    ) -> Option<(NonNull<u8>, &'a mut Page)> {
        debug_assert!(
            page.as_ptr() == empty_page().as_ptr()
                || bin_for_size(size) == bin_for_size(unsafe { page.as_ref() }.block_size),
            "trying to allocate size {size} in a page with block size {}",
            { unsafe { page.as_ref() }.block_size }
        );
        match unsafe { page.as_ref().free.as_mut() } {
            None => heap.malloc_generic(
                size,
                os_alloc,
                #[cfg(feature = "deferred_free")]
                deferred_free_hook,
            ),
            Some(block) => {
                debug_assert!(
                    (block as *const _ as usize).wrapping_sub(page.as_ptr() as usize)
                        < MI_SEGMENT_SIZE,
                    "block not in segment: block {block:p}, page {page:p}",
                );
                debug_assert!(
                    block.next.is_null() ||
                    (block.next as usize).abs_diff(block as *const _ as usize) % unsafe { page.as_ref() }.block_size == 0,
                    "diff between block and next not multiple of block size: block {block:p}, next {:p}, block size {}",
                    block.next,
                    unsafe{page.as_ref()}.block_size
                );
                debug_assert!(
                    if block.next.is_null() {
                        true
                    } else {
                        let segment = Segment::of_ptr(block);
                        let segment = unsafe { segment.as_ref() }.unwrap();
                        (block.next as usize).abs_diff(block as *const _ as usize)
                            < segment.page_size(page.as_ptr())
                    },
                    "block and next not in the same block: {block:p}, next {:p}",
                    block.next
                );
                let page = unsafe { page.as_mut() };
                page.free = block.next;
                page.used += 1;
                // convert to usize first to avoid UB
                // > Undefined Behavior: attempting a write access using ... at ...,
                // > but that tag does not exist in the borrow stack for this location
                let addr = block as *mut _ as usize;
                let ptr = unsafe { NonNull::new_unchecked(addr as *mut u8) };
                Some((ptr, page))
            }
        }
    }

    pub fn free_block<A: GlobalAlloc>(
        heap: &mut Heap,
        mut page: NonNull<Page>,
        segment: NonNull<Segment>,
        p: *mut u8,
        os_alloc: &A,
    ) {
        let page_mut = unsafe { page.as_mut() };
        if unsafe { page_mut.flags.flag_16 } == 0 {
            // fast path
            page_mut.free_block_core(p.cast());
            if page_mut.all_free() && page_mut.should_retire() {
                heap.retire_page(page, os_alloc);
            }
        } else {
            // generic path
            let block = if unsafe { page_mut.flags.flags }.has_aligned {
                let offset =
                    p as usize - unsafe { segment.as_ref() }.page_payload_addr(page.as_ptr());
                (p as usize - offset % page_mut.block_size()) as *mut Block
            } else {
                p.cast()
            };
            page_mut.free_block_core(block);
            if page_mut.all_free() {
                if page_mut.should_retire() {
                    heap.retire_page(page, os_alloc);
                }
            } else if unsafe { page_mut.flags.flags }.full {
                page_mut.flags.flags.full = false;
                heap.page_queue_push_back(page);
            }
        }
    }

    fn free_block_core(&mut self, block: *mut Block) {
        debug_assert!(self.used > 0);
        unsafe { (*block).next = self.local_free };
        self.local_free = block;
        self.used -= 1;
    }

    pub fn init(&mut self, page_size: usize, block_size: usize) {
        debug_assert_eq!(self.reserved, 0, "block double inited");
        self.block_size = block_size;
        self.bin = bin_for_size(block_size) as u8;
        self.reserved = (page_size / block_size) as _;
        self.extend();
    }

    pub fn free_collect(&mut self) {
        if !self.local_free.is_null() {
            match unsafe { self.free.as_mut() } {
                None => self.free = self.local_free,
                Some(mut tail) => {
                    while let Some(next) = unsafe { tail.next.as_mut() } {
                        tail = next;
                    }
                    tail.next = self.local_free;
                }
            }
            self.local_free = null_mut();
        }
    }

    // _mi_page_retire
    fn should_retire(&mut self) -> bool {
        fn mostly_used(p: *mut Page) -> bool {
            if let Some(page) = unsafe { p.as_mut() } {
                page.reserved - page.used < page.reserved / 8
            } else {
                true
            }
        }

        if self.block_size < MI_LARGE_SIZE_MAX && mostly_used(self.prev) && mostly_used(self.next) {
            self.flags.flag_16 = 0;
            false
        } else {
            true
        }
    }

    pub fn extend(&mut self) {
        if self.immediate_available() || self.capacity >= self.reserved {
            return;
        }
        let bsize = self.block_size;
        let max_extend = (MI_MAX_EXTEND_SIZE / bsize).max(MI_MIN_EXTEND);
        let extend = ((self.reserved - self.capacity) as usize).min(max_extend);
        let segment = unsafe { Segment::of_ptr(self).as_mut().unwrap_unchecked() };
        let payload_start = segment.page_payload_addr(self);
        let mut addr = payload_start + bsize * self.capacity as usize;
        let end = addr + bsize * (extend - 1);
        self.free = addr as _;
        while addr != end {
            let next = addr + bsize;
            unsafe { (*(addr as *mut Block)).next = next as _ };
            addr = next;
        }
        unsafe { (*(addr as *mut Block)).next = null_mut() };
        self.capacity += extend as u16;
    }

    pub const fn free(&self) -> *mut Block {
        self.free
    }

    pub fn immediate_available(&self) -> bool {
        !self.free.is_null()
    }

    pub const fn block_size(&self) -> usize {
        self.block_size
    }

    pub const fn bin(&self) -> usize {
        self.bin as _
    }

    pub fn set_full(&mut self, full: bool) {
        self.flags.flags.full = full;
    }

    pub const fn all_free(&self) -> bool {
        self.used == 0
    }

    pub fn set_aligned(&mut self, aligned: bool) {
        self.flags.flags.has_aligned = aligned;
    }

    pub const fn in_use(&self) -> bool {
        self.in_use
    }

    pub fn set_in_use(&mut self, in_use: bool) {
        self.in_use = in_use
    }
}

mod empty_page {
    use super::*;
    use core::ptr::{null_mut, NonNull};

    struct EmptyPage(Page);

    unsafe impl Sync for EmptyPage {}

    static EMPTY_PAGE: EmptyPage = EmptyPage(Page {
        in_use: false,
        flags: PageFlagUnion { flag_16: 0 },
        capacity: 0,
        reserved: 0,
        free: null_mut(),
        used: 0,
        local_free: null_mut(),
        block_size: 0,
        bin: 0,
        next: null_mut(),
        prev: null_mut(),
    });

    pub const fn empty_page() -> NonNull<Page> {
        unsafe { NonNull::new_unchecked(&raw const EMPTY_PAGE.0 as _) }
    }
}

pub use empty_page::empty_page;
