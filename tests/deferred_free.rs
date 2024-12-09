use baby_mimalloc::deferred_free::DeferredFreeHandle;
use baby_mimalloc::Mimalloc;
use rand::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::Mutex;

fn test_alloc<A: GlobalAlloc>(
    allocator: &mut Mimalloc<A>,
    size: usize,
    align: usize,
) -> (*mut u8, Layout) {
    let layout = Layout::from_size_align(size, align).unwrap();
    let p = unsafe { allocator.alloc(layout) };
    assert!(p as usize % align == 0, "p: {p:?}, align: {align}");
    unsafe { p.write_bytes(0x37, size) };
    (p, layout)
}

static DEFERRED_FREE_ALLOCATION: Mutex<Vec<(usize, Layout)>> = Mutex::new(Vec::new());
static DEFERRED_FREE_ALLOCATOR: Mutex<Mimalloc<System>> = {
    let mut allocator = Mimalloc::with_os_allocator(System);
    allocator.register_deferred_free(deferred_free_hook);
    Mutex::new(allocator)
};

fn deferred_free_hook(handle: &mut DeferredFreeHandle<System>, force: bool, heartbeat: u64) {
    if heartbeat % 10000 == 0 {
        dbg!(force, heartbeat);
    }
    for (addr, _) in DEFERRED_FREE_ALLOCATION.lock().unwrap().drain(..) {
        unsafe { handle.free(addr as _) };
    }
}

#[test]
fn test_deferred_free() {
    let mut rng = thread_rng();

    for _ in 0..10_000_000 {
        let align = 1 << rng.gen_range(0..=3);
        let size = rng.gen_range(1..128usize).next_multiple_of(align);
        let (p, layout) = test_alloc(&mut DEFERRED_FREE_ALLOCATOR.lock().unwrap(), size, align);
        DEFERRED_FREE_ALLOCATION
            .lock()
            .unwrap()
            .push((p as _, layout));
    }

    let mut allocator = DEFERRED_FREE_ALLOCATOR.lock().unwrap();
    for (addr, layout) in DEFERRED_FREE_ALLOCATION.lock().unwrap().drain(..) {
        unsafe { allocator.dealloc(addr as _, layout) };
    }
}
