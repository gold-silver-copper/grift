//! Tests for VecStorage backend.
//!
//! These tests verify that the Vec-backed arena works correctly for std users.

#![cfg(feature = "std")]

use grift_arena::{GenericArena, VecStorage, ArenaError};

#[test]
fn test_vec_storage_basic_allocation() {
    let storage = VecStorage::<isize>::with_capacity(100);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    let idx1 = arena.alloc(42).unwrap();
    let idx2 = arena.alloc(43).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), 42);
    assert_eq!(arena.get(idx2).unwrap(), 43);
    assert_eq!(arena.len(), 2);
    assert_eq!(arena.capacity(), 100);
}

#[test]
fn test_vec_storage_free_and_reuse() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    let idx1 = arena.alloc(42).unwrap();
    assert_eq!(arena.len(), 1);

    arena.free(idx1).unwrap();
    assert_eq!(arena.len(), 0);

    let idx2 = arena.alloc(43).unwrap();
    assert_eq!(arena.len(), 1);
    assert_eq!(arena.get(idx2).unwrap(), 43);
}

#[test]
fn test_vec_storage_out_of_memory() {
    let storage = VecStorage::<isize>::with_capacity(3);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    assert!(arena.alloc(1).is_ok());
    assert!(arena.alloc(2).is_ok());
    assert!(arena.alloc(3).is_ok());
    assert_eq!(arena.alloc(4), Err(ArenaError::OutOfMemory));
}

#[test]
fn test_vec_storage_clear() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    arena.alloc(1).unwrap();
    arena.alloc(2).unwrap();
    arena.alloc(3).unwrap();

    assert_eq!(arena.len(), 3);

    arena.clear();

    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());
}

#[test]
fn test_vec_storage_stats() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    arena.alloc(1).unwrap();
    arena.alloc(2).unwrap();

    let stats = arena.stats();
    assert_eq!(stats.capacity, 10);
    assert_eq!(stats.allocated, 2);
    assert_eq!(stats.free, 8);
    assert_eq!(stats.usage_percent(), 20.0);
}

#[test]
fn test_vec_storage_contiguous_alloc() {
    let storage = VecStorage::<isize>::with_capacity(100);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    // Allocate 5 contiguous slots
    let start = arena.alloc_contiguous(5, 0).unwrap();

    assert_eq!(arena.len(), 5);

    // Set values in contiguous block
    for i in 0..5 {
        let idx = arena.index_at_offset(start, i).unwrap();
        arena.set(idx, (i * 10) as isize).unwrap();
    }

    // Verify values
    for i in 0..5 {
        let idx = arena.index_at_offset(start, i).unwrap();
        assert_eq!(arena.get(idx).unwrap(), (i * 10) as isize);
    }

    // Free contiguous block
    arena.free_contiguous(start, 5).unwrap();
    assert_eq!(arena.len(), 0);
}

#[test]
fn test_vec_storage_iter() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    arena.alloc(1).unwrap();
    arena.alloc(2).unwrap();
    arena.alloc(3).unwrap();

    let values: Vec<isize> = arena.iter().map(|(_, v)| v).collect();
    assert_eq!(values, vec![1, 2, 3]);
}

#[test]
fn test_vec_storage_modify() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    let idx = arena.alloc(42).unwrap();
    assert_eq!(arena.get(idx).unwrap(), 42);

    arena.modify(idx, |v| *v += 10).unwrap();
    assert_eq!(arena.get(idx).unwrap(), 52);
}

#[test]
fn test_vec_storage_swap() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    let idx1 = arena.alloc(1).unwrap();
    let idx2 = arena.alloc(2).unwrap();

    arena.swap(idx1, idx2).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), 2);
    assert_eq!(arena.get(idx2).unwrap(), 1);
}

#[test]
fn test_vec_storage_find() {
    let storage = VecStorage::<isize>::with_capacity(10);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    arena.alloc(1).unwrap();
    arena.alloc(42).unwrap();
    arena.alloc(3).unwrap();

    let result = arena.find(|v| *v == 42);
    assert!(result.is_some());
    let (_, value) = result.unwrap();
    assert_eq!(value, 42);

    let not_found = arena.find(|v| *v == 999);
    assert!(not_found.is_none());
}

#[test]
fn test_vec_storage_with_struct() {
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Point {
        x: i32,
        y: i32,
    }

    let storage = VecStorage::<Point>::with_capacity(100);
    let arena: GenericArena<Point, VecStorage<Point>> = GenericArena::with_storage(storage);

    let p1 = arena.alloc(Point { x: 1, y: 2 }).unwrap();
    let p2 = arena.alloc(Point { x: 3, y: 4 }).unwrap();

    assert_eq!(arena.get(p1).unwrap(), Point { x: 1, y: 2 });
    assert_eq!(arena.get(p2).unwrap(), Point { x: 3, y: 4 });
}

#[test]
fn test_vec_storage_large_capacity() {
    // Test with a larger capacity to verify Vec works well
    let storage = VecStorage::<isize>::with_capacity(10000);
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::with_storage(storage);

    // Allocate many items
    for i in 0..1000 {
        let idx = arena.alloc(i).unwrap();
        assert_eq!(arena.get(idx).unwrap(), i);
    }

    assert_eq!(arena.len(), 1000);
    assert_eq!(arena.capacity(), 10000);
}

#[test]
fn test_vec_storage_new_default() {
    // Test using GenericArena::new which uses the default capacity
    let arena: GenericArena<isize, VecStorage<isize>> = GenericArena::new(0);
    
    // The default capacity should be 1024
    assert_eq!(arena.capacity(), 1024);
    
    let idx = arena.alloc(42).unwrap();
    assert_eq!(arena.get(idx).unwrap(), 42);
}
