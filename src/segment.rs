// NOTE: Avoid using `ptr::{add, offset_from}` when unsafe (UB). Convert to usize instead.

use crate::constants::*;
use crate::heap::Heap;
use crate::list::impl_list_item;
use crate::page::Page;
use core::alloc::{GlobalAlloc, Layout};
use core::mem::size_of;
use core::ptr::{null_mut, NonNull};

pub struct Segment {
    next: *mut Self,
    prev: *mut Self,
    used: usize,
    capacity: usize,
    segment_size: usize,
    info_size: usize,
    page_size: usize,
    // pages with a variable length at the end
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PageKind {
    Small,
    Large,
    Huge(usize),
}

impl_list_item!(Segment);

impl Segment {
    /// Allocate a segment and a page in it.
    pub fn alloc<A: GlobalAlloc>(
        page_kind: PageKind,
        os_alloc: &A,
    ) -> Option<(NonNull<Self>, NonNull<Page>)> {
        const INFO_ALIGN: usize = if MI_MAX_ALIGN_SIZE < 16 {
            16
        } else {
            MI_MAX_ALIGN_SIZE
        };
        let (capacity, segment_size, info_size, page_size) = match page_kind {
            PageKind::Small => {
                const {
                    (
                        MI_SMALL_PAGES_PER_SEGMENT,
                        MI_SEGMENT_SIZE,
                        (size_of::<Self>() + MI_SMALL_PAGES_PER_SEGMENT * size_of::<Page>())
                            .next_multiple_of(INFO_ALIGN),
                        MI_SMALL_PAGE_SIZE,
                    )
                }
            }
            PageKind::Large => {
                const {
                    assert!(MI_LARGE_PAGES_PER_SEGMENT == 1);
                    (
                        1,
                        MI_SEGMENT_SIZE,
                        (size_of::<Self>() + size_of::<Page>()).next_multiple_of(INFO_ALIGN),
                        MI_LARGE_PAGE_SIZE,
                    )
                }
            }
            PageKind::Huge(size) => {
                const INFO_SIZE: usize =
                    (size_of::<Segment>() + size_of::<Page>()).next_multiple_of(INFO_ALIGN);
                let segment_size = (size + INFO_SIZE).next_multiple_of(MI_PAGE_HUGE_ALIGN);
                (1, segment_size, INFO_SIZE, segment_size)
            }
        };

        let layout = unsafe { Layout::from_size_align_unchecked(segment_size, MI_SEGMENT_SIZE) };
        let p = unsafe { os_alloc.alloc(layout) as *mut Self };

        let segment = NonNull::new(p)?;
        let pages_base = Self::pages_base_addr(segment.as_ptr()) as *mut Page;

        // clear pages
        unsafe { pages_base.write_bytes(0, capacity) };

        let value = Self {
            next: null_mut(),
            prev: null_mut(),
            used: 1, // always immediately allocate a page
            capacity,
            segment_size,
            info_size,
            page_size,
        };
        unsafe { segment.write(value) };

        let mut page = unsafe { NonNull::new_unchecked(pages_base) };
        unsafe { page.as_mut() }.set_in_use(true);

        Some((segment, page))
    }

    pub fn find_free_small_page(&self) -> NonNull<Page> {
        debug_assert_eq!(self.capacity, MI_SMALL_PAGES_PER_SEGMENT);
        let mut addr = Self::pages_base_addr(self);
        for _ in 0..MI_SMALL_PAGES_PER_SEGMENT {
            let mut p = unsafe { NonNull::<Page>::new_unchecked(addr as _) };
            let page = unsafe { p.as_mut() };
            if !page.in_use() {
                page.set_in_use(true);
                return p;
            }
            addr += size_of::<Page>();
        }
        unreachable!()
    }

    pub fn is_full(&self) -> bool {
        self.used == self.capacity
    }

    pub fn increment_used(&mut self) {
        self.used += 1;
    }

    pub fn page_payload_addr(&self, page: *const Page) -> usize {
        let index = (page as usize - Self::pages_base_addr(self)) / size_of::<Page>();
        let base = self as *const _ as usize;
        let offset = if index == 0 {
            self.info_size
        } else {
            index * self.page_size
        };
        base + offset
    }

    pub fn of_ptr<T>(ptr: *const T) -> *mut Self {
        (ptr as usize & !MI_SEGMENT_MASK) as _
    }

    pub fn page_of_ptr(&self, ptr: *const u8) -> NonNull<Page> {
        let offset = ptr as usize - self as *const _ as usize;
        let index = offset / self.page_size;
        let p = (Self::pages_base_addr(self) + index * size_of::<Page>()) as _;
        unsafe { NonNull::new_unchecked(p) }
    }

    fn pages_base_addr(self_ptr: *const Self) -> usize {
        self_ptr as usize + size_of::<Self>()
    }

    pub fn page_size(&self, page: *const Page) -> usize {
        if (page as usize - self as *const _ as usize) < self.page_size {
            self.page_size - self.info_size
        } else {
            self.page_size
        }
    }

    pub fn remove_a_page<A: GlobalAlloc>(
        mut segment: NonNull<Self>,
        heap: &mut Heap,
        os_alloc: &A,
    ) {
        let seg = unsafe { segment.as_mut() };
        seg.used -= 1;

        if seg.used == 0 {
            heap.remove_small_free_segment(seg);
            unsafe {
                let layout = Layout::from_size_align_unchecked(seg.segment_size, MI_SEGMENT_SIZE);
                os_alloc.dealloc(segment.as_ptr().cast(), layout);
            }
        } else if seg.used + 1 == seg.capacity {
            heap.push_small_free_segment(seg);
        }
    }
}
