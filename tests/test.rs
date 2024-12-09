use baby_mimalloc::Mimalloc;
use rand::prelude::*;
use std::alloc::{GlobalAlloc, Layout, System};
use std::array::from_fn;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

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

#[test]
fn alloc_iter_size() {
    let mut allocator = Mimalloc::with_os_allocator(System);

    let allocation = Vec::from_iter((0..100_000).map(|i| test_alloc(&mut allocator, i, 1)));

    for (ptr, layout) in allocation {
        unsafe { allocator.dealloc(ptr, layout) };
    }
}

#[test]
fn random_alloc_small() {
    let mut rng = thread_rng();
    let mut allocator = Mimalloc::with_os_allocator(System);

    let allocation = Vec::from_iter((0..20_000_000).map(|_| {
        let align = 1 << rng.gen_range(0..=3);
        let size = rng.gen_range(1..128usize).next_multiple_of(align);
        test_alloc(&mut allocator, size, align)
    }));

    for (ptr, layout) in allocation {
        unsafe { allocator.dealloc(ptr, layout) };
    }
}

#[test]
fn random_alloc_large() {
    let mut rng = thread_rng();
    let mut allocator = Mimalloc::with_os_allocator(System);

    let allocation = Vec::from_iter((0..10000).map(|_| {
        let align = 1 << rng.gen_range(0..=20);
        let size = rng.gen_range(1..=10) * align;
        test_alloc(&mut allocator, size, align)
    }));

    for (ptr, layout) in allocation {
        unsafe { allocator.dealloc(ptr, layout) };
    }
}

#[derive(Default)]
struct SystemWithStatInner {
    system: System,
    allocation: BTreeMap<usize, Layout>,
    used: usize,
    peak: usize,
}

#[derive(Clone, Default)]
struct SystemWithStat(Arc<Mutex<SystemWithStatInner>>);

unsafe impl GlobalAlloc for SystemWithStat {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut inner = self.0.lock().unwrap();
        let p = inner.system.alloc(layout);
        inner.used += layout.size();
        inner.peak = inner.peak.max(inner.used);
        inner.allocation.insert(p as usize, layout);
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut inner = self.0.lock().unwrap();
        let allocated_layout = inner
            .allocation
            .remove(&(ptr as usize))
            .expect("deallocating unknown pointer");
        assert_eq!(
            layout, allocated_layout,
            "deallocating with different layout"
        );
        inner.used -= layout.size();
        inner.system.dealloc(ptr, layout);
    }
}

#[test]
fn alloc_dealloc() {
    let os_alloc = SystemWithStat::default();
    let mut allocator = Mimalloc::with_os_allocator(os_alloc.clone());

    const N: usize = 10_000_000;
    for count in (0..=N).step_by(N / 10) {
        let allocation = Vec::from_iter((0..count).map(|_| test_alloc(&mut allocator, 1, 1)));
        for (ptr, layout) in allocation {
            unsafe { allocator.dealloc(ptr, layout) };
        }
    }

    let peak = os_alloc.0.lock().unwrap().peak;
    let threshold = const { (N * 9).next_multiple_of(4 * 1024 * 1024) };
    assert!(peak <= threshold, "peak: {peak} > {threshold}");
    assert!(peak >= threshold / 2, "peak: {peak} < {threshold} / 2");
}

#[test]
fn random_alloc_dealloc_small() {
    let mut rng = thread_rng();
    let os_alloc = SystemWithStat::default();
    let mut allocator = Mimalloc::with_os_allocator(os_alloc.clone());

    const N: usize = 1_000_000;
    const K: usize = 4;
    const T: usize = 100;

    let mut count = [0; K];
    let mut ptrs = [const { Vec::new() }; K];

    for t in 0..=T {
        let new_count = if t == T {
            [0; K]
        } else {
            from_fn(|_| rng.gen_range(0..N))
        };
        for (i, (old, new)) in count.into_iter().zip(new_count).enumerate() {
            let size = (i + 1) * 8;
            if new > old {
                ptrs[i].extend((old..new).map(|_| test_alloc(&mut allocator, size, 1)));
            } else {
                ptrs[i]
                    .drain(new..)
                    .for_each(|(ptr, layout)| unsafe { allocator.dealloc(ptr, layout) });
            }
            ptrs[i].shuffle(&mut rng);
        }
        count = new_count;
    }

    let peak = os_alloc.0.lock().unwrap().peak;
    let threshold = const { (N * 5 * K * (K + 1)).next_multiple_of(4 * 1024 * 1024) };
    assert!(peak <= threshold, "peak: {peak} > {threshold}");
    assert!(peak >= threshold / 2, "peak: {peak} < {threshold} / 2");
}

#[test]
fn random_alloc_dealloc_small_collect() {
    let mut rng = thread_rng();
    let os_alloc = SystemWithStat::default();
    let mut allocator = Mimalloc::with_os_allocator(os_alloc.clone());

    const N: usize = 1_000_000;
    const K: usize = 3;
    const T: usize = 20;

    let mut count = [0; K];
    let mut ptrs = [const { Vec::new() }; K];

    for t in 0..=T {
        let new_count = if t == T {
            [0; K]
        } else {
            from_fn(|_| {
                let bound = if rng.gen() { N } else { 2 };
                rng.gen_range(0..bound)
            })
        };
        for (i, (old, new)) in count.into_iter().zip(new_count).enumerate() {
            let size = (i + 1) * 8;
            if new > old {
                ptrs[i].extend((old..new).map(|_| test_alloc(&mut allocator, size, 1)));
            } else {
                ptrs[i]
                    .drain(new..)
                    .for_each(|(ptr, layout)| unsafe { allocator.dealloc(ptr, layout) });
            }
            ptrs[i].shuffle(&mut rng);
            allocator.collect();
        }
        count = new_count;
    }

    let peak = os_alloc.0.lock().unwrap().peak;
    let threshold = const { (N * 5 * K * (K + 1)).next_multiple_of(4 * 1024 * 1024) };
    assert!(peak <= threshold, "peak: {peak} > {threshold}");
    assert!(peak >= threshold / 2, "peak: {peak} < {threshold} / 2");
}

#[test]
fn random_alloc_dealloc_large() {
    let mut rng = thread_rng();
    let os_alloc = SystemWithStat::default();
    let mut allocator = Mimalloc::with_os_allocator(os_alloc.clone());

    const N: usize = 500;
    const M: usize = 10;
    const K: usize = 20;
    const T: usize = 100;

    let mut count = [0; K];
    let mut allocation = [const { Vec::new() }; K];

    for t in 0..=T {
        let new_count = if t == T {
            [0; K]
        } else {
            from_fn(|_| rng.gen_range(0..N))
        };
        for (i, (old, new)) in count.into_iter().zip(new_count).enumerate() {
            let align = 1 << i;
            if new > old {
                allocation[i].extend((old..new).map(|_| {
                    let size = align * rng.gen_range(1..=M);
                    test_alloc(&mut allocator, size, align)
                }));
            } else {
                allocation[i]
                    .drain(new..)
                    .for_each(|(p, layout)| unsafe { allocator.dealloc(p, layout) });
            }
            allocation[i].shuffle(&mut rng);
        }
        count = new_count;
    }

    let peak = os_alloc.0.lock().unwrap().peak;
    let threshold = const { (N * (1 << K) * M).next_multiple_of(4 * 1024 * 1024) };
    assert!(peak <= threshold, "peak: {peak} > {threshold}");
    assert!(peak >= threshold / 2, "peak: {peak} < {threshold} / 2");
}
