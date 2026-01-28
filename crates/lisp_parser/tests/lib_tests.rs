use lisp_parser::*;

// ========================================================================
// Reserved Slots Tests
// ========================================================================

#[test]
fn test_reserved_slots_are_singletons() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Multiple calls to nil() should return the same index
    let nil1 = lisp.nil().unwrap();
    let nil2 = lisp.nil().unwrap();
    let nil3 = lisp.nil().unwrap();
    assert_eq!(nil1, nil2);
    assert_eq!(nil2, nil3);
    
    // Multiple calls to true_val() should return the same index
    let true1 = lisp.true_val().unwrap();
    let true2 = lisp.true_val().unwrap();
    let true3 = lisp.true_val().unwrap();
    assert_eq!(true1, true2);
    assert_eq!(true2, true3);
    
    // Multiple calls to false_val() should return the same index
    let false1 = lisp.false_val().unwrap();
    let false2 = lisp.false_val().unwrap();
    let false3 = lisp.false_val().unwrap();
    assert_eq!(false1, false2);
    assert_eq!(false2, false3);
    
    // Each singleton should be different
    assert_ne!(nil1, true1);
    assert_ne!(nil1, false1);
    assert_ne!(true1, false1);
}

#[test]
fn test_reserved_slots_have_correct_values() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Verify the values in reserved slots
    assert_eq!(lisp.get(lisp.nil().unwrap()).unwrap(), Value::Nil);
    assert_eq!(lisp.get(lisp.true_val().unwrap()).unwrap(), Value::True);
    assert_eq!(lisp.get(lisp.false_val().unwrap()).unwrap(), Value::False);
}

#[test]
fn test_reserved_slots_occupy_first_three_slots() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Reserved slots should be the first 4 slots (nil, true, false, intern_table_ref)
    assert_eq!(lisp.nil().unwrap().raw(), 0);
    assert_eq!(lisp.true_val().unwrap().raw(), 1);
    assert_eq!(lisp.false_val().unwrap().raw(), 2);
}

#[test]
fn test_reserved_slots_not_reallocated() {
    let lisp: Lisp<100> = Lisp::new();
    
    // After creating the Lisp context, 4 slots should be used
    // (nil, true, false, intern_table_ref)
    assert_eq!(lisp.arena().len(), 4);
    
    // Calling nil/true_val/false_val should NOT increase allocation count
    let _ = lisp.nil();
    let _ = lisp.true_val();
    let _ = lisp.false_val();
    assert_eq!(lisp.arena().len(), 4);
    
    // Calling many times should not increase count
    for _ in 0..100 {
        let _ = lisp.nil();
        let _ = lisp.true_val();
        let _ = lisp.false_val();
    }
    assert_eq!(lisp.arena().len(), 4);
}

#[test]
fn test_boolean_uses_reserved_slots() {
    let lisp: Lisp<100> = Lisp::new();
    
    // boolean() should use the reserved slots
    let b_true = lisp.boolean(true).unwrap();
    let b_false = lisp.boolean(false).unwrap();
    
    assert_eq!(b_true, lisp.true_val().unwrap());
    assert_eq!(b_false, lisp.false_val().unwrap());
}

#[test]
fn test_reserved_slots_survive_gc() {
    let lisp: Lisp<100> = Lisp::new();
    
    let nil = lisp.nil().unwrap();
    let true_val = lisp.true_val().unwrap();
    let false_val = lisp.false_val().unwrap();
    
    // Allocate some garbage
    let _ = lisp.number(1);
    let _ = lisp.number(2);
    let _ = lisp.number(3);
    
    // Run GC with empty roots - reserved slots should NOT be collected
    // because they're implicitly roots
    let stats = lisp.gc(&[nil, true_val, false_val]);
    
    // The numbers should be collected
    assert_eq!(stats.collected, 3);
    
    // Reserved slots should still be valid
    assert_eq!(lisp.get(nil).unwrap(), Value::Nil);
    assert_eq!(lisp.get(true_val).unwrap(), Value::True);
    assert_eq!(lisp.get(false_val).unwrap(), Value::False);
}

#[test]
fn test_regular_allocation_starts_after_reserved_slots() {
    let lisp: Lisp<100> = Lisp::new();
    
    // First regular allocation should be at slot 4 (after reserved 0, 1, 2, 3)
    // Slots: 0=nil, 1=true, 2=false, 3=intern_table_ref
    let num = lisp.number(42).unwrap();
    assert_eq!(num.raw(), 4);
    
    // Next allocations continue from there
    let num2 = lisp.number(43).unwrap();
    assert_eq!(num2.raw(), 5);
}

// ========================================================================
// Parser Tests
// ========================================================================

#[test]
fn test_parse_number() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "42").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(42));
    
    let idx = parse(&lisp, "-123").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(-123));
}

#[test]
fn test_parse_symbol() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "hello").unwrap();
    assert!(lisp.symbol_matches(idx, "hello").unwrap());
}

#[test]
fn test_parse_nil() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "nil").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
    
    let idx = parse(&lisp, "()").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
}

#[test]
fn test_parse_booleans() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "#t").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::True);
    
    let idx = parse(&lisp, "#f").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::False);
    
    let idx = parse(&lisp, "#T").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::True);
    
    let idx = parse(&lisp, "#F").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::False);
}

#[test]
fn test_parse_list() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "(1 2 3)").unwrap();
    
    // Check it's a cons
    let val = lisp.get(idx).unwrap();
    assert!(val.is_cons());
    
    // Check first element
    let car = lisp.car(idx).unwrap();
    assert_eq!(lisp.get(car).unwrap(), Value::Number(1));
}

#[test]
fn test_parse_nested() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "(+ 1 (- 3 2))").unwrap();
    assert!(lisp.get(idx).unwrap().is_cons());
}

#[test]
fn test_parse_quote() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "'x").unwrap();
    
    // Should be (quote x)
    let car = lisp.car(idx).unwrap();
    assert!(lisp.symbol_matches(car, "quote").unwrap());
}

#[test]
fn test_symbol_equality() {
    let lisp: Lisp<100> = Lisp::new();
    
    let a = lisp.symbol("hello").unwrap();
    let b = lisp.symbol("hello").unwrap();
    let c = lisp.symbol("world").unwrap();
    
    assert!(lisp.symbol_eq(a, b).unwrap());
    assert!(!lisp.symbol_eq(a, c).unwrap());
}

// NOTE: test_set_car_cdr removed - this is a PURE Lisp!

#[test]
fn test_gc() {
    let lisp: Lisp<100> = Lisp::new();
    
    let root = parse(&lisp, "(1 2 3)").unwrap();
    
    // Allocate garbage
    for i in 0..20 {
        lisp.number(i * 1000).unwrap();
    }
    
    let stats = lisp.gc(&[root]);
    assert!(stats.collected > 0);
}

#[test]
fn test_thunk_creation() {
    let lisp: Lisp<100> = Lisp::new();
    
    let expr = lisp.number(42).unwrap();
    let env = lisp.nil().unwrap();
    let thunk = lisp.thunk(expr, env).unwrap();
    
    assert!(lisp.get(thunk).unwrap().is_thunk());
}

#[test]
fn test_thunk_structure() {
    let lisp: Lisp<100> = Lisp::new();
    
    let expr = lisp.number(42).unwrap();
    let env = lisp.nil().unwrap();
    let thunk = lisp.thunk(expr, env).unwrap();
    
    // Check thunk structure
    match lisp.get(thunk).unwrap() {
        Value::Thunk { expr: e, env: en, cached: c } => {
            assert_eq!(lisp.get(e).unwrap(), Value::Number(42));
            assert_eq!(lisp.get(en).unwrap(), Value::Nil);
            assert!(c.is_null()); // Not yet cached
        }
        _ => panic!("Expected Thunk"),
    }
}

#[test]
fn test_thunk_with_complex_expr() {
    let lisp: Lisp<200> = Lisp::new();
    
    // Create a complex expression as the delayed expr
    let one = lisp.number(1).unwrap();
    let two = lisp.number(2).unwrap();
    let pair = lisp.cons(one, two).unwrap();
    let env = lisp.nil().unwrap();
    
    let thunk = lisp.thunk(pair, env).unwrap();
    
    assert!(lisp.get(thunk).unwrap().is_thunk());
    
    // Verify the expr is our pair
    match lisp.get(thunk).unwrap() {
        Value::Thunk { expr, .. } => {
            match lisp.get(expr).unwrap() {
                Value::Cons { car, cdr } => {
                    assert_eq!(lisp.get(car).unwrap(), Value::Number(1));
                    assert_eq!(lisp.get(cdr).unwrap(), Value::Number(2));
                }
                _ => panic!("Expected Cons in thunk"),
            }
        }
        _ => panic!("Expected Thunk"),
    }
}

#[test]
fn test_thunk_with_environment() {
    let lisp: Lisp<300> = Lisp::new();
    
    // Create a non-empty environment
    let name = lisp.symbol("x").unwrap();
    let value = lisp.number(100).unwrap();
    let binding = lisp.cons(name, value).unwrap();
    let env = lisp.cons(binding, lisp.nil().unwrap()).unwrap();
    
    let expr = lisp.number(42).unwrap();
    let thunk = lisp.thunk(expr, env).unwrap();
    
    // Verify environment is captured
    match lisp.get(thunk).unwrap() {
        Value::Thunk { env: e, .. } => {
            // First binding
            let first = lisp.car(e).unwrap();
            let bound_name = lisp.car(first).unwrap();
            let bound_val = lisp.cdr(first).unwrap();
            
            assert!(lisp.symbol_matches(bound_name, "x").unwrap());
            assert_eq!(lisp.get(bound_val).unwrap(), Value::Number(100));
        }
        _ => panic!("Expected Thunk"),
    }
}

#[test]
fn test_thunk_gc_trace() {
    // Test that GC properly traces through thunks
    let lisp: Lisp<200> = Lisp::new();
    
    // Create data that will be referenced by thunk
    let inner_data = lisp.cons(lisp.number(1).unwrap(), lisp.number(2).unwrap()).unwrap();
    let env_val = lisp.number(999).unwrap();
    let env_name = lisp.symbol("y").unwrap();
    let binding = lisp.cons(env_name, env_val).unwrap();
    let env = lisp.cons(binding, lisp.nil().unwrap()).unwrap();
    
    let thunk = lisp.thunk(inner_data, env).unwrap();
    
    // Create garbage
    for i in 0..50 {
        lisp.number(i * 1000).unwrap();
    }
    
    // Run GC with thunk as root
    let stats = lisp.gc(&[thunk]);
    assert!(stats.collected > 0);
    
    // Thunk and its referenced data should still be accessible
    assert!(lisp.get(thunk).unwrap().is_thunk());
    match lisp.get(thunk).unwrap() {
        Value::Thunk { expr, env, .. } => {
            // inner_data should still be valid
            assert!(lisp.get(expr).unwrap().is_cons());
            // environment should still be valid
            assert!(lisp.get(env).unwrap().is_cons());
        }
        _ => panic!("Expected Thunk"),
    }
}

#[test]
fn test_thunk_value_predicates() {
    let lisp: Lisp<100> = Lisp::new();
    
    let expr = lisp.number(42).unwrap();
    let env = lisp.nil().unwrap();
    let thunk = lisp.thunk(expr, env).unwrap();
    let thunk_val = lisp.get(thunk).unwrap();
    
    // Test all predicates on thunk
    assert!(thunk_val.is_thunk());
    assert!(!thunk_val.is_nil());
    assert!(!thunk_val.is_number());
    assert!(!thunk_val.is_symbol());
    assert!(!thunk_val.is_cons());
    assert!(!thunk_val.is_lambda());
    assert!(!thunk_val.is_builtin());
    assert!(!thunk_val.is_true());
    assert!(!thunk_val.is_false());
    assert!(!thunk_val.is_boolean());
    assert!(thunk_val.is_atom()); // Thunks are atoms
    assert!(!thunk_val.is_procedure()); // Thunks are not procedures
    
    // Type name
    assert_eq!(thunk_val.type_name(), "promise");
}

#[test]
fn test_multiple_thunks() {
    let lisp: Lisp<500> = Lisp::new();
    
    // Create multiple thunks
    let env = lisp.nil().unwrap();
    let t1 = lisp.thunk(lisp.number(1).unwrap(), env).unwrap();
    let t2 = lisp.thunk(lisp.number(2).unwrap(), env).unwrap();
    let t3 = lisp.thunk(lisp.number(3).unwrap(), env).unwrap();
    
    // Put them in a list
    let list = lisp.cons(t3, lisp.nil().unwrap()).unwrap();
    let list = lisp.cons(t2, list).unwrap();
    let list = lisp.cons(t1, list).unwrap();
    
    // All should be thunks
    let first = lisp.car(list).unwrap();
    assert!(lisp.get(first).unwrap().is_thunk());
    
    let second = lisp.car(lisp.cdr(list).unwrap()).unwrap();
    assert!(lisp.get(second).unwrap().is_thunk());
    
    let third = lisp.car(lisp.cdr(lisp.cdr(list).unwrap()).unwrap()).unwrap();
    assert!(lisp.get(third).unwrap().is_thunk());
}

// ========================================================================
// Contiguous String Tests
// ========================================================================

#[test]
fn test_string_basic() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.string("hello").unwrap();
    
    assert_eq!(lisp.string_len(hello).unwrap(), 5);
    assert_eq!(lisp.string_char_at(hello, 0).unwrap(), 'h');
    assert_eq!(lisp.string_char_at(hello, 1).unwrap(), 'e');
    assert_eq!(lisp.string_char_at(hello, 2).unwrap(), 'l');
    assert_eq!(lisp.string_char_at(hello, 3).unwrap(), 'l');
    assert_eq!(lisp.string_char_at(hello, 4).unwrap(), 'o');
}

#[test]
fn test_string_empty() {
    let lisp: Lisp<100> = Lisp::new();
    
    let empty = lisp.string("").unwrap();
    
    assert_eq!(lisp.string_len(empty).unwrap(), 0);
    // Accessing index 0 on empty string should fail
    assert!(lisp.string_char_at(empty, 0).is_err());
}

#[test]
fn test_string_single_char() {
    let lisp: Lisp<100> = Lisp::new();
    
    let single = lisp.string("x").unwrap();
    
    assert_eq!(lisp.string_len(single).unwrap(), 1);
    assert_eq!(lisp.string_char_at(single, 0).unwrap(), 'x');
    assert!(lisp.string_char_at(single, 1).is_err());
}

#[test]
fn test_string_unicode() {
    let lisp: Lisp<100> = Lisp::new();
    
    // Unicode string: "héllo" (with accent)
    let unicode = lisp.string("héllo").unwrap();
    
    assert_eq!(lisp.string_len(unicode).unwrap(), 5);
    assert_eq!(lisp.string_char_at(unicode, 0).unwrap(), 'h');
    assert_eq!(lisp.string_char_at(unicode, 1).unwrap(), 'é');
    assert_eq!(lisp.string_char_at(unicode, 2).unwrap(), 'l');
}

#[test]
fn test_string_matches() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.string("hello").unwrap();
    
    assert!(lisp.string_matches(hello, "hello").unwrap());
    assert!(!lisp.string_matches(hello, "Hello").unwrap());
    assert!(!lisp.string_matches(hello, "hello!").unwrap());
    assert!(!lisp.string_matches(hello, "hell").unwrap());
    assert!(!lisp.string_matches(hello, "").unwrap());
}

#[test]
fn test_string_eq_contiguous() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello1 = lisp.string("hello").unwrap();
    let hello2 = lisp.string("hello").unwrap();
    let world = lisp.string("world").unwrap();
    
    // Same content
    assert!(lisp.string_eq_contiguous(hello1, hello2).unwrap());
    
    // Same index
    assert!(lisp.string_eq_contiguous(hello1, hello1).unwrap());
    
    // Different content
    assert!(!lisp.string_eq_contiguous(hello1, world).unwrap());
}

#[test]
fn test_string_to_bytes() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.string("hello").unwrap();
    let mut buf = [0u8; 10];
    
    let len = lisp.string_to_bytes(hello, &mut buf).unwrap();
    
    assert_eq!(len, 5);
    assert_eq!(&buf[..5], b"hello");
}

#[test]
fn test_string_to_bytes_truncated() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.string("hello").unwrap();
    let mut buf = [0u8; 3]; // Too small
    
    let len = lisp.string_to_bytes(hello, &mut buf).unwrap();
    
    assert_eq!(len, 3);
    assert_eq!(&buf[..3], b"hel");
}

#[test]
fn test_string_to_bytes_skips_non_ascii() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // "hé" has 2 chars: 'h' (ASCII) and 'é' (non-ASCII)
    let mixed = lisp.string("héllo").unwrap();
    let mut buf = [0u8; 10];
    
    let len = lisp.string_to_bytes(mixed, &mut buf).unwrap();
    
    // Only ASCII chars are copied, 'é' is skipped
    assert_eq!(len, 4); // h, l, l, o
    assert_eq!(&buf[..4], b"hllo");
}

#[test]
fn test_string_free() {
    let lisp: Lisp<100> = Lisp::new();
    
    let initial_allocated = lisp.stats().allocated;
    
    let hello = lisp.string("hello").unwrap();
    let after_alloc = lisp.stats().allocated;
    
    // Should have allocated 6 slots (1 length + 5 chars)
    assert_eq!(after_alloc - initial_allocated, 6);
    
    lisp.string_free(hello).unwrap();
    let after_free = lisp.stats().allocated;
    
    // Should be back to initial
    assert_eq!(after_free, initial_allocated);
}

#[test]
fn test_string_multiple() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let s1 = lisp.string("foo").unwrap();
    let s2 = lisp.string("bar").unwrap();
    let s3 = lisp.string("baz").unwrap();
    
    // All strings should be independent
    assert!(lisp.string_matches(s1, "foo").unwrap());
    assert!(lisp.string_matches(s2, "bar").unwrap());
    assert!(lisp.string_matches(s3, "baz").unwrap());
    
    // Free one, others should still work
    lisp.string_free(s2).unwrap();
    
    assert!(lisp.string_matches(s1, "foo").unwrap());
    assert!(lisp.string_matches(s3, "baz").unwrap());
}

#[test]
fn test_string_memory_layout() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.string("hello").unwrap();
    
    // First slot should be Number(5)
    assert_eq!(lisp.get(hello).unwrap(), Value::Number(5));
    
    // Following slots should be Char values
    let idx1 = lisp.arena().index_at_offset(hello, 1).unwrap();
    let idx2 = lisp.arena().index_at_offset(hello, 2).unwrap();
    
    assert_eq!(lisp.get(idx1).unwrap(), Value::Char('h'));
    assert_eq!(lisp.get(idx2).unwrap(), Value::Char('e'));
}

#[test]
fn test_string_char_out_of_bounds() {
    let lisp: Lisp<100> = Lisp::new();
    
    let hello = lisp.string("hi").unwrap();
    
    assert!(lisp.string_char_at(hello, 0).is_ok());
    assert!(lisp.string_char_at(hello, 1).is_ok());
    assert!(lisp.string_char_at(hello, 2).is_err());
    assert!(lisp.string_char_at(hello, 100).is_err());
}

// ========================================================================
// Mutation Tests (set_car, set_cdr)
// ========================================================================

#[test]
fn test_set_car() {
    let lisp: Lisp<100> = Lisp::new();
    
    let a = lisp.number(1).unwrap();
    let b = lisp.number(2).unwrap();
    let c = lisp.number(3).unwrap();
    
    let pair = lisp.cons(a, b).unwrap();
    
    // Initially car is a (1)
    assert_eq!(lisp.car(pair).unwrap(), a);
    
    // Mutate car to c (3)
    lisp.set_car(pair, c).unwrap();
    
    // Now car should be c
    assert_eq!(lisp.car(pair).unwrap(), c);
    
    // cdr should be unchanged
    assert_eq!(lisp.cdr(pair).unwrap(), b);
}

#[test]
fn test_set_cdr() {
    let lisp: Lisp<100> = Lisp::new();
    
    let a = lisp.number(1).unwrap();
    let b = lisp.number(2).unwrap();
    let c = lisp.number(3).unwrap();
    
    let pair = lisp.cons(a, b).unwrap();
    
    // Initially cdr is b (2)
    assert_eq!(lisp.cdr(pair).unwrap(), b);
    
    // Mutate cdr to c (3)
    lisp.set_cdr(pair, c).unwrap();
    
    // Now cdr should be c
    assert_eq!(lisp.cdr(pair).unwrap(), c);
    
    // car should be unchanged
    assert_eq!(lisp.car(pair).unwrap(), a);
}

#[test]
fn test_set_car_on_non_pair_fails() {
    let lisp: Lisp<100> = Lisp::new();
    
    let num = lisp.number(42).unwrap();
    let new_val = lisp.number(99).unwrap();
    
    assert!(lisp.set_car(num, new_val).is_err());
}

#[test]
fn test_set_cdr_on_non_pair_fails() {
    let lisp: Lisp<100> = Lisp::new();
    
    let num = lisp.number(42).unwrap();
    let new_val = lisp.number(99).unwrap();
    
    assert!(lisp.set_cdr(num, new_val).is_err());
}

// ========================================================================
// Symbol Interning Tests
// ========================================================================

#[test]
fn test_symbol_interning_same_name_returns_same_index() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let sym1 = lisp.symbol("foo").unwrap();
    let sym2 = lisp.symbol("foo").unwrap();
    let sym3 = lisp.symbol("foo").unwrap();
    
    // Same symbol name should return the same index
    assert_eq!(sym1, sym2);
    assert_eq!(sym2, sym3);
}

#[test]
fn test_symbol_interning_different_names_return_different_indices() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let foo = lisp.symbol("foo").unwrap();
    let bar = lisp.symbol("bar").unwrap();
    let baz = lisp.symbol("baz").unwrap();
    
    // Different symbol names should return different indices
    assert_ne!(foo, bar);
    assert_ne!(bar, baz);
    assert_ne!(foo, baz);
}

#[test]
fn test_symbol_interning_from_bytes() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let sym1 = lisp.symbol("test").unwrap();
    let sym2 = lisp.symbol_from_bytes(b"test").unwrap();
    
    // Same content should return the same symbol
    assert_eq!(sym1, sym2);
}

#[test]
fn test_symbol_interning_preserves_content() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let sym = lisp.symbol("hello").unwrap();
    
    // The symbol should still match its name
    assert!(lisp.symbol_matches(sym, "hello").unwrap());
    assert!(!lisp.symbol_matches(sym, "world").unwrap());
}

#[test]
fn test_intern_table_is_gc_root() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Create some interned symbols
    let sym1 = lisp.symbol("a").unwrap();
    let sym2 = lisp.symbol("b").unwrap();
    let sym3 = lisp.symbol("c").unwrap();
    
    // Create some garbage
    for i in 0..50 {
        let _ = lisp.number(i);
    }
    
    // Run GC with no explicit roots
    let empty_roots: &[ArenaIndex] = &[];
    lisp.gc(empty_roots);
    
    // Interned symbols should still be accessible
    assert!(lisp.get(sym1).is_ok());
    assert!(lisp.get(sym2).is_ok());
    assert!(lisp.get(sym3).is_ok());
    
    // And should still match their names
    assert!(lisp.symbol_matches(sym1, "a").unwrap());
    assert!(lisp.symbol_matches(sym2, "b").unwrap());
    assert!(lisp.symbol_matches(sym3, "c").unwrap());
}

// ========================================================================
// Symbol Helper Method Tests
// ========================================================================

#[test]
fn test_symbol_len() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let empty = lisp.symbol("").unwrap();
    let short = lisp.symbol("hi").unwrap();
    let longer = lisp.symbol("hello world").unwrap();
    
    assert_eq!(lisp.symbol_len(empty).unwrap(), 0);
    assert_eq!(lisp.symbol_len(short).unwrap(), 2);
    assert_eq!(lisp.symbol_len(longer).unwrap(), 11);
}

#[test]
fn test_symbol_char_at() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.symbol("hello").unwrap();
    
    assert_eq!(lisp.symbol_char_at(hello, 0).unwrap(), Some('h'));
    assert_eq!(lisp.symbol_char_at(hello, 1).unwrap(), Some('e'));
    assert_eq!(lisp.symbol_char_at(hello, 2).unwrap(), Some('l'));
    assert_eq!(lisp.symbol_char_at(hello, 3).unwrap(), Some('l'));
    assert_eq!(lisp.symbol_char_at(hello, 4).unwrap(), Some('o'));
    assert_eq!(lisp.symbol_char_at(hello, 5).unwrap(), None);
    assert_eq!(lisp.symbol_char_at(hello, 100).unwrap(), None);
}

#[test]
fn test_symbol_to_bytes() {
    let lisp: Lisp<1000> = Lisp::new();
    
    let hello = lisp.symbol("hello").unwrap();
    
    let mut buf = [0u8; 32];
    let len = lisp.symbol_to_bytes(hello, &mut buf).unwrap();
    
    assert_eq!(len, 5);
    assert_eq!(&buf[..len], b"hello");
}

#[test]
fn test_contiguous_symbol_uses_less_memory() {
    let lisp: Lisp<10000> = Lisp::new();
    
    // Get initial allocation count (includes reserved slots)
    let initial = lisp.arena().len();
    
    // Create a symbol with contiguous strings
    // "factorial" (9 chars) = 1 (length) + 9 (chars) + 1 (Symbol) = 11 slots
    // Plus 2 slots for the intern table entry
    let _sym = lisp.symbol("factorial").unwrap();
    
    let after_symbol = lisp.arena().len();
    let slots_used = after_symbol - initial;
    
    // Old format would use: 9 Char + 9 Cons + 1 Symbol = 19 slots
    // New format uses: 1 Number + 9 Char + 1 Symbol + 2 Cons (intern table) = 13 slots
    // So we expect significantly fewer slots
    assert!(slots_used < 19, "Expected fewer than 19 slots, got {}", slots_used);
}

// ========================================================================
// Array Tests
// ========================================================================

#[test]
fn test_array_basic() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Create an array of 5 elements initialized to nil
    let arr = lisp.make_array(5, nil).unwrap();
    
    // Check length
    assert_eq!(lisp.array_len(arr).unwrap(), 5);
    
    // Check all elements are nil
    for i in 0..5 {
        let elem = lisp.array_get(arr, i).unwrap();
        assert_eq!(lisp.get(elem).unwrap(), Value::Nil);
    }
}

#[test]
fn test_array_empty() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Create an empty array
    let arr = lisp.make_array(0, nil).unwrap();
    
    // Check length is 0
    assert_eq!(lisp.array_len(arr).unwrap(), 0);
    
    // Check it's actually an array
    assert!(lisp.get(arr).unwrap().is_array());
}

#[test]
fn test_array_get_set() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    // Create array
    let arr = lisp.make_array(3, nil).unwrap();
    
    // Set values
    let val1 = lisp.number(42).unwrap();
    let val2 = lisp.number(100).unwrap();
    lisp.array_set(arr, 0, val1).unwrap();
    lisp.array_set(arr, 2, val2).unwrap();
    
    // Get values back
    let elem0 = lisp.array_get(arr, 0).unwrap();
    let elem1 = lisp.array_get(arr, 1).unwrap();
    let elem2 = lisp.array_get(arr, 2).unwrap();
    
    assert_eq!(lisp.get(elem0).unwrap(), Value::Number(42));
    assert_eq!(lisp.get(elem1).unwrap(), Value::Nil);  // Unchanged
    assert_eq!(lisp.get(elem2).unwrap(), Value::Number(100));
}

#[test]
fn test_array_out_of_bounds() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let arr = lisp.make_array(3, nil).unwrap();
    
    // Valid index
    assert!(lisp.array_get(arr, 2).is_ok());
    
    // Out of bounds
    assert!(lisp.array_get(arr, 3).is_err());
    assert!(lisp.array_get(arr, 100).is_err());
    
    // Out of bounds set
    assert!(lisp.array_set(arr, 3, nil).is_err());
}

#[test]
fn test_array_with_different_value_types() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let arr = lisp.make_array(4, nil).unwrap();
    
    // Set different value types
    let num = lisp.number(123).unwrap();
    let b = lisp.true_val().unwrap();
    let sym = lisp.symbol("foo").unwrap();
    let pair = lisp.cons(lisp.number(1).unwrap(), lisp.number(2).unwrap()).unwrap();
    
    lisp.array_set(arr, 0, num).unwrap();
    lisp.array_set(arr, 1, b).unwrap();
    lisp.array_set(arr, 2, sym).unwrap();
    lisp.array_set(arr, 3, pair).unwrap();
    
    // Verify
    let elem0 = lisp.array_get(arr, 0).unwrap();
    let elem1 = lisp.array_get(arr, 1).unwrap();
    let elem2 = lisp.array_get(arr, 2).unwrap();
    let elem3 = lisp.array_get(arr, 3).unwrap();
    
    assert!(lisp.get(elem0).unwrap().is_number());
    assert!(lisp.get(elem1).unwrap().is_true());
    assert!(lisp.get(elem2).unwrap().is_symbol());
    assert!(lisp.get(elem3).unwrap().is_cons());
}

#[test]
fn test_array_is_array_predicate() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let arr = lisp.make_array(3, nil).unwrap();
    let num = lisp.number(42).unwrap();
    let pair = lisp.cons(nil, nil).unwrap();
    
    assert!(lisp.get(arr).unwrap().is_array());
    assert!(!lisp.get(num).unwrap().is_array());
    assert!(!lisp.get(pair).unwrap().is_array());
    assert!(!lisp.get(nil).unwrap().is_array());
}

#[test]
fn test_array_len_on_non_array() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    assert!(lisp.array_len(nil).is_err());
}

#[test]
fn test_array_free() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let initial = lisp.arena().len();
    
    let arr = lisp.make_array(5, nil).unwrap();
    let after_alloc = lisp.arena().len();
    
    // Array should have allocated: 1 length slot + 5 element slots + 1 Array value = 7 slots
    assert_eq!(after_alloc - initial, 7);
    
    lisp.array_free(arr).unwrap();
    let after_free = lisp.arena().len();
    
    // All slots should be freed
    assert_eq!(after_free, initial);
}

#[test]
fn test_array_memory_layout() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let initial = lisp.arena().len();
    
    // Create array of 10 elements
    let _arr = lisp.make_array(10, nil).unwrap();
    
    // Should use: 1 length slot + 10 element slots + 1 Array value = 12 slots
    let after = lisp.arena().len();
    assert_eq!(after - initial, 12);
}

#[test]
fn test_array_type_name() {
    let arr = Value::Array { 
        data: ArenaIndex::NULL, 
        len: 0 
    };
    assert_eq!(arr.type_name(), "array");
}
