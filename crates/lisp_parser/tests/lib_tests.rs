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
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(42)));
    
    let idx = parse(&lisp, "-123").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(-123)));
}

#[test]
fn test_parse_symbol() {
    let lisp: Lisp<100> = Lisp::new();
    
    let idx = parse(&lisp, "hello").unwrap();
    assert!(lisp.symbol_matches(idx, "hello").unwrap());
}

#[test]
fn test_parse_empty_list() {
    let lisp: Lisp<100> = Lisp::new();
    
    // () is the empty list
    let idx = parse(&lisp, "()").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Nil);
    
    // 'nil' is now just a regular symbol in Scheme, not the empty list
    let idx = parse(&lisp, "nil").unwrap();
    assert!(lisp.get(idx).unwrap().is_symbol());
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
    assert_eq!(lisp.get(car).unwrap(), Value::Number(Number::integer(1)));
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
    
    // String value should be Value::String { data, len }
    match lisp.get(hello).unwrap() {
        Value::String { data, len } => {
            assert_eq!(len, 5);
            
            // Data slots should contain Char values
            let idx0 = lisp.arena().index_at_offset(data, 0).unwrap();
            let idx1 = lisp.arena().index_at_offset(data, 1).unwrap();
            
            assert_eq!(lisp.get(idx0).unwrap(), Value::Char('h'));
            assert_eq!(lisp.get(idx1).unwrap(), Value::Char('e'));
        }
        other => panic!("Expected Value::String, got {:?}", other),
    }
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
    
    assert_eq!(lisp.get(elem0).unwrap(), Value::Number(Number::integer(42)));
    assert_eq!(lisp.get(elem1).unwrap(), Value::Nil);  // Unchanged
    assert_eq!(lisp.get(elem2).unwrap(), Value::Number(Number::integer(100)));
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
    
    // Array should have allocated: 5 data slots + 1 Array value = 6 slots
    assert_eq!(after_alloc - initial, 6);
    
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
    let arr = lisp.make_array(10, nil).unwrap();
    
    // Should use: 10 data slots + 1 Array value = 11 slots
    let after = lisp.arena().len();
    assert_eq!(after - initial, 11);
}

#[test]
fn test_array_type_name() {
    let arr = Value::Array { 
        data: ArenaIndex::NULL, 
        len: 0 
    };
    assert_eq!(arr.type_name(), "array");
}

// ========================================================================
// String/Array Unification Tests
// ========================================================================

#[test]
fn test_string_type_name() {
    let s = Value::String { 
        data: ArenaIndex::NULL, 
        len: 0 
    };
    assert_eq!(s.type_name(), "string");
}

#[test]
fn test_string_is_string_predicate() {
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let s = lisp.string("hello").unwrap();
    let arr = lisp.make_array(3, nil).unwrap();
    let num = lisp.number(42).unwrap();
    
    assert!(lisp.get(s).unwrap().is_string());
    assert!(!lisp.get(arr).unwrap().is_string());
    assert!(!lisp.get(num).unwrap().is_string());
    assert!(!lisp.get(nil).unwrap().is_string());
}

#[test]
fn test_string_and_array_consistent_layout() {
    // This test verifies that strings and arrays have consistent memory layouts:
    // Both use Value::Type { data, len } with data pointing to contiguous storage
    let lisp: Lisp<1000> = Lisp::new();
    let nil = lisp.nil().unwrap();
    
    let initial = lisp.arena().len();
    
    // Create a string with 5 characters
    let s = lisp.string("hello").unwrap();
    let after_string = lisp.arena().len();
    
    // String should use: 5 data slots + 1 String value = 6 slots
    assert_eq!(after_string - initial, 6);
    
    // Create an array with 5 elements
    let arr = lisp.make_array(5, nil).unwrap();
    let after_array = lisp.arena().len();
    
    // Array should use: 5 data slots + 1 Array value = 6 slots
    assert_eq!(after_array - after_string, 6);
    
    // Verify consistent structure
    match lisp.get(s).unwrap() {
        Value::String { len, .. } => assert_eq!(len, 5),
        _ => panic!("Expected String"),
    }
    
    match lisp.get(arr).unwrap() {
        Value::Array { len, .. } => assert_eq!(len, 5),
        _ => panic!("Expected Array"),
    }
}

#[test]
fn test_string_gc_trace() {
    // Verify that GC properly traces strings (they should survive collection)
    let lisp: Lisp<200> = Lisp::new();
    
    let s = lisp.string("hello world").unwrap();
    
    // Create some garbage
    for i in 0..50 {
        lisp.number(i * 1000).unwrap();
    }
    
    // Run GC with the string as a root
    let stats = lisp.gc(&[s]);
    assert!(stats.collected > 0);
    
    // String should still be valid
    assert!(lisp.get(s).unwrap().is_string());
    assert_eq!(lisp.string_len(s).unwrap(), 11);
    assert!(lisp.string_matches(s, "hello world").unwrap());
}

// ========================================================================
// Numerical Tower Tests
// ========================================================================

#[test]
fn test_parse_integer() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Basic integers
    let idx = parse(&lisp, "42").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(42)));
    
    let idx = parse(&lisp, "-123").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(-123)));
    
    let idx = parse(&lisp, "+456").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(456)));
}

#[test]
fn test_parse_float() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Basic floats
    let idx = parse(&lisp, "3.14").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!(n.is_inexact());
            assert!((n.to_f64() - 3.14).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
    
    // Leading decimal
    let idx = parse(&lisp, ".5").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => assert!((n.to_f64() - 0.5).abs() < 0.0001),
        _ => panic!("Expected Number"),
    }
    
    // Trailing decimal
    let idx = parse(&lisp, "5.").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => assert!((n.to_f64() - 5.0).abs() < 0.0001),
        _ => panic!("Expected Number"),
    }
    
    // Scientific notation
    let idx = parse(&lisp, "1.5e2").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => assert!((n.to_f64() - 150.0).abs() < 0.0001),
        _ => panic!("Expected Number"),
    }
}

#[test]
fn test_parse_rational() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Basic rationals
    let idx = parse(&lisp, "3/4").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(Number::Rational { num, denom }) => {
            assert_eq!(num, 3);
            assert_eq!(denom, 4);
        }
        _ => panic!("Expected Rational"),
    }
    
    // Rational that reduces to integer
    let idx = parse(&lisp, "6/2").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(3)));
    
    // Negative rational
    let idx = parse(&lisp, "-3/4").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(Number::Rational { num, denom }) => {
            assert_eq!(num, -3);
            assert_eq!(denom, 4);
        }
        _ => panic!("Expected Rational"),
    }
}

#[test]
fn test_parse_complex_rectangular() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Basic complex
    let idx = parse(&lisp, "3+4i").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!(!n.is_real());
            assert!((n.real_part().to_f64() - 3.0).abs() < 0.0001);
            assert!((n.imag_part().to_f64() - 4.0).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
    
    // Negative imaginary
    let idx = parse(&lisp, "1-2i").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!((n.real_part().to_f64() - 1.0).abs() < 0.0001);
            assert!((n.imag_part().to_f64() - (-2.0)).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
    
    // Pure imaginary
    let idx = parse(&lisp, "+3i").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!((n.real_part().to_f64()).abs() < 0.0001);
            assert!((n.imag_part().to_f64() - 3.0).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
    
    // Imaginary unit
    let idx = parse(&lisp, "+i").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!((n.real_part().to_f64()).abs() < 0.0001);
            assert!((n.imag_part().to_f64() - 1.0).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
    
    let idx = parse(&lisp, "-i").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!((n.real_part().to_f64()).abs() < 0.0001);
            assert!((n.imag_part().to_f64() - (-1.0)).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
}

#[test]
fn test_parse_special_floats() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Positive infinity
    let idx = parse(&lisp, "+inf.0").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!(n.is_infinite());
            assert!(n.to_f64().is_sign_positive());
        }
        _ => panic!("Expected Number"),
    }
    
    // Negative infinity
    let idx = parse(&lisp, "-inf.0").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!(n.is_infinite());
            assert!(n.to_f64().is_sign_negative());
        }
        _ => panic!("Expected Number"),
    }
    
    // NaN
    let idx = parse(&lisp, "+nan.0").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => assert!(n.is_nan()),
        _ => panic!("Expected Number"),
    }
}

#[test]
fn test_parse_radix_prefixes() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Binary
    let idx = parse(&lisp, "#b1010").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(10)));
    
    // Octal
    let idx = parse(&lisp, "#o755").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(493)));
    
    // Hexadecimal
    let idx = parse(&lisp, "#xFF").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(255)));
    
    // Decimal (explicit)
    let idx = parse(&lisp, "#d42").unwrap();
    assert_eq!(lisp.get(idx).unwrap(), Value::Number(Number::integer(42)));
}

#[test]
fn test_parse_exactness_prefixes() {
    let lisp: Lisp<1000> = Lisp::new();
    
    // Force exact
    let idx = parse(&lisp, "#e3.14").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => assert!(n.is_exact()),
        _ => panic!("Expected Number"),
    }
    
    // Force inexact
    let idx = parse(&lisp, "#i42").unwrap();
    match lisp.get(idx).unwrap() {
        Value::Number(n) => {
            assert!(n.is_inexact());
            assert!((n.to_f64() - 42.0).abs() < 0.0001);
        }
        _ => panic!("Expected Number"),
    }
}

#[test]
fn test_number_type_predicates() {
    // Integer is: integer, rational, real, complex, exact
    let n = Number::integer(42);
    assert!(n.is_integer());
    assert!(n.is_rational());
    assert!(n.is_real());
    assert!(n.is_complex());
    assert!(n.is_exact());
    assert!(!n.is_inexact());
    
    // Float is: rational (if finite), real, complex, inexact
    let n = Number::float(3.14);
    assert!(!n.is_integer());
    assert!(n.is_rational()); // finite floats are rational
    assert!(n.is_real());
    assert!(n.is_complex());
    assert!(!n.is_exact());
    assert!(n.is_inexact());
    
    // Rational is: rational, real, complex, exact
    let n = Number::rational(3, 4);
    assert!(!n.is_integer());
    assert!(n.is_rational());
    assert!(n.is_real());
    assert!(n.is_complex());
    assert!(n.is_exact());
    assert!(!n.is_inexact());
    
    // Complex is: complex, inexact
    let n = Number::complex(3.0, 4.0);
    assert!(!n.is_integer());
    assert!(!n.is_rational());
    assert!(!n.is_real());
    assert!(n.is_complex());
    assert!(!n.is_exact());
    assert!(n.is_inexact());
}

#[test]
fn test_number_arithmetic() {
    // Integer arithmetic
    let a = Number::integer(10);
    let b = Number::integer(3);
    assert_eq!(a.add(&b), Number::integer(13));
    assert_eq!(a.sub(&b), Number::integer(7));
    assert_eq!(a.mul(&b), Number::integer(30));
    
    // Integer division produces rational
    match a.div(&b) {
        Number::Rational { num, denom } => {
            assert_eq!(num, 10);
            assert_eq!(denom, 3);
        }
        _ => panic!("Expected Rational"),
    }
    
    // Rational arithmetic
    let r1 = Number::rational(1, 2);
    let r2 = Number::rational(1, 3);
    match r1.add(&r2) {
        Number::Rational { num, denom } => {
            assert_eq!(num, 5);
            assert_eq!(denom, 6);
        }
        _ => panic!("Expected Rational"),
    }
}

#[test]
fn test_complex_operations() {
    // Magnitude of 3+4i = 5
    let c = Number::complex(3.0, 4.0);
    assert!((c.magnitude().to_f64() - 5.0).abs() < 0.0001);
    
    // Real and imaginary parts
    assert!((c.real_part().to_f64() - 3.0).abs() < 0.0001);
    assert!((c.imag_part().to_f64() - 4.0).abs() < 0.0001);
    
    // Polar form
    let p = Number::from_polar(5.0, 0.0);
    assert!((p.real_part().to_f64() - 5.0).abs() < 0.0001);
    assert!(p.imag_part().to_f64().abs() < 0.0001);
}
