//! Example: Vec-backed Arena Storage
//!
//! This example demonstrates how to use `VecStorage` for dynamic arena capacity
//! at runtime. This is useful when you don't know the arena size at compile time.
//!
//! Run with: cargo run --example vec_storage

use grift_arena::{GenericArena, VecStorage};

fn main() {
    println!("=== Vec-backed Arena Storage Example ===\n");
    
    // Create storage with runtime-determined capacity
    let capacity = 10000;
    let storage = VecStorage::<isize>::with_capacity(capacity);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);
    
    println!("Created arena with capacity: {}", arena.capacity());
    
    // Allocate some values
    let idx1 = arena.alloc(42).unwrap();
    let idx2 = arena.alloc(100).unwrap();
    let idx3 = arena.alloc(-50).unwrap();
    
    println!("\nAllocated 3 values:");
    println!("  idx1 = {}", arena.get(idx1).unwrap());
    println!("  idx2 = {}", arena.get(idx2).unwrap());
    println!("  idx3 = {}", arena.get(idx3).unwrap());
    
    // Show stats
    let stats = arena.stats();
    println!("\nArena stats:");
    println!("  Capacity:  {}", stats.capacity);
    println!("  Allocated: {}", stats.allocated);
    println!("  Free:      {}", stats.free);
    println!("  Usage:     {:.2}%", stats.usage_percent());
    
    // Modify a value
    arena.modify(idx1, |v| *v += 8).unwrap();
    println!("\nAfter modifying idx1: {}", arena.get(idx1).unwrap());
    
    // Free a value
    arena.free(idx2).unwrap();
    println!("\nFreed idx2. Remaining allocated: {}", arena.len());
    
    // Reuse freed slot
    let idx4 = arena.alloc(999).unwrap();
    println!("Allocated new value: {} at idx4", arena.get(idx4).unwrap());
    
    // Iterate over all values
    println!("\nAll values in arena:");
    for (idx, value) in arena.iter() {
        println!("  [{}] = {}", idx.raw(), value);
    }
    
    // Contiguous allocation example
    let start = arena.alloc_contiguous(5, 0).unwrap();
    println!("\nAllocated 5 contiguous slots starting at {}", start.raw());
    
    for i in 0..5 {
        let idx = arena.index_at_offset(start, i).unwrap();
        arena.set(idx, (i * 10) as isize).unwrap();
    }
    
    println!("Set contiguous values:");
    for i in 0..5 {
        let idx = arena.index_at_offset(start, i).unwrap();
        println!("  offset {} = {}", i, arena.get(idx).unwrap());
    }
    
    // Final stats
    let final_stats = arena.stats();
    println!("\nFinal arena stats:");
    println!("  Allocated: {} / {}", final_stats.allocated, final_stats.capacity);
    println!("  Usage:     {:.4}%", final_stats.usage_percent());
    
    println!("\n=== Example Complete ===");
}
