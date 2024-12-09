use crate::constants::*;

pub const fn wsize_from_size(size: usize) -> usize {
    size.div_ceil(MI_INTPTR_SIZE)
}

pub const fn bin_for_size(size: usize) -> usize {
    let wsize = wsize_from_size(size);
    bin_for_wsize(wsize)
}

pub const fn bin_for_wsize(wsize: usize) -> usize {
    if wsize <= 1 {
        1
    } else if (MI_ALIGN_W == 4 && wsize <= 4) || (MI_ALIGN_W == 2 && wsize <= 8) {
        wsize.next_multiple_of(2)
    } else if MI_ALIGN_W == 1 && wsize <= 8 {
        wsize
    } else if wsize > MI_LARGE_WSIZE_MAX {
        MI_BIN_HUGE
    } else {
        let wsize = if MI_ALIGN_W == 4 {
            wsize.next_multiple_of(4)
        } else {
            wsize
        } - 1;
        let b = (usize::BITS - 1 - wsize.leading_zeros()) as usize;
        ((b << 2) + ((wsize >> (b - 2)) & 3)) - 3
    }
}

const fn wsize_range_in_same_small_bin() -> [(u8, u8); MI_SMALL_WSIZE_MAX + 1] {
    let mut result = [(0, 0); MI_SMALL_WSIZE_MAX + 1];

    let mut wsize = 1;

    while wsize <= MI_SMALL_WSIZE_MAX {
        let bin = bin_for_wsize(wsize);
        let l = if wsize == 1 { 0 } else { wsize };
        let mut r = wsize + 1;
        while r <= MI_SMALL_SIZE_MAX && bin_for_wsize(r) == bin {
            r += 1;
        }
        let mut i = l;
        while i < r {
            result[i] = (l as u8, r as u8);
            i += 1;
        }
        wsize = r;
    }

    result
}

const fn block_size_for_bin() -> [usize; MI_BIN_HUGE] {
    let mut result = [1; MI_BIN_HUGE];
    let mut wsize = 1;
    while wsize <= MI_LARGE_WSIZE_MAX {
        result[bin_for_wsize(wsize)] = wsize * MI_INTPTR_SIZE;
        wsize += 1;
    }
    result
}

pub const WSIZE_RANGE_IN_SAME_SMALL_BIN: [(u8, u8); MI_SMALL_WSIZE_MAX + 1] =
    wsize_range_in_same_small_bin();

pub const BLOCK_SIZE_FOR_BIN: [usize; MI_BIN_HUGE] = block_size_for_bin();
