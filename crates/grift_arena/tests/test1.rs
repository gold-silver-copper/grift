// tests/arena_tests.rs

use grift_arena::{
    Arena, ArenaCopy, ArenaDelete, ArenaError, ArenaIndex, ArenaResult, ArenaStats, GcStats, Slotted, Trace,
};


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Val {
    Free(usize),
    V(isize),
}

impl Slotted for Val {
    fn is_free(&self) -> bool { matches!(self, Val::Free(_)) }
    fn next_free(&self) -> usize { match self { Val::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { Val::Free(next) }
}

// ============================================================================
// Basic Functionality Tests
// ============================================================================

#[test]
fn test_new_arena_is_empty() {
    let arena: Arena<Val, 10> = Arena::new();
    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());
    assert!(!arena.is_full());
    assert_eq!(arena.capacity(), 10);
    assert_eq!(arena.available(), 10);
}

#[test]
fn test_basic_allocation() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(42)).unwrap();
    let idx2 = arena.alloc(Val::V(43)).unwrap();
    let idx3 = arena.alloc(Val::V(44)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Val::V(42));
    assert_eq!(arena.get(idx2).unwrap(), Val::V(43));
    assert_eq!(arena.get(idx3).unwrap(), Val::V(44));
    assert_eq!(arena.len(), 3);
    assert_eq!(arena.available(), 7);
}

#[test]
fn test_alloc_returns_different_indices() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();

    assert_ne!(idx1, idx2);
    assert_ne!(idx2, idx3);
    assert_ne!(idx1, idx3);
}

#[test]
fn test_out_of_memory() {
    let arena: Arena<Val, 3> = Arena::new();

    assert!(arena.alloc(Val::V(1)).is_ok());
    assert!(arena.alloc(Val::V(2)).is_ok());
    assert!(arena.alloc(Val::V(3)).is_ok());

    assert_eq!(arena.alloc(Val::V(4)), Err(ArenaError::OutOfMemory));
    assert!(arena.is_full());
    assert_eq!(arena.available(), 0);
}

#[test]
fn test_get_invalid_index() {
    let arena: Arena<Val, 10> = Arena::new();

    // Out of bounds index
    let invalid_idx = ArenaIndex::new(100);
    assert_eq!(arena.get(invalid_idx), Err(ArenaError::IndexOutOfBounds));
}

#[test]
fn test_get_freed_index() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    arena.free(idx).unwrap();

    // After freeing, the index is invalid
    assert_eq!(arena.get(idx), Err(ArenaError::IndexNotAllocated));
}

// ============================================================================
// Free and Reuse Tests
// ============================================================================

#[test]
fn test_free_and_reuse() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(42)).unwrap();
    assert_eq!(arena.len(), 1);

    arena.free(idx1).unwrap();
    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());

    let idx2 = arena.alloc(Val::V(43)).unwrap();
    assert_eq!(arena.len(), 1);
    assert_eq!(arena.get(idx2).unwrap(), Val::V(43));
}

#[test]
fn test_free_invalid_index() {
    let arena: Arena<Val, 10> = Arena::new();

    // Out of bounds index
    let invalid_idx = ArenaIndex::new(100);
    assert_eq!(arena.free(invalid_idx), Err(ArenaError::IndexOutOfBounds));
}

#[test]
fn test_double_free() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    arena.free(idx).unwrap();

    // Double free returns IndexNotAllocated because the slot is no longer allocated
    assert_eq!(arena.free(idx), Err(ArenaError::IndexNotAllocated));
}

#[test]
fn test_fragmentation_and_reuse() {
    let arena: Arena<Val, 10> = Arena::new();

    // Allocate 5 items
    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();
    let idx4 = arena.alloc(Val::V(4)).unwrap();
    let idx5 = arena.alloc(Val::V(5)).unwrap();

    // Free every other one
    arena.free(idx2).unwrap();
    arena.free(idx4).unwrap();

    assert_eq!(arena.len(), 3);
    assert_eq!(arena.available(), 7);

    // Should be able to reuse freed slots
    let idx6 = arena.alloc(Val::V(6)).unwrap();
    let idx7 = arena.alloc(Val::V(7)).unwrap();

    assert_eq!(arena.len(), 5);
    assert_eq!(arena.get(idx6).unwrap(), Val::V(6));
    assert_eq!(arena.get(idx7).unwrap(), Val::V(7));
}

// ============================================================================
// Set Tests
// ============================================================================

#[test]
fn test_set_value() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(42));

    arena.set(idx, Val::V(100)).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(100));

    arena.set(idx, Val::V(-50)).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(-50));
}

#[test]
fn test_set_invalid_index() {
    let arena: Arena<Val, 10> = Arena::new();

    // Out of bounds index
    let invalid_idx = ArenaIndex::new(100);
    assert_eq!(
        arena.set(invalid_idx, Val::V(42)),
        Err(ArenaError::IndexOutOfBounds)
    );
}

#[test]
fn test_set_freed_index() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    arena.free(idx).unwrap();

    // After freeing, the index is invalid
    assert_eq!(arena.set(idx, Val::V(100)), Err(ArenaError::IndexNotAllocated));
}

// ============================================================================
// Clear Tests
// ============================================================================

#[test]
fn test_clear() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();

    assert_eq!(arena.len(), 3);

    arena.clear();

    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());
    assert_eq!(arena.available(), 10);
}

#[test]
fn test_clear_allows_full_reuse() {
    let arena: Arena<Val, 5> = Arena::new();

    // Fill the arena
    for i in 0..5 {
        arena.alloc(Val::V(i)).unwrap();
    }
    assert!(arena.is_full());

    arena.clear();

    // Should be able to allocate again
    for i in 0..5 {
        assert!(arena.alloc(Val::V(i * 10)).is_ok());
    }
    assert!(arena.is_full());
}

// ============================================================================
// Iterator Tests
// ============================================================================

#[test]
fn test_iter_empty() {
    let arena: Arena<Val, 10> = Arena::new();
    let count = arena.iter().count();
    assert_eq!(count, 0);
}

#[test]
fn test_iter_values() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();

    let values: Vec<Val> = arena.iter().map(|(_, v)| v).collect();
    assert_eq!(values.len(), 3);
    assert!(values.contains(&Val::V(1)));
    assert!(values.contains(&Val::V(2)));
    assert!(values.contains(&Val::V(3)));
}

#[test]
fn test_iter_indices() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(10)).unwrap();
    let idx2 = arena.alloc(Val::V(20)).unwrap();
    let idx3 = arena.alloc(Val::V(30)).unwrap();

    let indices: Vec<ArenaIndex> = arena.iter().map(|(i, _)| i).collect();
    assert_eq!(indices.len(), 3);
    assert!(indices.contains(&idx1));
    assert!(indices.contains(&idx2));
    assert!(indices.contains(&idx3));
}

#[test]
fn test_iter_with_gaps() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();
    let idx4 = arena.alloc(Val::V(4)).unwrap();

    // Free middle items
    arena.free(idx2).unwrap();
    arena.free(idx3).unwrap();

    let values: Vec<Val> = arena.iter().map(|(_, v)| v).collect();
    assert_eq!(values.len(), 2);
    assert!(values.contains(&Val::V(1)));
    assert!(values.contains(&Val::V(4)));
    assert!(!values.contains(&Val::V(2)));
    assert!(!values.contains(&Val::V(3)));
}

// ============================================================================
// Statistics Tests
// ============================================================================

#[test]
fn test_stats_empty() {
    let arena: Arena<Val, 10> = Arena::new();
    let stats = arena.stats();

    assert_eq!(stats.capacity, 10);
    assert_eq!(stats.allocated, 0);
    assert_eq!(stats.free, 10);
    assert_eq!(stats.usage_percent(), 0.0);
}

#[test]
fn test_stats_partial() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();

    let stats = arena.stats();
    assert_eq!(stats.capacity, 10);
    assert_eq!(stats.allocated, 2);
    assert_eq!(stats.free, 8);
    assert_eq!(stats.usage_percent(), 20.0);
}

#[test]
fn test_stats_full() {
    let arena: Arena<Val, 5> = Arena::new();

    for i in 0..5 {
        arena.alloc(Val::V(i)).unwrap();
    }

    let stats = arena.stats();
    assert_eq!(stats.capacity, 5);
    assert_eq!(stats.allocated, 5);
    assert_eq!(stats.free, 0);
    assert_eq!(stats.usage_percent(), 100.0);
}

// ============================================================================
// is_allocated Tests
// ============================================================================

#[test]
fn test_is_allocated() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    assert!(arena.is_allocated(idx));

    arena.free(idx).unwrap();
    assert!(!arena.is_allocated(idx));
}

#[test]
fn test_is_allocated_invalid_index() {
    let arena: Arena<Val, 10> = Arena::new();

    // Out of bounds index
    let invalid_idx = ArenaIndex::new(100);
    assert!(!arena.is_allocated(invalid_idx));
}

// ============================================================================
// Type Tests (different types)
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fval {
    Free(usize),
    V(f64),
}

impl Slotted for Fval {
    fn is_free(&self) -> bool { matches!(self, Fval::Free(_)) }
    fn next_free(&self) -> usize { match self { Fval::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { Fval::Free(next) }
}

#[test]
fn test_arena_with_floats() {
    let arena: Arena<Fval, 10> = Arena::new();

    let idx1 = arena.alloc(Fval::V(3.14)).unwrap();
    let idx2 = arena.alloc(Fval::V(2.71828)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Fval::V(3.14));
    assert_eq!(arena.get(idx2).unwrap(), Fval::V(2.71828));
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum PointSlot {
    Free(usize),
    P { x: isize, y: isize },
}

impl Slotted for PointSlot {
    fn is_free(&self) -> bool { matches!(self, PointSlot::Free(_)) }
    fn next_free(&self) -> usize { match self { PointSlot::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { PointSlot::Free(next) }
}

#[test]
fn test_arena_with_struct() {
    let arena: Arena<PointSlot, 10> = Arena::new();

    let p1 = PointSlot::P { x: 10, y: 20 };
    let p2 = PointSlot::P { x: 30, y: 40 };

    let idx1 = arena.alloc(p1).unwrap();
    let idx2 = arena.alloc(p2).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), p1);
    assert_eq!(arena.get(idx2).unwrap(), p2);
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Color {
    Free(usize),
    Red,
    Green,
    Blue,
    RGB(u8, u8, u8),
}

impl Slotted for Color {
    fn is_free(&self) -> bool { matches!(self, Color::Free(_)) }
    fn next_free(&self) -> usize { match self { Color::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { Color::Free(next) }
}

#[test]
fn test_arena_with_enum() {
    let arena: Arena<Color, 10> = Arena::new();

    let idx1 = arena.alloc(Color::Green).unwrap();
    let idx2 = arena.alloc(Color::RGB(128, 255, 64)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Color::Green);
    assert_eq!(arena.get(idx2).unwrap(), Color::RGB(128, 255, 64));
}

// ============================================================================
// Recursive Tree Tests
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum Tree {
    Free(usize),
    Leaf(isize),
    Branch(ArenaIndex, ArenaIndex),
}

impl Slotted for Tree {
    fn is_free(&self) -> bool { matches!(self, Tree::Free(_)) }
    fn next_free(&self) -> usize { match self { Tree::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { Tree::Free(next) }
}

impl ArenaDelete<Tree, 100> for Tree {
    fn delete_recursive(&self, arena: &Arena<Tree, 100>) -> ArenaResult<()> {
        match *self {
            Tree::Free(_) | Tree::Leaf(_) => Ok(()),
            Tree::Branch(left, right) => {
                arena.delete_recursive(left)?;
                arena.delete_recursive(right)?;
                Ok(())
            }
        }
    }
}

impl ArenaCopy<Tree, 100> for Tree {
    fn copy_deep(&self, arena: &Arena<Tree, 100>) -> ArenaResult<Tree> {
        match *self {
            Tree::Free(n) => Ok(Tree::Free(n)),
            Tree::Leaf(n) => Ok(Tree::Leaf(n)),
            Tree::Branch(left, right) => {
                let new_left = arena.copy_deep(left)?;
                let new_right = arena.copy_deep(right)?;
                Ok(Tree::Branch(new_left, new_right))
            }
        }
    }
}

#[test]
fn test_simple_tree() {
    let arena: Arena<Tree, 100> = Arena::new();

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    match arena.get(root).unwrap() {
        Tree::Branch(l, r) => {
            assert_eq!(arena.get(l).unwrap(), Tree::Leaf(1));
            assert_eq!(arena.get(r).unwrap(), Tree::Leaf(2));
        }
        _ => panic!("Expected branch"),
    }
}

#[test]
fn test_recursive_delete_leaf() {
    let arena: Arena<Tree, 100> = Arena::new();

    let leaf = arena.alloc(Tree::Leaf(42)).unwrap();
    assert_eq!(arena.len(), 1);

    arena.delete_recursive(leaf).unwrap();
    assert_eq!(arena.len(), 0);
}

#[test]
fn test_recursive_delete_simple_tree() {
    let arena: Arena<Tree, 100> = Arena::new();

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    assert_eq!(arena.len(), 3);

    arena.delete_recursive(root).unwrap();

    assert_eq!(arena.len(), 0);
}

#[test]
fn test_recursive_delete_complex_tree() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Build tree: ((1, 2), (3, 4))
    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let left_branch = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();

    let leaf3 = arena.alloc(Tree::Leaf(3)).unwrap();
    let leaf4 = arena.alloc(Tree::Leaf(4)).unwrap();
    let right_branch = arena.alloc(Tree::Branch(leaf3, leaf4)).unwrap();

    let root = arena
        .alloc(Tree::Branch(left_branch, right_branch))
        .unwrap();

    assert_eq!(arena.len(), 7);

    arena.delete_recursive(root).unwrap();

    assert_eq!(arena.len(), 0);
}

#[test]
fn test_deep_copy_leaf() {
    let arena: Arena<Tree, 100> = Arena::new();

    let leaf = arena.alloc(Tree::Leaf(42)).unwrap();
    let copied = arena.copy_deep(leaf).unwrap();

    assert_eq!(arena.len(), 2);
    assert_eq!(arena.get(copied).unwrap(), Tree::Leaf(42));
    assert_ne!(leaf, copied);
}

#[test]
fn test_deep_copy_simple_tree() {
    let arena: Arena<Tree, 100> = Arena::new();

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    let copied_root = arena.copy_deep(root).unwrap();

    // Should have 6 nodes total (3 original + 3 copied)
    assert_eq!(arena.len(), 6);

    // Verify structure is copied
    match arena.get(copied_root).unwrap() {
        Tree::Branch(cl, cr) => {
            assert_eq!(arena.get(cl).unwrap(), Tree::Leaf(1));
            assert_eq!(arena.get(cr).unwrap(), Tree::Leaf(2));
            // Indices should be different
            assert_ne!(cl, left);
            assert_ne!(cr, right);
        }
        _ => panic!("Expected branch"),
    }
}

#[test]
fn test_deep_copy_complex_tree() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Build tree: ((1, 2), (3, 4))
    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let left_branch = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();

    let leaf3 = arena.alloc(Tree::Leaf(3)).unwrap();
    let leaf4 = arena.alloc(Tree::Leaf(4)).unwrap();
    let right_branch = arena.alloc(Tree::Branch(leaf3, leaf4)).unwrap();

    let root = arena
        .alloc(Tree::Branch(left_branch, right_branch))
        .unwrap();

    assert_eq!(arena.len(), 7);

    let copied_root = arena.copy_deep(root).unwrap();

    // Should have 14 nodes total (7 original + 7 copied)
    assert_eq!(arena.len(), 14);

    // Verify copied tree has same values
    match arena.get(copied_root).unwrap() {
        Tree::Branch(cl_branch, cr_branch) => {
            match arena.get(cl_branch).unwrap() {
                Tree::Branch(cl1, cl2) => {
                    assert_eq!(arena.get(cl1).unwrap(), Tree::Leaf(1));
                    assert_eq!(arena.get(cl2).unwrap(), Tree::Leaf(2));
                }
                _ => panic!("Expected branch"),
            }
            match arena.get(cr_branch).unwrap() {
                Tree::Branch(cr3, cr4) => {
                    assert_eq!(arena.get(cr3).unwrap(), Tree::Leaf(3));
                    assert_eq!(arena.get(cr4).unwrap(), Tree::Leaf(4));
                }
                _ => panic!("Expected branch"),
            }
        }
        _ => panic!("Expected branch"),
    }
}

// ============================================================================
// Edge Cases and Stress Tests
// ============================================================================

#[test]
fn test_single_cell_arena() {
    let arena: Arena<Val, 1> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    assert!(arena.is_full());
    assert_eq!(arena.alloc(Val::V(43)), Err(ArenaError::OutOfMemory));

    assert_eq!(arena.get(idx).unwrap(), Val::V(42));

    arena.free(idx).unwrap();
    assert!(arena.is_empty());

    let idx2 = arena.alloc(Val::V(100)).unwrap();
    assert_eq!(arena.get(idx2).unwrap(), Val::V(100));
}

#[test]
fn test_alternating_alloc_free() {
    let arena: Arena<Val, 10> = Arena::new();

    for i in 0..100 {
        let idx = arena.alloc(Val::V(i)).unwrap();
        assert_eq!(arena.len(), 1);
        assert_eq!(arena.get(idx).unwrap(), Val::V(i));
        arena.free(idx).unwrap();
        assert_eq!(arena.len(), 0);
    }
}

#[test]
fn test_fill_and_empty_repeatedly() {
    let arena: Arena<Val, 5> = Arena::new();

    for round in 0..10 {
        let mut indices = Vec::new();

        // Fill arena
        for i in 0..5 {
            let idx = arena.alloc(Val::V(round * 10 + i)).unwrap();
            indices.push(idx);
        }
        assert!(arena.is_full());

        // Empty arena
        for idx in indices {
            arena.free(idx).unwrap();
        }
        assert!(arena.is_empty());
    }
}

#[test]
fn test_index_consistency() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(100)).unwrap();
    let idx2 = arena.alloc(Val::V(200)).unwrap();
    let idx3 = arena.alloc(Val::V(300)).unwrap();

    // Indices should remain valid and consistent
    assert_eq!(arena.get(idx1).unwrap(), Val::V(100));
    assert_eq!(arena.get(idx2).unwrap(), Val::V(200));
    assert_eq!(arena.get(idx3).unwrap(), Val::V(300));

    // Even after modifying other values
    arena.set(idx2, Val::V(250)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Val::V(100));
    assert_eq!(arena.get(idx2).unwrap(), Val::V(250));
    assert_eq!(arena.get(idx3).unwrap(), Val::V(300));
}

#[test]
fn test_arena_index_api() {
    let idx = ArenaIndex::new(42);
    assert_eq!(idx.raw(), 42);

    // Test that indices with different slots are not equal
    let idx2 = ArenaIndex::new(43);
    assert_ne!(idx, idx2);
}

#[test]
fn test_large_arena() {
    let arena: Arena<Val, 1000> = Arena::new();

    // Allocate many items
    let mut indices = Vec::new();
    for i in 0..500 {
        let idx = arena.alloc(Val::V(i)).unwrap();
        indices.push(idx);
    }

    assert_eq!(arena.len(), 500);
    assert_eq!(arena.available(), 500);

    // Verify all values
    for (i, &idx) in indices.iter().enumerate() {
        assert_eq!(arena.get(idx).unwrap(), Val::V(i as isize));
    }

    // Free half
    for &idx in indices.iter().take(250) {
        arena.free(idx).unwrap();
    }

    assert_eq!(arena.len(), 250);
    assert_eq!(arena.available(), 750);
}

// ============================================================================
// ABA Problem Prevention Tests
// ============================================================================

#[test]
fn test_aba_problem_prevention() {
    let arena: Arena<Val, 10> = Arena::new();

    // Allocate slot
    let old_idx = arena.alloc(Val::V(100)).unwrap();
    let old_raw = old_idx.raw();

    // Free it
    arena.free(old_idx).unwrap();

    // Old index should now be invalid (slot is free)
    assert_eq!(arena.get(old_idx), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.set(old_idx, Val::V(999)), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.free(old_idx), Err(ArenaError::IndexNotAllocated));

    // Allocate new value - will reuse the same slot
    let new_idx = arena.alloc(Val::V(200)).unwrap();

    // Verify same slot is reused (free-list LIFO)
    assert_eq!(new_idx.raw(), old_raw);

    // New index should work
    assert_eq!(arena.get(new_idx).unwrap(), Val::V(200));
}

#[test]
fn test_stale_index_after_multiple_reuses() {
    let arena: Arena<Val, 3> = Arena::new();

    // Get initial index
    let stale_idx = arena.alloc(Val::V(1)).unwrap();
    arena.free(stale_idx).unwrap();

    // Reuse slot multiple times
    for i in 0..10 {
        let idx = arena.alloc(Val::V(i * 100)).unwrap();
        arena.free(idx).unwrap();
    }

    // Original stale index should still be invalid
    assert_eq!(arena.get(stale_idx), Err(ArenaError::IndexNotAllocated));
}

#[test]
fn test_iterator_indices_after_free_realloc() {
    let arena: Arena<Val, 5> = Arena::new();

    let idx1 = arena.alloc(Val::V(10)).unwrap();
    let idx2 = arena.alloc(Val::V(20)).unwrap();
    let idx3 = arena.alloc(Val::V(30)).unwrap();

    // Free middle one
    arena.free(idx2).unwrap();

    // Reallocate
    let idx4 = arena.alloc(Val::V(40)).unwrap();

    // Iterator should return current valid indices
    let iter_indices: Vec<ArenaIndex> = arena.iter().map(|(i, _)| i).collect();

    assert!(iter_indices.contains(&idx1));
    // Without generations, idx4 reuses the same slot as idx2, so idx2 == idx4
    assert_eq!(idx2, idx4);
    assert!(iter_indices.contains(&idx4));
    assert!(iter_indices.contains(&idx3));
}

// ============================================================================
// Free-List Behavior Tests
// ============================================================================

#[test]
fn test_free_list_lifo_order() {
    let arena: Arena<Val, 5> = Arena::new();

    // Allocate all slots
    let idx0 = arena.alloc(Val::V(0)).unwrap();
    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();
    let idx4 = arena.alloc(Val::V(4)).unwrap();

    // Free in order: 1, 3, 0
    arena.free(idx1).unwrap();
    arena.free(idx3).unwrap();
    arena.free(idx0).unwrap();

    // Reallocate - should come back in LIFO order: 0, 3, 1
    let new1 = arena.alloc(Val::V(100)).unwrap();
    let new2 = arena.alloc(Val::V(200)).unwrap();
    let new3 = arena.alloc(Val::V(300)).unwrap();

    assert_eq!(new1.raw(), idx0.raw()); // Last freed, first allocated
    assert_eq!(new2.raw(), idx3.raw());
    assert_eq!(new3.raw(), idx1.raw());
}

#[test]
fn test_free_list_integrity_after_clear() {
    let arena: Arena<Val, 5> = Arena::new();

    // Allocate some
    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();

    arena.clear();

    // After clear, free list should be rebuilt
    // Should be able to allocate all slots in order 0, 1, 2, 3, 4
    let idx0 = arena.alloc(Val::V(10)).unwrap();
    let idx1 = arena.alloc(Val::V(20)).unwrap();
    let idx2 = arena.alloc(Val::V(30)).unwrap();
    let idx3 = arena.alloc(Val::V(40)).unwrap();
    let idx4 = arena.alloc(Val::V(50)).unwrap();

    assert_eq!(idx0.raw(), 0);
    assert_eq!(idx1.raw(), 1);
    assert_eq!(idx2.raw(), 2);
    assert_eq!(idx3.raw(), 3);
    assert_eq!(idx4.raw(), 4);
}

#[test]
fn test_free_list_no_corruption_on_partial_free() {
    let arena: Arena<Val, 10> = Arena::new();

    let mut indices = Vec::new();
    for i in 0..10 {
        indices.push(arena.alloc(Val::V(i)).unwrap());
    }

    // Free every other slot
    for i in (0..10).step_by(2) {
        arena.free(indices[i]).unwrap();
    }

    assert_eq!(arena.len(), 5);

    // Allocate 5 more - should fill exactly
    for i in 0..5 {
        assert!(arena.alloc(Val::V(100 + i)).is_ok());
    }

    assert!(arena.is_full());
    assert_eq!(arena.alloc(Val::V(999)), Err(ArenaError::OutOfMemory));
}

// ============================================================================
// Boundary and Edge Case Tests
// ============================================================================

#[test]
fn test_index_at_boundary() {
    let arena: Arena<Val, 10> = Arena::new();

    // Index at boundary - slot doesn't exist
    let boundary_not_allocated = ArenaIndex::new(9);
    assert_eq!(
        arena.get(boundary_not_allocated),
        Err(ArenaError::IndexNotAllocated)
    );

    // Index one past boundary - always IndexOutOfBounds
    let past_boundary = ArenaIndex::new(10);
    assert_eq!(arena.get(past_boundary), Err(ArenaError::IndexOutOfBounds));
}

#[test]
fn test_max_index_value() {
    let arena: Arena<Val, 10> = Arena::new();

    let huge_idx = ArenaIndex::new(usize::MAX);
    assert_eq!(arena.get(huge_idx), Err(ArenaError::IndexOutOfBounds));
    assert_eq!(arena.free(huge_idx), Err(ArenaError::IndexOutOfBounds));
    assert_eq!(arena.set(huge_idx, Val::V(42)), Err(ArenaError::IndexOutOfBounds));
}

#[test]
fn test_arena_index_equality() {
    let idx1 = ArenaIndex::new(5);
    let idx2 = ArenaIndex::new(5);
    let idx3 = ArenaIndex::new(6);

    assert_eq!(idx1, idx2);
    assert_ne!(idx1, idx3); // Different slot
}

#[test]
fn test_arena_index_hash() {
    use std::collections::HashSet;

    let mut set = HashSet::new();

    set.insert(ArenaIndex::new(0));
    set.insert(ArenaIndex::new(1));
    set.insert(ArenaIndex::new(2));

    assert_eq!(set.len(), 3);
    assert!(set.contains(&ArenaIndex::new(0)));
    assert!(set.contains(&ArenaIndex::new(1)));
    assert!(set.contains(&ArenaIndex::new(2)));
    assert!(!set.contains(&ArenaIndex::new(3)));
}

// ============================================================================
// Complex Allocation Patterns
// ============================================================================

#[test]
fn test_zigzag_allocation_pattern() {
    let arena: Arena<Val, 10> = Arena::new();

    // Allocate all
    let mut indices: Vec<ArenaIndex> = (0..10).map(|i| arena.alloc(Val::V(i)).unwrap()).collect();

    // Free odd indices
    for i in (1..10).step_by(2) {
        arena.free(indices[i]).unwrap();
    }

    // Free even indices
    for i in (0..10).step_by(2) {
        arena.free(indices[i]).unwrap();
    }

    assert!(arena.is_empty());

    // Reallocate all - should work
    indices = (0..10).map(|i| arena.alloc(Val::V(i * 10)).unwrap()).collect();

    assert!(arena.is_full());

    // Verify all values
    for (i, &idx) in indices.iter().enumerate() {
        assert_eq!(arena.get(idx).unwrap(), Val::V((i * 10) as isize));
    }
}

#[test]
fn test_random_like_access_pattern() {
    let arena: Arena<Val, 20> = Arena::new();

    let mut active_indices: Vec<ArenaIndex> = Vec::new();

    // Simulate random-like alloc/free pattern
    for i in 0..100 {
        if i % 3 == 0 && !active_indices.is_empty() {
            // Free oldest
            let idx = active_indices.remove(0);
            arena.free(idx).unwrap();
        } else if arena.available() > 0 {
            // Allocate
            let idx = arena.alloc(Val::V(i)).unwrap();
            active_indices.push(idx);
        }
    }

    // All active indices should still be valid
    for &idx in &active_indices {
        assert!(arena.is_allocated(idx));
    }

    // Count should match
    assert_eq!(arena.len(), active_indices.len());
}

#[test]
fn test_interleaved_alloc_free_set() {
    let arena: Arena<Val, 5> = Arena::new();

    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();

    arena.set(idx1, Val::V(10)).unwrap();

    let idx3 = arena.alloc(Val::V(3)).unwrap();

    arena.free(idx2).unwrap();
    arena.set(idx1, Val::V(100)).unwrap();
    arena.set(idx3, Val::V(300)).unwrap();

    // idx2 is now invalid
    assert_eq!(arena.get(idx2), Err(ArenaError::IndexNotAllocated));

    let idx4 = arena.alloc(Val::V(4)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Val::V(100));
    assert_eq!(arena.get(idx3).unwrap(), Val::V(300));
    assert_eq!(arena.get(idx4).unwrap(), Val::V(4));
    // Without generations, idx4 reuses idx2's slot, so idx2 == idx4
    assert_eq!(idx2, idx4);
}

// ============================================================================
// Tree Tests
// ============================================================================

#[test]
fn test_tree_with_stale_indices() {
    let arena: Arena<Tree, 100> = Arena::new();

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    // Free the entire tree
    arena.delete_recursive(root).unwrap();

    // All indices should now be stale
    assert_eq!(arena.get(root), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.get(left), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.get(right), Err(ArenaError::IndexNotAllocated));
}

#[test]
fn test_copy_then_delete_original() {
    let arena: Arena<Tree, 100> = Arena::new();

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    // Deep copy the tree
    let copied_root = arena.copy_deep(root).unwrap();

    // Delete original
    arena.delete_recursive(root).unwrap();

    // Original indices should be stale
    assert_eq!(arena.get(root), Err(ArenaError::IndexNotAllocated));

    // Copied tree should still work
    match arena.get(copied_root).unwrap() {
        Tree::Branch(cl, cr) => {
            assert_eq!(arena.get(cl).unwrap(), Tree::Leaf(1));
            assert_eq!(arena.get(cr).unwrap(), Tree::Leaf(2));
        }
        _ => panic!("Expected branch"),
    }
}

#[test]
fn test_partial_tree_delete() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Build tree: Branch(Branch(1, 2), Leaf(3))
    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let left_branch = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();
    let leaf3 = arena.alloc(Tree::Leaf(3)).unwrap();
    let root = arena.alloc(Tree::Branch(left_branch, leaf3)).unwrap();

    assert_eq!(arena.len(), 5);

    // Delete only the left branch (not the whole tree)
    arena.delete_recursive(left_branch).unwrap();

    assert_eq!(arena.len(), 2); // root and leaf3 remain

    // Root still exists but contains stale index for left branch
    match arena.get(root).unwrap() {
        Tree::Branch(l, r) => {
            assert_eq!(arena.get(l), Err(ArenaError::IndexNotAllocated)); // Stale
            assert_eq!(arena.get(r).unwrap(), Tree::Leaf(3)); // Valid
        }
        _ => panic!("Expected branch"),
    }
}

// ============================================================================
// Statistics with Free-List
// ============================================================================

#[test]
fn test_fragmentation_after_free_realloc() {
    let arena: Arena<Val, 10> = Arena::new();

    // Fill arena
    let indices: Vec<_> = (0..10).map(|i| arena.alloc(Val::V(i)).unwrap()).collect();

    // Create fragmentation by freeing every other slot
    for i in (0..10).step_by(2) {
        arena.free(indices[i]).unwrap();
    }

    let stats = arena.stats();
    assert_eq!(stats.allocated, 5);
    assert_eq!(stats.free, 5);
    assert!(stats.fragmentation > 0.0);

    // Reallocate to fill gaps
    for _ in 0..5 {
        arena.alloc(Val::V(99)).unwrap();
    }

    let stats_after = arena.stats();
    assert_eq!(stats_after.allocated, 10);
    assert_eq!(stats_after.fragmentation, 0.0);
}

// ============================================================================
// Stress Tests
// ============================================================================

#[test]
fn test_stress_rapid_alloc_free() {
    let arena: Arena<Val, 100> = Arena::new();

    for _ in 0..1000 {
        let mut indices = Vec::new();

        // Allocate random amount (up to half capacity)
        let count = 50;
        for i in 0..count {
            indices.push(arena.alloc(Val::V(i)).unwrap());
        }

        // Free all
        for idx in indices {
            arena.free(idx).unwrap();
        }

        assert!(arena.is_empty());
    }
}

#[test]
fn test_stress_mixed_operations() {
    let arena: Arena<Val, 50> = Arena::new();

    let mut valid_indices: Vec<ArenaIndex> = Vec::new();

    for i in 0..500 {
        match i % 5 {
            0 | 1 | 2 => {
                // Allocate
                if let Ok(idx) = arena.alloc(Val::V(i as isize)) {
                    valid_indices.push(idx);
                }
            }
            3 => {
                // Free oldest if available
                if !valid_indices.is_empty() {
                    let idx = valid_indices.remove(0);
                    arena.free(idx).unwrap();
                }
            }
            4 => {
                // Set random valid index
                if !valid_indices.is_empty() {
                    let idx = valid_indices[i % valid_indices.len()];
                    arena.set(idx, Val::V(i as isize * 10)).unwrap();
                }
            }
            _ => unreachable!(),
        }

        // Invariant: len should match valid_indices count
        assert_eq!(arena.len(), valid_indices.len());
    }
}

// ============================================================================
// Garbage Collection Tests
// ============================================================================

// Implement Trace for Tree (already defined above)
impl<const N: usize> Trace<Tree, N> for Tree {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        match *self {
            Tree::Free(_) | Tree::Leaf(_) => {} // No references
            Tree::Branch(left, right) => {
                tracer(left);
                tracer(right);
            }
        }
    }
}

#[test]
fn test_gc_basic_collection() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Build a tree: root -> (leaf1, leaf2)
    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();

    // Allocate some garbage (not reachable from root)
    let garbage1 = arena.alloc(Tree::Leaf(999)).unwrap();
    let garbage2 = arena.alloc(Tree::Leaf(888)).unwrap();
    let _garbage3 = arena.alloc(Tree::Leaf(777)).unwrap();

    assert_eq!(arena.len(), 6);

    // Collect garbage
    let stats = arena.collect_garbage(&[root]);

    assert_eq!(stats.total_before, 6);
    assert_eq!(stats.marked, 3); // root, leaf1, leaf2
    assert_eq!(stats.collected, 3); // garbage1, garbage2, garbage3

    assert_eq!(arena.len(), 3);

    // Verify the tree is still intact
    assert_eq!(arena.get(root).unwrap(), Tree::Branch(leaf1, leaf2));
    assert_eq!(arena.get(leaf1).unwrap(), Tree::Leaf(1));
    assert_eq!(arena.get(leaf2).unwrap(), Tree::Leaf(2));

    // Garbage should be gone
    assert_eq!(arena.get(garbage1), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.get(garbage2), Err(ArenaError::IndexNotAllocated));
}

#[test]
fn test_gc_no_garbage() {
    let arena: Arena<Tree, 100> = Arena::new();

    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();

    // No garbage - all objects reachable
    let stats = arena.collect_garbage(&[root]);

    assert_eq!(stats.marked, 3);
    assert_eq!(stats.collected, 0);
    assert_eq!(arena.len(), 3);
}

#[test]
fn test_gc_all_garbage() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Allocate objects but don't keep any roots
    arena.alloc(Tree::Leaf(1)).unwrap();
    arena.alloc(Tree::Leaf(2)).unwrap();
    arena.alloc(Tree::Leaf(3)).unwrap();

    assert_eq!(arena.len(), 3);

    // Collect with no roots - everything is garbage
    let stats = arena.collect_garbage(&[]);

    assert_eq!(stats.marked, 0);
    assert_eq!(stats.collected, 3);
    assert_eq!(arena.len(), 0);
}

#[test]
fn test_gc_multiple_roots() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Two separate trees
    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let root1 = arena.alloc(Tree::Branch(leaf1, leaf1)).unwrap();

    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let root2 = arena.alloc(Tree::Branch(leaf2, leaf2)).unwrap();

    // Some garbage
    arena.alloc(Tree::Leaf(999)).unwrap();

    assert_eq!(arena.len(), 5);

    // Both roots reachable
    let stats = arena.collect_garbage(&[root1, root2]);

    assert_eq!(stats.marked, 4); // root1, leaf1, root2, leaf2
    assert_eq!(stats.collected, 1);
    assert_eq!(arena.len(), 4);
}

#[test]
fn test_gc_handles_cycles() {
    // Create a structure that could contain cycles
    #[derive(Clone, Copy)]
    enum Node {
        Free(usize),
        N { value: isize, left: Option<ArenaIndex>, right: Option<ArenaIndex> },
    }

    impl Slotted for Node {
        fn is_free(&self) -> bool { matches!(self, Node::Free(_)) }
        fn next_free(&self) -> usize { match self { Node::Free(n) => *n, _ => unreachable!() } }
        fn make_free(next: usize) -> Self { Node::Free(next) }
    }

    impl<const N: usize> Trace<Node, N> for Node {
        fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
            if let Node::N { left, right, .. } = self {
                if let Some(l) = left { tracer(*l); }
                if let Some(r) = right { tracer(*r); }
            }
        }
    }

    let arena: Arena<Node, 100> = Arena::new();

    // Create nodes
    let a = arena
        .alloc(Node::N {
            value: 1,
            left: None,
            right: None,
        })
        .unwrap();
    let b = arena
        .alloc(Node::N {
            value: 2,
            left: None,
            right: None,
        })
        .unwrap();

    // Create cycle: a -> b, b -> a
    arena
        .set(
            a,
            Node::N {
                value: 1,
                left: Some(b),
                right: None,
            },
        )
        .unwrap();
    arena
        .set(
            b,
            Node::N {
                value: 2,
                left: Some(a),
                right: None,
            },
        )
        .unwrap();

    // Add garbage
    arena
        .alloc(Node::N {
            value: 999,
            left: None,
            right: None,
        })
        .unwrap();

    assert_eq!(arena.len(), 3);

    // GC should handle the cycle without infinite loop
    let stats = arena.collect_garbage(&[a]);

    assert_eq!(stats.marked, 2); // a and b (cycle)
    assert_eq!(stats.collected, 1); // garbage
    assert_eq!(arena.len(), 2);
}

#[test]
fn test_gc_deep_tree() {
    let arena: Arena<Tree, 1000> = Arena::new();

    // Build a deep left-leaning tree
    let mut current = arena.alloc(Tree::Leaf(0)).unwrap();
    for i in 1..100 {
        let leaf = arena.alloc(Tree::Leaf(i)).unwrap();
        current = arena.alloc(Tree::Branch(current, leaf)).unwrap();
    }

    let root = current;

    // Add some garbage
    for i in 0..50 {
        arena.alloc(Tree::Leaf(1000 + i)).unwrap();
    }

    assert_eq!(arena.len(), 199 + 50); // 199 tree nodes + 50 garbage

    let stats = arena.collect_garbage(&[root]);

    assert_eq!(stats.marked, 199);
    assert_eq!(stats.collected, 50);
    assert_eq!(arena.len(), 199);
}

#[test]
fn test_gc_with_shared_references() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Create a shared leaf
    let shared_leaf = arena.alloc(Tree::Leaf(42)).unwrap();

    // Two branches pointing to the same leaf (diamond shape)
    let branch1 = arena.alloc(Tree::Branch(shared_leaf, shared_leaf)).unwrap();
    let branch2 = arena.alloc(Tree::Branch(shared_leaf, shared_leaf)).unwrap();
    let root = arena.alloc(Tree::Branch(branch1, branch2)).unwrap();

    // Garbage
    arena.alloc(Tree::Leaf(999)).unwrap();

    assert_eq!(arena.len(), 5);

    let stats = arena.collect_garbage(&[root]);

    // shared_leaf should only be counted once
    assert_eq!(stats.marked, 4); // root, branch1, branch2, shared_leaf
    assert_eq!(stats.collected, 1);
}

#[test]
fn test_gc_invalid_roots_ignored() {
    let arena: Arena<Tree, 100> = Arena::new();

    let valid_root = arena.alloc(Tree::Leaf(1)).unwrap();
    arena.alloc(Tree::Leaf(999)).unwrap(); // garbage

    // Create some invalid indices
    let invalid1 = ArenaIndex::new(50); // Out of allocated range

    // GC should ignore invalid roots
    let stats = arena.collect_garbage(&[valid_root, invalid1]);

    assert_eq!(stats.marked, 1);
    assert_eq!(stats.collected, 1);
}

#[test]
fn test_gc_empty_arena() {
    let arena: Arena<Tree, 100> = Arena::new();

    let stats = arena.collect_garbage(&[]);

    assert_eq!(stats.total_before, 0);
    assert_eq!(stats.marked, 0);
    assert_eq!(stats.collected, 0);
}

#[test]
fn test_gc_stats() {
    let arena: Arena<Tree, 100> = Arena::new();

    for i in 0..10 {
        arena.alloc(Tree::Leaf(i)).unwrap();
    }

    let root = arena.alloc(Tree::Leaf(100)).unwrap();

    let stats = arena.collect_garbage(&[root]);

    assert_eq!(
        stats,
        GcStats {
            marked: 1,
            collected: 10,
            total_before: 11,
        }
    );
}

#[test]
fn test_gc_collect_garbage_multi() {
    let arena: Arena<Tree, 100> = Arena::new();

    let root1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let root2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let root3 = arena.alloc(Tree::Leaf(3)).unwrap();
    arena.alloc(Tree::Leaf(999)).unwrap(); // garbage

    // Use multiple root sets
    let roots_a = [root1, root2];
    let roots_b = [root3];

    let stats = arena.collect_garbage_multi(&[&roots_a, &roots_b]);

    assert_eq!(stats.marked, 3);
    assert_eq!(stats.collected, 1);
}

#[test]
fn test_gc_repeated_collections() {
    let arena: Arena<Tree, 100> = Arena::new();

    let mut root = arena.alloc(Tree::Leaf(0)).unwrap();

    for round in 0..10 {
        // Add garbage each round
        for i in 0..5 {
            arena.alloc(Tree::Leaf(round * 100 + i)).unwrap();
        }

        // Extend the tree
        let new_leaf = arena.alloc(Tree::Leaf(round)).unwrap();
        root = arena.alloc(Tree::Branch(root, new_leaf)).unwrap();

        // Collect
        let stats = arena.collect_garbage(&[root]);
        assert_eq!(stats.collected, 5); // Only the garbage
    }

    // Tree should have grown: 1 initial + 10 rounds * 2 nodes = 21
    assert_eq!(arena.len(), 21);
}

// Linked list for GC tests
#[derive(Clone, Copy, Debug, PartialEq)]
enum ListNode {
    Free(usize),
    N { value: isize, next: Option<ArenaIndex> },
}

impl Slotted for ListNode {
    fn is_free(&self) -> bool { matches!(self, ListNode::Free(_)) }
    fn next_free(&self) -> usize { match self { ListNode::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { ListNode::Free(next) }
}

impl<const N: usize> Trace<ListNode, N> for ListNode {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        if let ListNode::N { next: Some(n), .. } = self {
            tracer(*n);
        }
    }
}

#[test]
fn test_gc_linked_list() {
    let arena: Arena<ListNode, 100> = Arena::new();

    // Build list: head -> n1 -> n2 -> n3 -> None
    let n3 = arena
        .alloc(ListNode::N {
            value: 3,
            next: None,
        })
        .unwrap();
    let n2 = arena
        .alloc(ListNode::N {
            value: 2,
            next: Some(n3),
        })
        .unwrap();
    let n1 = arena
        .alloc(ListNode::N {
            value: 1,
            next: Some(n2),
        })
        .unwrap();
    let head = arena
        .alloc(ListNode::N {
            value: 0,
            next: Some(n1),
        })
        .unwrap();

    // Garbage nodes
    arena
        .alloc(ListNode::N {
            value: -1,
            next: None,
        })
        .unwrap();
    arena
        .alloc(ListNode::N {
            value: -2,
            next: None,
        })
        .unwrap();

    assert_eq!(arena.len(), 6);

    let stats = arena.collect_garbage(&[head]);

    assert_eq!(stats.marked, 4); // head, n1, n2, n3
    assert_eq!(stats.collected, 2);
    assert_eq!(arena.len(), 4);

    // Verify list is intact
    let h = arena.get(head).unwrap();
    if let ListNode::N { value, next } = h {
        assert_eq!(value, 0);
        let node1 = arena.get(next.unwrap()).unwrap();
        if let ListNode::N { value: v1, next: n1 } = node1 {
            assert_eq!(v1, 1);
            let node2 = arena.get(n1.unwrap()).unwrap();
            if let ListNode::N { value: v2, next: n2 } = node2 {
                assert_eq!(v2, 2);
                let node3 = arena.get(n2.unwrap()).unwrap();
                if let ListNode::N { value: v3, next: n3 } = node3 {
                    assert_eq!(v3, 3);
                    assert!(n3.is_none());
                } else { panic!("Expected ListNode::N"); }
            } else { panic!("Expected ListNode::N"); }
        } else { panic!("Expected ListNode::N"); }
    } else { panic!("Expected ListNode::N"); }
}

#[test]
fn test_gc_stress() {
    let arena: Arena<Tree, 1000> = Arena::new();

    let mut roots: Vec<ArenaIndex> = Vec::new();

    for round in 0..50 {
        // Create a small tree
        let leaf1 = arena.alloc(Tree::Leaf(round * 2)).unwrap();
        let leaf2 = arena.alloc(Tree::Leaf(round * 2 + 1)).unwrap();
        let root = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();
        roots.push(root);

        // Create garbage
        for i in 0..10 {
            arena.alloc(Tree::Leaf(1000 + round * 10 + i)).unwrap();
        }
    }

    // 50 trees * 3 nodes + 50 * 10 garbage = 150 + 500 = 650
    assert_eq!(arena.len(), 650);

    let stats = arena.collect_garbage(&roots);

    assert_eq!(stats.marked, 150);
    assert_eq!(stats.collected, 500);
    assert_eq!(arena.len(), 150);

    // All roots should still be valid
    for root in roots {
        assert!(arena.is_allocated(root));
    }
}

// ============================================================================
// GC Enable/Disable Tests
// ============================================================================

#[test]
#[test]
#[test]
#[test]
#[test]
#[test]
#[test]
#[test]
#[test]
#[test]
#[test]
// ============================================================================
// Additional GC Edge Case Tests
// ============================================================================
#[test]
fn test_gc_self_referential() {
    // A node that points to itself
    #[derive(Clone, Copy)]
    enum SelfRef {
        Free(usize),
        N { self_ptr: Option<ArenaIndex> },
    }

    impl Slotted for SelfRef {
        fn is_free(&self) -> bool { matches!(self, SelfRef::Free(_)) }
        fn next_free(&self) -> usize { match self { SelfRef::Free(n) => *n, _ => unreachable!() } }
        fn make_free(next: usize) -> Self { SelfRef::Free(next) }
    }

    impl<const N: usize> Trace<SelfRef, N> for SelfRef {
        fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
            if let SelfRef::N { self_ptr: Some(ptr) } = self {
                tracer(*ptr);
            }
        }
    }

    let arena: Arena<SelfRef, 100> = Arena::new();

    // Allocate and make it point to itself
    let node = arena.alloc(SelfRef::N { self_ptr: None }).unwrap();
    arena
        .set(
            node,
            SelfRef::N {
                self_ptr: Some(node),
            },
        )
        .unwrap();

    // Add garbage
    arena.alloc(SelfRef::N { self_ptr: None }).unwrap();

    let stats = arena.collect_garbage(&[node]);

    assert_eq!(stats.marked, 1);
    assert_eq!(stats.collected, 1);
}

#[test]
fn test_gc_long_chain() {
    let arena: Arena<ListNode, 1000> = Arena::new();

    // Build a very long linked list
    let mut current = arena
        .alloc(ListNode::N {
            value: 0,
            next: None,
        })
        .unwrap();
    for i in 1..500 {
        let next = arena
            .alloc(ListNode::N {
            value: i,
                next: Some(current),
            })
            .unwrap();
        current = next;
    }

    let head = current;

    // Add garbage
    for _ in 0..100 {
        arena
            .alloc(ListNode::N {
            value: -1,
                next: None,
            })
            .unwrap();
    }

    assert_eq!(arena.len(), 600);

    let stats = arena.collect_garbage(&[head]);

    assert_eq!(stats.marked, 500);
    assert_eq!(stats.collected, 100);
    assert_eq!(arena.len(), 500);
}

#[test]
fn test_gc_dense_graph() {
    // Create a structure where many nodes point to the same targets
    #[derive(Clone, Copy)]
    enum MultiRef {
        Free(usize),
        N { refs: [Option<ArenaIndex>; 4] },
    }

    impl Slotted for MultiRef {
        fn is_free(&self) -> bool { matches!(self, MultiRef::Free(_)) }
        fn next_free(&self) -> usize { match self { MultiRef::Free(n) => *n, _ => unreachable!() } }
        fn make_free(next: usize) -> Self { MultiRef::Free(next) }
    }

    impl<const N: usize> Trace<MultiRef, N> for MultiRef {
        fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
            if let MultiRef::N { refs } = self {
                for r in refs {
                    if let Some(idx) = r {
                        tracer(*idx);
                    }
                }
            }
        }
    }

    let arena: Arena<MultiRef, 100> = Arena::new();

    // Create some shared nodes
    let shared1 = arena.alloc(MultiRef::N { refs: [None; 4] }).unwrap();
    let shared2 = arena.alloc(MultiRef::N { refs: [None; 4] }).unwrap();

    // Create nodes that all point to the shared nodes
    let mut roots = Vec::new();
    for _ in 0..10 {
        let node = arena
            .alloc(MultiRef::N {
                refs: [Some(shared1), Some(shared2), Some(shared1), Some(shared2)],
            })
            .unwrap();
        roots.push(node);
    }

    // Add garbage
    for _ in 0..20 {
        arena.alloc(MultiRef::N { refs: [None; 4] }).unwrap();
    }

    assert_eq!(arena.len(), 32); // 2 shared + 10 roots + 20 garbage

    let stats = arena.collect_garbage(&roots);

    assert_eq!(stats.marked, 12); // 10 roots + 2 shared
    assert_eq!(stats.collected, 20);
}

#[test]
fn test_gc_interleaved_alloc_collect() {
    let arena: Arena<Tree, 100> = Arena::new();

    let mut root = arena.alloc(Tree::Leaf(0)).unwrap();

    for i in 1..20 {
        // Add to tree
        let new_leaf = arena.alloc(Tree::Leaf(i)).unwrap();
        root = arena.alloc(Tree::Branch(root, new_leaf)).unwrap();

        // Add garbage
        arena.alloc(Tree::Leaf(100 + i)).unwrap();
        arena.alloc(Tree::Leaf(200 + i)).unwrap();

        // Collect every 5 iterations
        if i % 5 == 0 {
            let stats = arena.collect_garbage(&[root]);
            assert!(stats.collected > 0);
        }
    }

    // Final collection
    let final_stats = arena.collect_garbage(&[root]);

    // Tree nodes: 1 initial + 19 * 2 (leaf + branch per iteration) = 39
    // But some branches reuse, so it's: 1 + 19 leaves + 19 branches = 39
    assert!(arena.len() <= 39);
    assert!(final_stats.marked > 0);
}

#[test]
fn test_gc_stats_accuracy() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Create exactly known structure
    let l1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let l2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let l3 = arena.alloc(Tree::Leaf(3)).unwrap();
    let b1 = arena.alloc(Tree::Branch(l1, l2)).unwrap();
    let root = arena.alloc(Tree::Branch(b1, l3)).unwrap();

    // Add exactly 5 garbage nodes
    for i in 0..5 {
        arena.alloc(Tree::Leaf(100 + i)).unwrap();
    }

    assert_eq!(arena.len(), 10);

    let stats = arena.collect_garbage(&[root]);

    assert_eq!(stats.total_before, 10);
    assert_eq!(stats.marked, 5); // root, b1, l1, l2, l3
    assert_eq!(stats.collected, 5);
    assert_eq!(arena.len(), 5);
}

// ============================================================================
// GC Stress Tests
// ============================================================================

/// Node with many children to test the 16-child buffer limit in GC
#[derive(Clone, Copy)]
enum ManyChildNode {
    Free(usize),
    N { children: [Option<ArenaIndex>; 8] },
}

impl Slotted for ManyChildNode {
    fn is_free(&self) -> bool { matches!(self, ManyChildNode::Free(_)) }
    fn next_free(&self) -> usize { match self { ManyChildNode::Free(n) => *n, _ => unreachable!() } }
    fn make_free(next: usize) -> Self { ManyChildNode::Free(next) }
}

impl<const N: usize> Trace<ManyChildNode, N> for ManyChildNode {
    fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
        if let ManyChildNode::N { children } = self {
            for child in children {
                if let Some(idx) = child {
                    tracer(*idx);
                }
            }
        }
    }
}

#[test]
fn test_gc_stress_wide_tree() {
    // Test tree where each node has many children
    let arena: Arena<ManyChildNode, 1000> = Arena::new();

    // Create a wide tree with 8 children per node, 3 levels deep
    // Level 0: 1 root
    // Level 1: 8 nodes
    // Level 2: 64 nodes
    // Total: 73 nodes

    let mut level2_nodes = Vec::new();
    for _ in 0..64 {
        let node = arena
            .alloc(ManyChildNode::N {
                children: [None; 8],
            })
            .unwrap();
        level2_nodes.push(node);
    }

    let mut level1_nodes = Vec::new();
    for i in 0..8 {
        let mut children = [None; 8];
        for j in 0..8 {
            children[j] = Some(level2_nodes[i * 8 + j]);
        }
        let node = arena.alloc(ManyChildNode::N { children }).unwrap();
        level1_nodes.push(node);
    }

    let mut root_children = [None; 8];
    for i in 0..8 {
        root_children[i] = Some(level1_nodes[i]);
    }
    let root = arena
        .alloc(ManyChildNode::N {
            children: root_children,
        })
        .unwrap();

    // Add garbage
    for _ in 0..100 {
        arena
            .alloc(ManyChildNode::N {
                children: [None; 8],
            })
            .unwrap();
    }

    assert_eq!(arena.len(), 173); // 73 tree + 100 garbage

    let stats = arena.collect_garbage(&[root]);

    assert_eq!(stats.marked, 73);
    assert_eq!(stats.collected, 100);
    assert_eq!(arena.len(), 73);
}

#[test]
fn test_gc_stress_very_deep_tree() {
    let arena: Arena<Tree, 2000> = Arena::new();

    // Build an extremely deep tree (500 levels to leave room for garbage)
    // 1 initial + 499 iterations * 2 = 999 nodes
    let mut current = arena.alloc(Tree::Leaf(0)).unwrap();
    for i in 1..500 {
        let leaf = arena.alloc(Tree::Leaf(i)).unwrap();
        current = arena.alloc(Tree::Branch(current, leaf)).unwrap();
    }

    let root = current;
    let tree_size = arena.len();
    assert_eq!(tree_size, 999);

    // Add garbage to fill remaining space
    let garbage_to_add = 500usize;
    for i in 0..garbage_to_add {
        arena.alloc(Tree::Leaf(10000 + i as isize)).unwrap();
    }

    assert_eq!(arena.len(), 999 + garbage_to_add);

    let stats = arena.collect_garbage(&[root]);

    assert_eq!(stats.marked, 999);
    assert_eq!(stats.collected, garbage_to_add);
    assert_eq!(arena.len(), 999);
}

#[test]
fn test_gc_stress_full_arena_all_garbage() {
    let arena: Arena<Tree, 500> = Arena::new();

    // Fill the entire arena with garbage (no roots)
    for i in 0..500 {
        arena.alloc(Tree::Leaf(i)).unwrap();
    }

    assert!(arena.is_full());

    // Collect with no roots - everything is garbage
    let stats = arena.collect_garbage(&[]);

    assert_eq!(stats.marked, 0);
    assert_eq!(stats.collected, 500);
    assert!(arena.is_empty());
}

#[test]
fn test_gc_stress_full_arena_all_reachable() {
    let arena: Arena<Tree, 500> = Arena::new();

    // Build a tree that uses all 500 slots
    // Binary tree: n leaves need n-1 internal nodes, so ~250 leaves + 249 internal = 499
    // Let's do a linked structure instead for simplicity

    let mut roots = Vec::new();

    // Create 250 small trees (2 nodes each = 500 total)
    for i in 0..250 {
        let leaf = arena.alloc(Tree::Leaf(i * 2)).unwrap();
        let branch = arena.alloc(Tree::Branch(leaf, leaf)).unwrap();
        roots.push(branch);
    }

    assert!(arena.is_full());

    // All should be reachable
    let stats = arena.collect_garbage(&roots);

    assert_eq!(stats.marked, 500);
    assert_eq!(stats.collected, 0);
    assert!(arena.is_full());
}

#[test]
fn test_gc_stress_rapid_cycles() {
    let arena: Arena<Tree, 100> = Arena::new();

    let mut root = arena.alloc(Tree::Leaf(0)).unwrap();

    // Perform 1000 rapid GC cycles
    for round in 0..1000 {
        // Add a node to the tree
        if arena.available() >= 2 {
            let new_leaf = arena.alloc(Tree::Leaf(round)).unwrap();
            root = arena.alloc(Tree::Branch(root, new_leaf)).unwrap();
        }

        // Add some garbage
        let garbage_count = (round % 5) + 1;
        for i in 0..garbage_count {
            if arena.available() > 0 {
                arena.alloc(Tree::Leaf(10000 + round * 10 + i)).unwrap();
            }
        }

        // Collect
        let stats = arena.collect_garbage(&[root]);

        // Should have collected the garbage we just added
        assert!(stats.collected <= garbage_count as usize);

        // Tree should still be intact
        assert!(arena.is_allocated(root));
    }
}

#[test]
fn test_gc_stress_alternating_patterns() {
    let arena: Arena<Tree, 300> = Arena::new();

    for pattern in 0..10 {
        // Clear previous
        arena.clear();

        let mut roots = Vec::new();

        // Different allocation patterns each round
        match pattern % 4 {
            0 => {
                // Many small trees
                for i in 0..50 {
                    let leaf = arena.alloc(Tree::Leaf(i)).unwrap();
                    roots.push(leaf);
                }
                // Add garbage
                for i in 0..50 {
                    arena.alloc(Tree::Leaf(1000 + i)).unwrap();
                }
            }
            1 => {
                // Few large trees (5 trees * 29 nodes each = 145 nodes)
                for _ in 0..5 {
                    let mut node = arena.alloc(Tree::Leaf(0)).unwrap();
                    for j in 1..15 {
                        let leaf = arena.alloc(Tree::Leaf(j)).unwrap();
                        node = arena.alloc(Tree::Branch(node, leaf)).unwrap();
                    }
                    roots.push(node);
                }
                // Add some garbage (not filling completely)
                for _ in 0..50 {
                    arena.alloc(Tree::Leaf(9999)).unwrap();
                }
            }
            2 => {
                // Single large tree (1 + 50*2 = 101 nodes)
                let mut node = arena.alloc(Tree::Leaf(0)).unwrap();
                for i in 1..51 {
                    let leaf = arena.alloc(Tree::Leaf(i)).unwrap();
                    node = arena.alloc(Tree::Branch(node, leaf)).unwrap();
                }
                roots.push(node);
                // Add some garbage
                for _ in 0..50 {
                    arena.alloc(Tree::Leaf(8888)).unwrap();
                }
            }
            3 => {
                // Interleaved roots and garbage
                for i in 0..100 {
                    let node = arena.alloc(Tree::Leaf(i)).unwrap();
                    if i % 2 == 0 {
                        roots.push(node);
                    }
                    // Node is garbage if odd
                }
            }
            _ => unreachable!(),
        }

        let before = arena.len();
        let stats = arena.collect_garbage(&roots);

        // Verify roots survived
        for root in &roots {
            assert!(arena.is_allocated(*root));
        }

        // Verify something was collected (except maybe pattern 0 with 50/50 split)
        if pattern % 4 != 0 {
            assert!(stats.collected > 0 || stats.marked == before);
        }
    }
}

#[test]
fn test_gc_stress_fragmented_heap() {
    let arena: Arena<Tree, 100> = Arena::new();

    // Create highly fragmented heap
    let mut indices = Vec::new();
    for i in 0..100 {
        indices.push(arena.alloc(Tree::Leaf(i)).unwrap());
    }

    // Free every other slot (creates fragmentation)
    for i in (0..100).step_by(2) {
        arena.free(indices[i]).unwrap();
    }

    // Remaining 50 slots are our "roots"
    let roots: Vec<ArenaIndex> = indices
        .iter()
        .enumerate()
        .filter(|(i, _)| i % 2 == 1)
        .map(|(_, &idx)| idx)
        .collect();

    // Allocate in the gaps (these become garbage)
    for i in 0..50 {
        arena.alloc(Tree::Leaf(1000 + i)).unwrap();
    }

    assert!(arena.is_full());

    // Collect - should free the newly allocated garbage
    let stats = arena.collect_garbage(&roots);

    assert_eq!(stats.marked, 50);
    assert_eq!(stats.collected, 50);
    assert_eq!(arena.len(), 50);
}

#[test]
fn test_gc_stress_many_roots() {
    let arena: Arena<Tree, 1000> = Arena::new();

    // Create 500 individual roots (each a single leaf)
    let mut roots = Vec::new();
    for i in 0..500 {
        let root = arena.alloc(Tree::Leaf(i)).unwrap();
        roots.push(root);
    }

    // Add 500 garbage nodes
    for i in 0..500 {
        arena.alloc(Tree::Leaf(10000 + i)).unwrap();
    }

    assert!(arena.is_full());

    let stats = arena.collect_garbage(&roots);

    assert_eq!(stats.marked, 500);
    assert_eq!(stats.collected, 500);
    assert_eq!(arena.len(), 500);
}

#[test]
fn test_gc_stress_diamond_dag() {
    // Create a DAG where many nodes share the same children
    let arena: Arena<Tree, 500> = Arena::new();

    // Create shared leaves
    let mut shared_leaves = Vec::new();
    for i in 0..10 {
        shared_leaves.push(arena.alloc(Tree::Leaf(i)).unwrap());
    }

    // Create many branches that all point to the same shared leaves
    let mut roots = Vec::new();
    for i in 0..100 {
        let left_idx = i % 10;
        let right_idx = (i + 1) % 10;
        let branch = arena
            .alloc(Tree::Branch(
                shared_leaves[left_idx],
                shared_leaves[right_idx],
            ))
            .unwrap();
        roots.push(branch);
    }

    // Add garbage
    for i in 0..200 {
        arena.alloc(Tree::Leaf(1000 + i)).unwrap();
    }

    let stats = arena.collect_garbage(&roots);

    // Should mark: 10 shared leaves + 100 branches = 110
    assert_eq!(stats.marked, 110);
    assert_eq!(stats.collected, 200);
}

#[test]
fn test_gc_stress_complex_cycles() {
    // Create a structure with multiple interconnected cycles
    #[derive(Clone, Copy)]
    enum CycleNode {
        Free(usize),
        N { refs: [Option<ArenaIndex>; 4] },
    }

    impl Slotted for CycleNode {
        fn is_free(&self) -> bool { matches!(self, CycleNode::Free(_)) }
        fn next_free(&self) -> usize { match self { CycleNode::Free(n) => *n, _ => unreachable!() } }
        fn make_free(next: usize) -> Self { CycleNode::Free(next) }
    }

    impl<const N: usize> Trace<CycleNode, N> for CycleNode {
        fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
            if let CycleNode::N { refs } = self {
                for r in refs {
                    if let Some(idx) = r {
                        tracer(*idx);
                    }
                }
            }
        }
    }

    let arena: Arena<CycleNode, 100> = Arena::new();

    // Create a ring of nodes
    let mut ring_nodes = Vec::new();
    for _ in 0..20 {
        ring_nodes.push(arena.alloc(CycleNode::N { refs: [None; 4] }).unwrap());
    }

    // Connect them in a ring with cross-connections
    for i in 0..20 {
        let next = (i + 1) % 20;
        let cross1 = (i + 5) % 20;
        let cross2 = (i + 10) % 20;

        arena
            .set(
                ring_nodes[i],
                CycleNode::N {
                    refs: [
                        Some(ring_nodes[next]),
                        Some(ring_nodes[cross1]),
                        Some(ring_nodes[cross2]),
                        None,
                    ],
                },
            )
            .unwrap();
    }

    // Add garbage nodes
    for _ in 0..50 {
        arena.alloc(CycleNode::N { refs: [None; 4] }).unwrap();
    }

    // Use first ring node as root
    let stats = arena.collect_garbage(&[ring_nodes[0]]);

    assert_eq!(stats.marked, 20); // All ring nodes reachable
    assert_eq!(stats.collected, 50);
}

#[test]
fn test_gc_stress_maximum_children_per_trace() {
    // Test the 16-child buffer limit by having nodes with exactly 16+ children
    // traced in rapid succession

    #[derive(Clone, Copy)]
    enum Node16 {
        Free(usize),
        N { children: [Option<ArenaIndex>; 16] },
    }

    impl Slotted for Node16 {
        fn is_free(&self) -> bool { matches!(self, Node16::Free(_)) }
        fn next_free(&self) -> usize { match self { Node16::Free(n) => *n, _ => unreachable!() } }
        fn make_free(next: usize) -> Self { Node16::Free(next) }
    }

    impl<const N: usize> Trace<Node16, N> for Node16 {
        fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
            if let Node16::N { children } = self {
                for child in children {
                    if let Some(idx) = child {
                        tracer(*idx);
                    }
                }
            }
        }
    }

    let arena: Arena<Node16, 500> = Arena::new();

    // Create leaves
    let mut leaves = Vec::new();
    for _ in 0..32 {
        leaves.push(
            arena
                .alloc(Node16::N {
                    children: [None; 16],
                })
                .unwrap(),
        );
    }

    // Create a node with exactly 16 children
    let mut children1 = [None; 16];
    for i in 0..16 {
        children1[i] = Some(leaves[i]);
    }
    let node1 = arena
        .alloc(Node16::N {
            children: children1,
        })
        .unwrap();

    // Create another node with 16 different children
    let mut children2 = [None; 16];
    for i in 0..16 {
        children2[i] = Some(leaves[16 + i]);
    }
    let node2 = arena
        .alloc(Node16::N {
            children: children2,
        })
        .unwrap();

    // Root points to both
    let root = arena
        .alloc(Node16::N {
            children: [
                Some(node1),
                Some(node2),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        })
        .unwrap();

    // Add garbage
    for _ in 0..100 {
        arena
            .alloc(Node16::N {
                children: [None; 16],
            })
            .unwrap();
    }

    let stats = arena.collect_garbage(&[root]);

    // root + 2 intermediate + 32 leaves = 35
    assert_eq!(stats.marked, 35);
    assert_eq!(stats.collected, 100);
}

#[test]
fn test_gc_stress_repeated_collect_same_roots() {
    let arena: Arena<Tree, 100> = Arena::new();

    let leaf1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let leaf2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(leaf1, leaf2)).unwrap();

    // Repeatedly collect with same roots - should be idempotent
    for _ in 0..100 {
        // Add some garbage
        if arena.available() >= 5 {
            for i in 0..5 {
                arena.alloc(Tree::Leaf(1000 + i)).unwrap();
            }
        }

        let stats = arena.collect_garbage(&[root]);

        // Should always have 3 marked (our tree)
        assert_eq!(stats.marked, 3);

        // Tree should remain intact
        assert_eq!(arena.get(root).unwrap(), Tree::Branch(leaf1, leaf2));
        assert_eq!(arena.get(leaf1).unwrap(), Tree::Leaf(1));
        assert_eq!(arena.get(leaf2).unwrap(), Tree::Leaf(2));
    }
}

#[test]
fn test_gc_stress_collect_after_mutations() {
    let arena: Arena<Tree, 100> = Arena::new();

    let mut leaf = arena.alloc(Tree::Leaf(0)).unwrap();

    for round in 0..50 {
        // Mutate the leaf value
        arena.set(leaf, Tree::Leaf(round)).unwrap();

        // Add garbage
        let garbage = arena.alloc(Tree::Leaf(1000 + round)).unwrap();

        // Collect
        let stats = arena.collect_garbage(&[leaf]);

        assert_eq!(stats.marked, 1);
        assert_eq!(stats.collected, 1);

        // Garbage should be gone
        assert!(!arena.is_allocated(garbage));

        // Leaf should have new value
        assert_eq!(arena.get(leaf).unwrap(), Tree::Leaf(round));
    }
}

#[test]

fn test_gc_stress_worst_case_mark_stack() {
    // Create a structure that maximizes mark stack usage
    // A long chain where each node must be pushed to the stack
    let arena: Arena<Tree, 500> = Arena::new();

    // Build a right-leaning tree (worst case for stack depth)
    let mut current = arena.alloc(Tree::Leaf(0)).unwrap();
    for i in 1..250 {
        let new_branch = arena.alloc(Tree::Branch(current, current)).unwrap();
        current = new_branch;
    }

    let root = current;

    // Add garbage
    while arena.available() > 0 {
        arena.alloc(Tree::Leaf(9999)).unwrap();
    }

    let stats = arena.collect_garbage(&[root]);

    // All tree nodes should be marked
    assert!(stats.marked > 0);
    // Garbage should be collected
    assert!(stats.collected > 0);
}

#[test]
fn test_gc_stress_incremental_tree_building() {
    let arena: Arena<Tree, 500> = Arena::new();

    let mut root = arena.alloc(Tree::Leaf(0)).unwrap();

    // Build tree incrementally with GC after each step
    for i in 1..100isize {
        // Expand tree
        let new_leaf = arena.alloc(Tree::Leaf(i)).unwrap();
        root = arena.alloc(Tree::Branch(root, new_leaf)).unwrap();

        // Add garbage proportional to tree size
        let garbage_count = i % 10;
        for j in 0..garbage_count {
            if arena.available() > 0 {
                arena.alloc(Tree::Leaf(1000 * i + j)).unwrap();
            }
        }

        // Collect
        let stats = arena.collect_garbage(&[root]);

        // Tree should be intact
        let expected_tree_size = (1 + i * 2) as usize; // initial leaf + i*(leaf + branch)
        assert_eq!(stats.marked, expected_tree_size);
    }
}

// ============================================================================
// New API Tests
// ============================================================================

#[test]
fn test_arena_index_null() {
    let null_idx = ArenaIndex::NIL;
    assert!(null_idx.is_nil());

    let default_idx = ArenaIndex::default();
    assert!(default_idx.is_nil());
    assert_eq!(null_idx, default_idx);

    let normal_idx = ArenaIndex::new(5);
    assert!(!normal_idx.is_nil());
}

#[test]
fn test_modify() {
    let arena: Arena<Val, 10> = Arena::new();
    let idx = arena.alloc(Val::V(42)).unwrap();

    arena.modify(idx, |v| if let Val::V(n) = v { *n += 10 }).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(52));

    arena.modify(idx, |v| if let Val::V(n) = v { *n *= 2 }).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(104));
}

#[test]
fn test_modify_invalid() {
    let arena: Arena<Val, 10> = Arena::new();
    let idx = arena.alloc(Val::V(42)).unwrap();
    arena.free(idx).unwrap();

    assert_eq!(
        arena.modify(idx, |_| {}),
        Err(ArenaError::IndexNotAllocated)
    );
}

#[test]
fn test_try_get() {
    let arena: Arena<Val, 10> = Arena::new();
    let idx = arena.alloc(Val::V(42)).unwrap();

    assert_eq!(arena.try_get(idx), Some(Val::V(42)));

    arena.free(idx).unwrap();
    assert_eq!(arena.try_get(idx), None);

    // Invalid index
    let bad_idx = ArenaIndex::new(100);
    assert_eq!(arena.try_get(bad_idx), None);
}

#[test]
fn test_swap() {
    let arena: Arena<Val, 10> = Arena::new();
    let idx1 = arena.alloc(Val::V(100)).unwrap();
    let idx2 = arena.alloc(Val::V(200)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Val::V(100));
    assert_eq!(arena.get(idx2).unwrap(), Val::V(200));

    arena.swap(idx1, idx2).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Val::V(200));
    assert_eq!(arena.get(idx2).unwrap(), Val::V(100));
}

#[test]
fn test_swap_same_index() {
    let arena: Arena<Val, 10> = Arena::new();
    let idx = arena.alloc(Val::V(42)).unwrap();

    // Swapping with self should be a no-op
    arena.swap(idx, idx).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(42));
}

#[test]
fn test_replace() {
    let arena: Arena<Val, 10> = Arena::new();
    let idx = arena.alloc(Val::V(42)).unwrap();

    let old = arena.replace(idx, Val::V(100)).unwrap();
    assert_eq!(old, Val::V(42));
    assert_eq!(arena.get(idx).unwrap(), Val::V(100));
}

#[test]
fn test_validate() {
    let arena: Arena<Val, 10> = Arena::new();
    assert!(arena.validate());

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    assert!(arena.validate());

    let idx = arena.alloc(Val::V(3)).unwrap();
    arena.free(idx).unwrap();
    assert!(arena.validate());

    arena.clear();
    assert!(arena.validate());
}

#[test]
fn test_is_slot_occupied() {
    let arena: Arena<Val, 10> = Arena::new();

    assert!(!arena.is_slot_occupied(0));

    let idx = arena.alloc(Val::V(42)).unwrap();
    assert!(arena.is_slot_occupied(idx.raw()));

    arena.free(idx).unwrap();
    assert!(!arena.is_slot_occupied(idx.raw()));
}

#[test]
fn test_allocated_indices() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();

    let indices = arena.allocated_indices();

    // First 3 should be valid
    assert_eq!(indices[0], idx1);
    assert_eq!(indices[1], idx2);
    assert_eq!(indices[2], idx3);

    // Rest should be NULL
    for i in 3..10 {
        assert!(indices[i].is_nil());
    }
}

#[test]
fn test_for_each() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();

    let mut sum = 0;
    arena.for_each(|_, v| if let Val::V(n) = v { sum += n });
    assert_eq!(sum, 6);
}

#[test]
fn test_for_each_mut() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();

    arena.for_each_mut(|_, v| if let Val::V(n) = v { *n *= 10 });

    let mut sum = 0;
    arena.for_each(|_, v| if let Val::V(n) = v { sum += n });
    assert_eq!(sum, 60);
}

#[test]
fn test_count_where() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();
    arena.alloc(Val::V(4)).unwrap();
    arena.alloc(Val::V(5)).unwrap();

    assert_eq!(arena.count_where(|v| matches!(v, Val::V(n) if n % 2 == 0)), 2); // 2, 4
    assert_eq!(arena.count_where(|v| matches!(v, Val::V(n) if *n > 3)), 2); // 4, 5
    assert_eq!(arena.count_where(|v| matches!(v, Val::V(n) if *n > 10)), 0);
}

#[test]
fn test_find() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(10)).unwrap();
    arena.alloc(Val::V(20)).unwrap();
    arena.alloc(Val::V(30)).unwrap();

    let found = arena.find(|v| matches!(v, Val::V(10)));
    assert!(found.is_some());
    let (idx, val) = found.unwrap();
    assert_eq!(idx, idx1);
    assert_eq!(val, Val::V(10));

    assert!(arena.find(|v| matches!(v, Val::V(999))).is_none());
}

#[test]
fn test_any_all() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(4)).unwrap();
    arena.alloc(Val::V(6)).unwrap();

    assert!(arena.any(|v| matches!(v, Val::V(4))));
    assert!(!arena.any(|v| matches!(v, Val::V(5))));

    assert!(arena.all(|v| matches!(v, Val::V(n) if n % 2 == 0))); // All even
    assert!(!arena.all(|v| matches!(v, Val::V(n) if *n > 3))); // Not all > 3
}

#[test]
fn test_all_empty() {
    let arena: Arena<Val, 10> = Arena::new();
    // all() on empty returns true (vacuously true)
    assert!(arena.all(|_| false));
}

#[test]
fn test_arena_error_methods() {
    assert!(ArenaError::OutOfMemory.is_out_of_memory());
    assert!(!ArenaError::OutOfMemory.is_invalid_index());

    assert!(!ArenaError::IndexOutOfBounds.is_out_of_memory());
    assert!(ArenaError::IndexOutOfBounds.is_invalid_index());

    assert_eq!(ArenaError::OutOfMemory.as_str(), "Arena is full");
}

#[test]
fn test_gc_stats_methods() {
    let stats = GcStats {
        marked: 3,
        collected: 7,
        total_before: 10,
    };

    assert!(stats.did_collect());
    assert_eq!(stats.remaining(), 3);
    assert!((stats.collection_ratio() - 0.7).abs() < 0.001);
    assert!((stats.survival_ratio() - 0.3).abs() < 0.001);

    let empty_stats = GcStats::default();
    assert!(!empty_stats.did_collect());
    assert_eq!(empty_stats.remaining(), 0);
}

#[test]
fn test_arena_stats_methods() {
    let stats = ArenaStats {
        capacity: 100,
        allocated: 30,
        free: 70,
        fragmentation: 0.2,
    };

    assert!((stats.usage_percent() - 30.0).abs() < 0.001);
    assert!((stats.free_percent() - 70.0).abs() < 0.001);
    assert!(!stats.is_empty());
    assert!(!stats.is_full());
    assert!(!stats.is_fragmented(0.3));
    assert!(stats.is_fragmented(0.1));
}

#[test]
fn test_alloc_then_gc_then_alloc() {
    let arena: Arena<Tree, 5> = Arena::new();

    let root = arena.alloc(Tree::Leaf(1)).unwrap();
    arena.alloc(Tree::Leaf(999)).unwrap(); // garbage
    arena.alloc(Tree::Leaf(888)).unwrap(); // garbage
    arena.alloc(Tree::Leaf(777)).unwrap(); // garbage
    arena.alloc(Tree::Leaf(666)).unwrap(); // garbage

    assert!(arena.is_full());

    // Alloc fails when full
    assert!(arena.alloc(Tree::Leaf(2)).is_err());

    // Explicitly run GC first, then alloc succeeds
    arena.collect_garbage(&[root]);
    let new_idx = arena.alloc(Tree::Leaf(2)).unwrap();

    // Should have collected garbage and allocated
    assert_eq!(arena.len(), 2);
    assert!(arena.is_allocated(root));
    assert!(arena.is_allocated(new_idx));
}

#[test]
fn test_gc_cannot_help_when_all_reachable() {
    let arena: Arena<Tree, 3> = Arena::new();

    // Fill with non-garbage
    let r1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let r2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let r3 = arena.alloc(Tree::Leaf(3)).unwrap();

    // All are roots, so GC won't help
    let stats = arena.collect_garbage(&[r1, r2, r3]);
    assert_eq!(stats.collected, 0);

    // Still can't alloc
    let result = arena.alloc(Tree::Leaf(4));
    assert_eq!(result, Err(ArenaError::OutOfMemory));
}

#[test]
fn test_gc_handles_many_children() {
    // Test that GC correctly handles nodes with >16 children
    #[derive(Clone, Copy)]
    enum BigNode {
        Free(usize),
        N { children: [Option<ArenaIndex>; 20] }, // More than 16!
    }

    impl Slotted for BigNode {
        fn is_free(&self) -> bool { matches!(self, BigNode::Free(_)) }
        fn next_free(&self) -> usize { match self { BigNode::Free(n) => *n, _ => unreachable!() } }
        fn make_free(next: usize) -> Self { BigNode::Free(next) }
    }

    impl<const N: usize> Trace<BigNode, N> for BigNode {
        fn trace<F: FnMut(ArenaIndex)>(&self, mut tracer: F) {
            if let BigNode::N { children } = self {
                for child in children {
                    if let Some(idx) = child {
                        tracer(*idx);
                    }
                }
            }
        }
    }

    let arena: Arena<BigNode, 100> = Arena::new();

    // Create 20 leaf nodes
    let mut leaves = [ArenaIndex::NIL; 20];
    for i in 0..20 {
        leaves[i] = arena
            .alloc(BigNode::N {
                children: [None; 20],
            })
            .unwrap();
    }

    // Create a root with all 20 as children
    let mut root_children = [None; 20];
    for i in 0..20 {
        root_children[i] = Some(leaves[i]);
    }
    let root = arena
        .alloc(BigNode::N {
            children: root_children,
        })
        .unwrap();

    // Add garbage
    for _ in 0..30 {
        arena
            .alloc(BigNode::N {
                children: [None; 20],
            })
            .unwrap();
    }

    assert_eq!(arena.len(), 51); // 21 tree nodes + 30 garbage

    let stats = arena.collect_garbage(&[root]);

    // All 21 tree nodes should be marked (root + 20 children)
    assert_eq!(stats.marked, 21);
    assert_eq!(stats.collected, 30);
    assert_eq!(arena.len(), 21);
}

// ============================================================================
// Additional Edge Case Tests for GC Boundary Conditions
// ============================================================================

/// Test that collecting garbage with NULL indices in roots is handled safely
#[test]
fn test_gc_with_null_roots() {
    let arena: Arena<Tree, 50> = Arena::new();

    // Allocate some values
    let idx1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let idx2 = arena.alloc(Tree::Leaf(2)).unwrap();

    // Use NULL indices in roots - they should be safely ignored
    let stats = arena.collect_garbage(&[ArenaIndex::NIL, idx1, ArenaIndex::NIL]);

    // idx1 was preserved, idx2 was garbage
    assert_eq!(stats.marked, 1);
    assert_eq!(stats.collected, 1);
    assert_eq!(arena.len(), 1);
}

/// Test GC with out-of-bounds roots
#[test]
fn test_gc_with_out_of_bounds_roots() {
    let arena: Arena<Tree, 10> = Arena::new();

    let idx = arena.alloc(Tree::Leaf(42)).unwrap();

    // Create an invalid index that's out of bounds
    let invalid_idx = ArenaIndex::new(1000);

    // GC should safely ignore invalid indices
    let stats = arena.collect_garbage(&[invalid_idx, idx]);

    assert_eq!(stats.marked, 1);
    assert_eq!(stats.collected, 0);
    assert_eq!(arena.len(), 1);
}

/// Test GC with stale generation roots
#[test]
fn test_gc_with_stale_roots() {
    let arena: Arena<Tree, 10> = Arena::new();

    let idx1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let idx2 = arena.alloc(Tree::Leaf(2)).unwrap();

    // Free and reallocate idx1 to make the old index stale
    arena.free(idx1).unwrap();
    let idx3 = arena.alloc(Tree::Leaf(3)).unwrap();

    // Use the stale idx1 as root - it should be ignored
    // idx2 should be collected since it's not in roots
    let stats = arena.collect_garbage(&[idx1, idx3]); // idx1 is stale

    // Only idx3 should be marked (idx1 is stale and ignored)
    assert_eq!(stats.marked, 1);
    assert_eq!(stats.collected, 1); // idx2 is garbage
    assert_eq!(arena.len(), 1);
}

/// Test error kind methods
#[test]
fn test_arena_error_kind_methods() {
    let out_of_memory = ArenaError::OutOfMemory;
    assert!(out_of_memory.is_out_of_memory());
    assert!(!out_of_memory.is_invalid_index());
    assert!(!out_of_memory.is_trace_error());

    let index_not_allocated = ArenaError::IndexNotAllocated;
    assert!(!index_not_allocated.is_out_of_memory());
    assert!(index_not_allocated.is_invalid_index());
    assert!(!index_not_allocated.is_trace_error());

    let trace_error = ArenaError::TraceError;
    assert!(!trace_error.is_out_of_memory());
    assert!(!trace_error.is_invalid_index());
    assert!(trace_error.is_trace_error());

    // Test as_str
    assert_eq!(out_of_memory.as_str(), "Arena is full");
    assert_eq!(index_not_allocated.as_str(), "Index not allocated");
    assert_eq!(trace_error.as_str(), "Error during GC tracing");
}

/// Test GC with completely empty roots array
#[test]
fn test_gc_with_empty_roots() {
    let arena: Arena<Tree, 10> = Arena::new();

    arena.alloc(Tree::Leaf(1)).unwrap();
    arena.alloc(Tree::Leaf(2)).unwrap();
    arena.alloc(Tree::Leaf(3)).unwrap();

    // All should be garbage since no roots
    let stats = arena.collect_garbage(&[]);

    assert_eq!(stats.marked, 0);
    assert_eq!(stats.collected, 3);
    assert_eq!(arena.len(), 0);
}

/// Test GC multi with empty root sets
#[test]
fn test_gc_multi_with_empty_root_sets() {
    let arena: Arena<Tree, 10> = Arena::new();

    let idx = arena.alloc(Tree::Leaf(1)).unwrap();
    arena.alloc(Tree::Leaf(2)).unwrap();

    // Some empty, some with values
    let empty: &[ArenaIndex] = &[];
    let has_root: &[ArenaIndex] = &[idx];

    let stats = arena.collect_garbage_multi(&[empty, has_root, empty]);

    assert_eq!(stats.marked, 1);
    assert_eq!(stats.collected, 1);
}

/// Test GC stats methods
#[test]
fn test_gc_stats_methods_comprehensive() {
    // Test with zero objects
    let empty_stats = GcStats {
        marked: 0,
        collected: 0,
        total_before: 0,
    };
    assert!(!empty_stats.did_collect());
    assert_eq!(empty_stats.remaining(), 0);
    assert_eq!(empty_stats.collection_ratio(), 0.0);
    assert_eq!(empty_stats.survival_ratio(), 1.0);

    // Test with some collection
    let some_stats = GcStats {
        marked: 7,
        collected: 3,
        total_before: 10,
    };
    assert!(some_stats.did_collect());
    assert_eq!(some_stats.remaining(), 7);
    assert!((some_stats.collection_ratio() - 0.3).abs() < 0.001);
    assert!((some_stats.survival_ratio() - 0.7).abs() < 0.001);

    // Test with 100% collection
    let full_collect = GcStats {
        marked: 0,
        collected: 5,
        total_before: 5,
    };
    assert!(full_collect.did_collect());
    assert_eq!(full_collect.remaining(), 0);
    assert_eq!(full_collect.collection_ratio(), 1.0);
    assert_eq!(full_collect.survival_ratio(), 0.0);
}

/// Test ArenaStats methods comprehensively
#[test]
fn test_arena_stats_methods_comprehensive() {
    // Empty arena
    let empty_stats = ArenaStats {
        capacity: 100,
        allocated: 0,
        free: 100,
        fragmentation: 0.0,
    };
    assert!(empty_stats.is_empty());
    assert!(!empty_stats.is_full());
    assert_eq!(empty_stats.usage_percent(), 0.0);
    assert_eq!(empty_stats.free_percent(), 100.0);
    assert!(!empty_stats.is_fragmented(0.1));

    // Full arena
    let full_stats = ArenaStats {
        capacity: 100,
        allocated: 100,
        free: 0,
        fragmentation: 0.0,
    };
    assert!(!full_stats.is_empty());
    assert!(full_stats.is_full());
    assert_eq!(full_stats.usage_percent(), 100.0);
    assert_eq!(full_stats.free_percent(), 0.0);

    // Fragmented arena
    let fragmented = ArenaStats {
        capacity: 100,
        allocated: 50,
        free: 50,
        fragmentation: 0.5,
    };
    assert!(fragmented.is_fragmented(0.4));
    assert!(!fragmented.is_fragmented(0.6));

    // Zero capacity arena (edge case)
    let zero_cap = ArenaStats {
        capacity: 0,
        allocated: 0,
        free: 0,
        fragmentation: 0.0,
    };
    assert_eq!(zero_cap.usage_percent(), 0.0);
}

/// Test that GC handles arena at maximum capacity correctly
#[test]
fn test_gc_at_max_capacity_boundary() {
    let arena: Arena<Tree, 5> = Arena::new();

    // Fill arena completely
    let idx0 = arena.alloc(Tree::Leaf(0)).unwrap();
    let idx1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let idx2 = arena.alloc(Tree::Leaf(2)).unwrap();
    let idx3 = arena.alloc(Tree::Leaf(3)).unwrap();
    let idx4 = arena.alloc(Tree::Leaf(4)).unwrap();

    assert!(arena.is_full());
    assert!(arena.alloc(Tree::Leaf(5)).is_err());

    // Collect keeping only some roots
    let stats = arena.collect_garbage(&[idx0, idx2, idx4]);

    assert_eq!(stats.marked, 3);
    assert_eq!(stats.collected, 2);
    assert_eq!(arena.len(), 3);
    assert!(!arena.is_full());

    // Now we can allocate again
    let _new_idx = arena.alloc(Tree::Leaf(100)).unwrap();
    assert_eq!(arena.len(), 4);
}

/// Test GC boundary: GC doesn't help when all reachable
#[test]
fn test_gc_ineffective_when_all_reachable() {
    let arena: Arena<Tree, 3> = Arena::new();

    // Fill arena with all reachable values
    let idx0 = arena.alloc(Tree::Leaf(0)).unwrap();
    let idx1 = arena.alloc(Tree::Leaf(1)).unwrap();
    let idx2 = arena.alloc(Tree::Leaf(2)).unwrap();

    // All roots, so nothing can be collected
    let stats = arena.collect_garbage(&[idx0, idx1, idx2]);
    assert_eq!(stats.collected, 0);

    // Should still fail since GC couldn't free anything
    let result = arena.alloc(Tree::Leaf(3));
    assert_eq!(result, Err(ArenaError::OutOfMemory));
    assert_eq!(arena.len(), 3);
}
