pub const MI_INTPTR_SIZE: usize = usize::BITS as usize / 8;
pub const MI_INTPTR_SHIFT: usize = match MI_INTPTR_SIZE {
    4 => 2,
    8 => 3,
    _ => panic!("only 32-bit and 64-bit platforms are supported"),
};

pub const MI_SMALL_PAGE_SHIFT: usize = 13 + MI_INTPTR_SHIFT;
pub const MI_LARGE_PAGE_SHIFT: usize = 6 + MI_SMALL_PAGE_SHIFT;
pub const MI_SEGMENT_SHIFT: usize = MI_LARGE_PAGE_SHIFT;

pub const MI_SEGMENT_SIZE: usize = 1 << MI_SEGMENT_SHIFT;
pub const MI_SEGMENT_MASK: usize = MI_SEGMENT_SIZE - 1;

pub const MI_SMALL_PAGE_SIZE: usize = 1 << MI_SMALL_PAGE_SHIFT;
pub const MI_LARGE_PAGE_SIZE: usize = 1 << MI_LARGE_PAGE_SHIFT;

pub const MI_SMALL_PAGES_PER_SEGMENT: usize = MI_SEGMENT_SIZE / MI_SMALL_PAGE_SIZE;
pub const MI_LARGE_PAGES_PER_SEGMENT: usize = MI_SEGMENT_SIZE / MI_LARGE_PAGE_SIZE;

pub const MI_SMALL_WSIZE_MAX: usize = 128;
pub const MI_SMALL_SIZE_MAX: usize = MI_SMALL_WSIZE_MAX << MI_INTPTR_SHIFT;

pub const MI_LARGE_SIZE_MAX: usize = MI_LARGE_PAGE_SIZE / 8;
pub const MI_LARGE_WSIZE_MAX: usize = MI_LARGE_SIZE_MAX >> MI_INTPTR_SHIFT;

pub const MI_BIN_HUGE: usize = 64;

pub const MI_MAX_ALIGN_SIZE: usize = 16;

pub const MI_ALIGN_W: usize = {
    assert!(MI_MAX_ALIGN_SIZE % MI_INTPTR_SIZE == 0);
    let result = MI_MAX_ALIGN_SIZE / MI_INTPTR_SIZE;
    match result {
        1 | 2 | 4 => result,
        _ => panic!("invalid max alignment"),
    }
};

pub const MI_PAGE_HUGE_ALIGN: usize = 256 * 1024;

pub const MI_MAX_EXTEND_SIZE: usize = 4096;
pub const MI_MIN_EXTEND: usize = 1;
