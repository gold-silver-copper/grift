use grift_arena::*;

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

#[test]
fn test_basic_allocation() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(42)).unwrap();
    let idx2 = arena.alloc(Val::V(43)).unwrap();

    assert_eq!(arena.get(idx1).unwrap(), Val::V(42));
    assert_eq!(arena.get(idx2).unwrap(), Val::V(43));
    assert_eq!(arena.len(), 2);
}

#[test]
fn test_free_and_reuse() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(42)).unwrap();
    assert_eq!(arena.len(), 1);

    arena.free(idx1).unwrap();
    assert_eq!(arena.len(), 0);

    let idx2 = arena.alloc(Val::V(43)).unwrap();
    assert_eq!(arena.len(), 1);
    assert_eq!(arena.get(idx2).unwrap(), Val::V(43));
}

#[test]
fn test_out_of_memory() {
    let arena: Arena<Val, 3> = Arena::new();

    assert!(arena.alloc(Val::V(1)).is_ok());
    assert!(arena.alloc(Val::V(2)).is_ok());
    assert!(arena.alloc(Val::V(3)).is_ok());
    assert_eq!(arena.alloc(Val::V(4)), Err(ArenaError::OutOfMemory));
}

#[test]
fn test_invalid_index() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    arena.free(idx).unwrap();

    // After freeing, the index is invalid
    assert_eq!(arena.get(idx), Err(ArenaError::IndexNotAllocated));
}

#[test]
fn test_free_list_o1_allocation() {
    let arena: Arena<Val, 5> = Arena::new();

    // Allocate all slots
    let idx0 = arena.alloc(Val::V(0)).unwrap();
    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();
    let idx4 = arena.alloc(Val::V(4)).unwrap();

    assert!(arena.is_full());

    // Free some slots in non-sequential order
    arena.free(idx2).unwrap();
    arena.free(idx0).unwrap();
    arena.free(idx4).unwrap();

    assert_eq!(arena.len(), 2);
    assert_eq!(arena.available(), 3);

    // Allocate again - should reuse freed slots (LIFO order from free list)
    let new1 = arena.alloc(Val::V(100)).unwrap();
    let new2 = arena.alloc(Val::V(200)).unwrap();
    let new3 = arena.alloc(Val::V(300)).unwrap();

    assert_eq!(arena.len(), 5);

    // Verify the new values are accessible
    assert_eq!(arena.get(new1).unwrap(), Val::V(100));
    assert_eq!(arena.get(new2).unwrap(), Val::V(200));
    assert_eq!(arena.get(new3).unwrap(), Val::V(300));

    // Original unfreed indices should still work
    assert_eq!(arena.get(idx1).unwrap(), Val::V(1));
    assert_eq!(arena.get(idx3).unwrap(), Val::V(3));
}

#[test]
fn test_clear_invalidates_all_indices() {
    let arena: Arena<Val, 10> = Arena::new();

    let idx1 = arena.alloc(Val::V(1)).unwrap();
    let idx2 = arena.alloc(Val::V(2)).unwrap();
    let idx3 = arena.alloc(Val::V(3)).unwrap();

    arena.clear();

    // All old indices should be invalid
    assert_eq!(arena.get(idx1), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.get(idx2), Err(ArenaError::IndexNotAllocated));
    assert_eq!(arena.get(idx3), Err(ArenaError::IndexNotAllocated));

    // New allocations should work
    let new_idx = arena.alloc(Val::V(42)).unwrap();
    assert_eq!(arena.get(new_idx).unwrap(), Val::V(42));
}

#[test]
fn test_stats() {
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
fn test_clear() {
    let arena: Arena<Val, 10> = Arena::new();

    arena.alloc(Val::V(1)).unwrap();
    arena.alloc(Val::V(2)).unwrap();
    arena.alloc(Val::V(3)).unwrap();

    assert_eq!(arena.len(), 3);

    arena.clear();

    assert_eq!(arena.len(), 0);
    assert!(arena.is_empty());
}

// Example of recursive deletion
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
            Tree::Free(_) => Ok(()),
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
fn test_recursive_delete() {
    let arena: Arena<Tree, 100> = Arena::new();

    let left = arena.alloc(Tree::Leaf(1)).unwrap();
    let right = arena.alloc(Tree::Leaf(2)).unwrap();
    let root = arena.alloc(Tree::Branch(left, right)).unwrap();

    assert_eq!(arena.len(), 3);

    arena.delete_recursive(root).unwrap();

    assert_eq!(arena.len(), 0);
}

#[test]
fn test_deep_copy() {
    let arena: Arena<Tree, 100> = Arena::new();

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
    let arena: Arena<Val, 10> = Arena::new();

    let idx = arena.alloc(Val::V(42)).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(42));

    arena.set(idx, Val::V(100)).unwrap();
    assert_eq!(arena.get(idx).unwrap(), Val::V(100));
}

#[test]
fn test_trace_error() {
    // Test the new TraceError variant
    let err = ArenaError::TraceError;
    assert_eq!(err.as_str(), "Error during GC tracing");
    assert!(err.is_trace_error());
    assert!(!err.is_out_of_memory());
    assert!(!err.is_invalid_index());
}
