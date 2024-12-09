#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use baby_mimalloc::{new_mimalloc_mmap_mutex, MimallocMmapMutex};
use rand::distributions::{DistString, Standard};
use rand::prelude::*;

#[global_allocator]
static ALLOCATOR: MimallocMmapMutex = new_mimalloc_mmap_mutex();

#[test]
fn vec_test() {
    for _ in 0..10 {
        let mut vec = Vec::new();
        let mut rng = thread_rng();
        for _ in 0..10_000 {
            let len = rng.gen_range(1..100_000);
            let val = vec![42; len];
            vec.push(val);
        }
    }
}

#[test]
fn btree_map_test() {
    const N: usize = 100_000;
    let mut map = BTreeMap::new();
    let mut rng = thread_rng();
    for _ in 0..N {
        let key_len = rng.gen_range(1..10);
        let key = Standard.sample_string(&mut rng, key_len);
        let val_len = rng.gen_range(1..10000);
        let val = Standard.sample_string(&mut rng, val_len);
        map.insert(key, val);
    }
    let map_len = map.len();
    assert!(map_len < N);
    let vec = map.into_iter().collect::<Vec<_>>();
    assert_eq!(map_len, vec.len());
}
