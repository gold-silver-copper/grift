use grift_arena::*;

#[test]
fn test_basic_allocation() {
    let arena: Arena<isize, 10> = Arena::new(0);

    let idx1 = arena.alloc(42).unwrap();
    let idx2 = arena.alloc(43).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), 42);
    assert_eq!(arena.get(idx2).unwrap(), 43);
    assert_eq!(arena.len(), 2);
}

#[test]
fn test_free_and_reuse() {
    let arena: Arena<isize, 10> = Arena::new(0);

    let idx1 = arena.alloc(42).unwrap();
    assert_eq!(arena.len(), 1);

    arena.free(idx1).unwrap();
    assert_eq!(arena.len(), 0);

    let idx2 = arena.alloc(43).unwrap();
    assert_eq!(arena.len(), 1);
    assert_eq!(arena.get(idx2).unwrap(), 43);
}

#[test]
fn test_out_of_memory() {
    let arena: Arena<isize, 3> = Arena::new(0);

    assert!(arena.alloc(1).is_ok());
    assert!(arena.alloc(2).is_ok());
    assert!(arena.alloc(3).is_ok());
    assert_eq!(arena.alloc(4), Err(ArenaError::OutOfMemory));
}

#[test]
fn test_invalid_index() {
    let arena: Arena<isize, 10> = Arena::new(0);

    let idx = arena.alloc(42).unwrap();
    arena.free(idx).unwrap();

    // After freeing, the index is invalid
    assert_eq!(arena.get(idx), Err(ArenaError::InvalidIndex));
}



#[test]
fn test_free_list_o1_allocation() {
    let arena: Arena<isize, 5> = Arena::new(0);

    // Allocate all slots
    let idx0 = arena.alloc(0).unwrap();
    let idx1 = arena.alloc(1).unwrap();
    let idx2 = arena.alloc(2).unwrap();
    let idx3 = arena.alloc(3).unwrap();
    let idx4 = arena.alloc(4).unwrap();

    assert!(arena.is_full());

    // Free some slots in non-sequential order
    arena.free(idx2).unwrap();
    arena.free(idx0).unwrap();
    arena.free(idx4).unwrap();

    assert_eq!(arena.len(), 2);
    assert_eq!(arena.available(), 3);

    // Allocate again - should reuse freed slots (LIFO order from free list)
    let new1 = arena.alloc(100).unwrap();
    let new2 = arena.alloc(200).unwrap();
    let new3 = arena.alloc(300).unwrap();

    assert_eq!(arena.len(), 5);

    // Verify the new values are accessible
    assert_eq!(arena.get(new1).unwrap(), 100);
    assert_eq!(arena.get(new2).unwrap(), 200);
    assert_eq!(arena.get(new3).unwrap(), 300);

    // Original unfreed indices should still work
    assert_eq!(arena.get(idx1).unwrap(), 1);
    assert_eq!(arena.get(idx3).unwrap(), 3);
}

#[test]
fn test_clear_invalidates_all_indices() {
    let arena: Arena<isize, 10> = Arena::new(0);

    let idx1 = arena.alloc(1).unwrap();
    let idx2 = arena.alloc(2).unwrap();
    let idx3 = arena.alloc(3).unwrap();

    arena.clear();

    // All old indices should be invalid
    assert_eq!(arena.get(idx1), Err(ArenaError::InvalidIndex));
    assert_eq!(arena.get(idx2), Err(ArenaError::InvalidIndex));
    assert_eq!(arena.get(idx3), Err(ArenaError::InvalidIndex));

    // New allocations should work
    let new_idx = arena.alloc(42).unwrap();
    assert_eq!(arena.get(new_idx).unwrap(), 42);
}

#[test]
fn test_stats() {
    let arena: Arena<isize, 10> = Arena::new(0);

    arena.alloc(1).unwrap();
    arena.alloc(2).unwrap();

    let stats = arena.stats();
    assert_eq!(stats.capacity, 10);
    assert_eq!(stats.allocated, 2);
    assert_eq!(stats.free, 8);
    assert_eq!(stats.usage_percent(), 20.0);
}

#[test]
fn test_clear() {
    let arena: Arena<isize, 10> = Arena::new(0);

    arena.alloc(1).unwrap();
    arena.alloc(2).unwrap();
    arena.alloc(3).unwrap();

    assert_eq!(arena.len(), 3);

    arena.clear();

    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());
}

// Example of recursive deletion
#[derive(Clone, Copy, Debug, PartialEq)]
enum Tree {
    Leaf(isize),
    Branch(ArenaIndex, ArenaIndex),
}

impl ArenaDelete<Tree, 100> for Tree {
    fn delete_recursive(&self, arena: &Arena<Tree, 100>) -> ArenaResult<()> {
        match *self {
            Tree::Leaf(_) => Ok(()),
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
fn test_recursive_delete() {
    let arena: Arena<Tree, 100> = Arena::new(Tree::Leaf(0));

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    assert_eq!(arena.len(), 3);

    arena.delete_recursive(root).unwrap();

    assert_eq!(arena.len(), 0);
}

#[test]
fn test_deep_copy() {
    let arena: Arena<Tree, 100> = Arena::new(Tree::Leaf(0));

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    let copied_root = arena.copy_deep(root).unwrap();

    // Should have 6 nodes total (3 original + 3 copied)
    assert_eq!(arena.len(), 6);

    // Verify structure is copied
    if let Tree::Branch(cl, cr) = arena.get(copied_root).unwrap() {
        assert_eq!(arena.get(cl).unwrap(), Tree::Leaf(1));
        assert_eq!(arena.get(cr).unwrap(), Tree::Leaf(2));
    } else {
        panic!("Expected branch");
    }
}

#[test]
fn test_set() {
    let arena: Arena<isize, 10> = Arena::new(0);

    let idx = arena.alloc(42).unwrap();
    assert_eq!(arena.get(idx).unwrap(), 42);

    arena.set(idx, 100).unwrap();
    assert_eq!(arena.get(idx).unwrap(), 100);
}

#[test]
fn test_trace_error() {
    // Test the new TraceError variant
    let err = ArenaError::TraceError;
    assert_eq!(err.as_str(), "error during GC tracing");
    assert!(err.is_trace_error());
    assert!(!err.is_out_of_memory());
    assert!(!err.is_invalid_index());
}
