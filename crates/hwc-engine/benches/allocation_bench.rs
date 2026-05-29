use std::sync::{Arc, RwLock};
use std::time::Instant;

fn main() {
    let max_chunks = 18_750_000;

    println!(
        "Benchmarking allocation of {} Arc<RwLock<Option<...>>> slots",
        max_chunks
    );

    // Test 1: Dense Vec allocation (current approach)
    let start = Instant::now();
    let mut dense_vec = Vec::with_capacity(max_chunks);
    for _ in 0..max_chunks {
        dense_vec.push(Arc::new(RwLock::new(None::<Arc<u64>>)));
    }
    let dense_time = start.elapsed();
    println!("Dense Vec allocation: {:?}", dense_time);

    // Test 2: Empty HashMap (sparse approach)
    let start = Instant::now();
    let _sparse_map: rustc_hash::FxHashMap<usize, Arc<RwLock<Option<Arc<u64>>>>> =
        rustc_hash::FxHashMap::default();
    let sparse_time = start.elapsed();
    println!("Sparse FxHashMap allocation: {:?}", sparse_time);

    // Test 3: Access time comparison (1000 random accesses)
    let mut sparse_map: rustc_hash::FxHashMap<usize, Arc<RwLock<Option<Arc<u64>>>>> =
        rustc_hash::FxHashMap::default();
    sparse_map.insert(1000, Arc::new(RwLock::new(Some(Arc::new(42)))));

    let start = Instant::now();
    for _ in 0..1000 {
        let _guard = dense_vec[1000].read().unwrap();
    }
    let dense_access = start.elapsed();
    println!("Dense Vec access (1000x): {:?}", dense_access);

    let start = Instant::now();
    for _ in 0..1000 {
        if let Some(entry) = sparse_map.get(&1000) {
            let _guard = entry.read().unwrap();
        }
    }
    let sparse_access = start.elapsed();
    println!("Sparse HashMap access (1000x): {:?}", sparse_access);
}
