use crate::constants::*;
use crate::list::{LinkedList, LinkedListItem};
use crate::page::{empty_page, Page};
use crate::segment::{PageKind, Segment};
use crate::utils::{
    bin_for_size, wsize_from_size, BLOCK_SIZE_FOR_BIN, WSIZE_RANGE_IN_SAME_SMALL_BIN,
};
use core::alloc::GlobalAlloc;
use core::ptr::{null_mut, NonNull};

pub struct Heap {
    pages_free_direct: [NonNull<Page>; MI_SMALL_WSIZE_MAX + 1],
    pages: [LinkedList<Page>; MI_BIN_HUGE + 1],
    small_free_segments: LinkedList<Segment>,
    deferred_free_hook: Option<fn(bool, u64)>,
    heartbeat: u64,
    calling_deferred_free: bool,
}

impl Default for Heap {
    fn default() -> Self {
        Self::new()
    }
}

impl Heap {
    pub const fn new() -> Self {
        Self {
            pages_free_direct: [empty_page(); MI_SMALL_WSIZE_MAX + 1],
            pages: [const { LinkedList::new() }; MI_BIN_HUGE + 1],
            small_free_segments: LinkedList::new(),
            deferred_free_hook: None,
            heartbeat: 0,
            calling_deferred_free: false,
        }
    }

    pub fn malloc<A: GlobalAlloc>(&mut self, size: usize, os_alloc: &A) -> *mut u8 {
        let result = if size <= MI_SMALL_SIZE_MAX {
            let page = self.get_small_free_page(size);
            Page::malloc_fast(page, self, size, os_alloc)
        } else {
            self.malloc_generic(size, os_alloc)
        };
        debug_assert!(
            match &result {
                None => true,
                Some((ptr, page)) => page.free() != ptr.as_ptr() as _,
            },
            "page.free not changed after allocation"
        );
        debug_assert!(
            match &result {
                None => true,
                Some((_, page)) => page.block_size() >= size,
            },
            "allocated from a block smaller than requested"
        );
        result.map_or(null_mut(), |(ptr, _)| ptr.as_ptr())
    }

    pub fn malloc_aligned<A: GlobalAlloc>(
        &mut self,
        size: usize,
        align: usize,
        os_alloc: &A,
    ) -> *mut u8 {
        debug_assert!(align.is_power_of_two());

        if align <= MI_INTPTR_SIZE {
            return self.malloc(size, os_alloc);
        }
        if size >= usize::MAX - align {
            return null_mut();
        }

        if size <= MI_SMALL_SIZE_MAX {
            let page = self.get_small_free_page(size);
            let free = unsafe { page.as_ref() }.free();
            if !free.is_null() && (free as usize & (align - 1) == 0) {
                return Page::malloc_fast(page, self, size, os_alloc)
                    .map_or(null_mut(), |(ptr, _)| ptr.as_ptr());
            }
        }

        self.malloc_generic(size + align - 1, os_alloc)
            .map_or(null_mut(), |(ptr, page)| {
                page.set_aligned(true);
                let aligned_addr = (ptr.as_ptr() as usize + align - 1) & !(align - 1);
                aligned_addr as *mut u8
            })
    }

    pub fn free<A: GlobalAlloc>(&mut self, p: *mut u8, os_alloc: &A) {
        if let Some(segment) = unsafe { Segment::of_ptr(p).as_ref() } {
            let page = segment.page_of_ptr(p);
            Page::free_block(self, page, segment.into(), p, os_alloc);
        }
    }

    fn get_small_free_page(&mut self, size: usize) -> NonNull<Page> {
        let wsize = wsize_from_size(size);
        debug_assert!(wsize < self.pages_free_direct.len());
        self.pages_free_direct[wsize]
    }

    pub fn malloc_generic<A: GlobalAlloc>(
        &mut self,
        size: usize,
        os_alloc: &A,
    ) -> Option<(NonNull<u8>, &mut Page)> {
        self.deferred_free(false);

        let page = if size <= MI_LARGE_SIZE_MAX {
            self.find_free_page(size, os_alloc)
        } else {
            self.alloc_huge_page(size, os_alloc)
        };

        NonNull::new(page).and_then(|p| Page::malloc_fast(p, self, size, os_alloc))
    }

    pub const fn register_deferred_free(&mut self, hook: fn(bool, u64)) {
        self.deferred_free_hook = Some(hook);
    }

    fn deferred_free(&mut self, force: bool) {
        self.heartbeat = self.heartbeat.wrapping_add(1);
        if let Some(hook) = self.deferred_free_hook {
            if !self.calling_deferred_free {
                self.calling_deferred_free = true;
                hook(force, self.heartbeat);
                self.calling_deferred_free = false;
            }
        }
    }

    fn find_free_page<A: GlobalAlloc>(&mut self, size: usize, os_alloc: &A) -> *mut Page {
        let bin = bin_for_size(size);
        let pq = &mut self.pages[bin];

        if let Some(page) = unsafe { pq.first().as_mut() } {
            page.free_collect();
            if page.immediate_available() {
                return page;
            }
        }

        // mi_page_queue_find_free_ex

        let mut page_to_retire = null_mut::<Page>();
        let mut p = pq.first();
        let mut page_free_count = 0;

        while let Some(page) = unsafe { p.as_mut() } {
            page.free_collect();

            let next = page.next();

            if page.immediate_available() {
                if page_free_count < 8 && page.all_free() {
                    page_free_count += 1;
                    if let Some(rpage) = NonNull::new(page_to_retire) {
                        self.retire_page(rpage, os_alloc);
                    }
                    page_to_retire = p;
                    p = next;
                    continue;
                }
                break;
            }

            page.extend();
            if page.immediate_available() {
                break;
            }

            page.set_full(true);
            self.page_queue_remove(page);

            p = next;
        }

        if p.is_null() {
            p = page_to_retire;
            page_to_retire = null_mut();
        }

        if let Some(rpage) = NonNull::new(page_to_retire) {
            self.retire_page(rpage, os_alloc);
        }

        if p.is_null() {
            let block_size = BLOCK_SIZE_FOR_BIN[bin];
            self.alloc_page(block_size, os_alloc)
        } else {
            debug_assert!(unsafe { (*p).immediate_available() });
            p
        }
    }

    fn alloc_page<A: GlobalAlloc>(&mut self, block_size: usize, os_alloc: &A) -> *mut Page {
        self.segment_page_alloc(block_size, os_alloc)
            .map_or(null_mut(), |(segment, mut p)| {
                let page_size = unsafe { segment.as_ref() }.page_size(p.as_ptr());
                let page = unsafe { p.as_mut() };
                page.init(page_size, block_size);
                self.page_queue_push_front(page);
                p.as_ptr()
            })
    }

    fn alloc_huge_page<A: GlobalAlloc>(&mut self, size: usize, os_alloc: &A) -> *mut Page {
        let block_size = size.next_multiple_of(MI_INTPTR_SIZE);
        self.alloc_page(block_size, os_alloc)
    }

    fn page_queue_push_front(&mut self, page: &mut Page) {
        let pq = &mut self.pages[page.bin()];
        unsafe { pq.push_front(page.into()) };
        self.page_queue_first_update(page.block_size(), page);
    }

    pub fn page_queue_push_back(&mut self, page: NonNull<Page>) {
        let pq = &mut self.pages[unsafe { page.as_ref() }.bin()];
        if unsafe { pq.push_back(page) } {
            self.page_queue_first_update(unsafe { page.as_ref() }.block_size(), page.as_ptr());
        }
    }

    fn page_queue_remove(&mut self, page: &mut Page) {
        let pq = &mut self.pages[page.bin()];
        debug_assert!(pq.contains(page));
        if unsafe { pq.remove(page.into()) } {
            let first = pq.first();
            self.page_queue_first_update(page.block_size(), first);
        }
    }

    fn page_queue_first_update(&mut self, block_size: usize, page: *mut Page) {
        if block_size <= MI_SMALL_SIZE_MAX {
            let wsize = wsize_from_size(block_size);
            if self.pages_free_direct[wsize].as_ptr() != page {
                let page = NonNull::new(page).unwrap_or(empty_page());
                let (l, r) = WSIZE_RANGE_IN_SAME_SMALL_BIN[wsize];
                self.pages_free_direct[l as usize..r as usize].fill(page);
            }
        }
    }

    fn segment_page_alloc<A: GlobalAlloc>(
        &mut self,
        block_size: usize,
        os_alloc: &A,
    ) -> Option<(NonNull<Segment>, NonNull<Page>)> {
        if block_size < MI_SMALL_PAGE_SIZE / 8 {
            match unsafe { self.small_free_segments.first().as_mut() } {
                None => {
                    let (segment, page) = Segment::alloc(PageKind::Small, os_alloc)?;
                    unsafe { self.small_free_segments.push_back(segment) };
                    Some((segment, page))
                }
                Some(segment) => {
                    let page = segment.find_free_small_page();
                    segment.increment_used();
                    if segment.is_full() {
                        unsafe { self.small_free_segments.remove(segment.into()) };
                    }
                    Some((segment.into(), page))
                }
            }
        } else {
            let page_kind = if block_size < MI_LARGE_SIZE_MAX - size_of::<Segment>() {
                PageKind::Large
            } else {
                PageKind::Huge(block_size)
            };
            Segment::alloc(page_kind, os_alloc)
        }
    }

    pub fn push_small_free_segment(&mut self, segment: &mut Segment) {
        unsafe { self.small_free_segments.push_back(segment.into()) };
    }

    pub fn remove_small_free_segment(&mut self, segment: &mut Segment) {
        if self.small_free_segments.contains(segment) {
            unsafe { self.small_free_segments.remove(segment.into()) };
        }
    }

    // _mi_page_free
    pub fn retire_page<A: GlobalAlloc>(&mut self, mut page: NonNull<Page>, os_alloc: &A) {
        self.page_queue_remove(unsafe { page.as_mut() });
        unsafe { page.write_bytes(0, 1) };
        let segment = unsafe { NonNull::new_unchecked(Segment::of_ptr(page.as_ptr())) };
        Segment::remove_a_page(segment, self, os_alloc);
    }

    pub fn collect<A: GlobalAlloc>(&mut self, os_alloc: &A) {
        for i in 0..self.pages.len() {
            let mut p = self.pages[i].first();
            while let Some(mut page) = NonNull::new(p) {
                let page_mut = unsafe { page.as_mut() };
                page_mut.free_collect();
                p = page_mut.next();
                if page_mut.all_free() {
                    self.retire_page(page, os_alloc);
                }
            }
        }
    }
}
