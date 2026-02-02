//! Example: Vec-backed Arena Storage
//!
//! This example demonstrates how to use `VecStorage` for dynamic arena capacity
//! at runtime. This requires the `std` feature.
//!
//! Run with: cargo run --example vec_storage_demo --features std

use grift::{GenericArena, VecStorage};

fn main() {
    println!("=== Vec-backed Arena Storage Demo ===\n");
    
    // Create storage with runtime-determined capacity
    let capacity = 10000;
    let storage = VecStorage::<isize>::with_capacity(capacity);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);
    
    println!("Created arena with dynamic capacity: {}", arena.capacity());
    
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
    
    // Free a value and reuse
    arena.free(idx2).unwrap();
    let idx4 = arena.alloc(999).unwrap();
    println!("\nFreed and reallocated: {}", arena.get(idx4).unwrap());
    
    // Iterate over all values
    println!("\nAll values in arena:");
    for (idx, value) in arena.iter() {
        println!("  [{}] = {}", idx.raw(), value);
    }
    
    // Compare with fixed-size array (the default)
    use grift::Arena;
    let fixed_arena: Arena<isize, 1000> = Arena::new(0);
    let _idx = fixed_arena.alloc(123).unwrap();
    println!("\nFixed-size arena capacity: {}", fixed_arena.capacity());
    
    println!("\n=== Demo Complete ===");
}
