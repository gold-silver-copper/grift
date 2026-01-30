

Refer to scheme spec md file and spec.html. Consult and update Architecture documentation.  Read all code for source of truth and grep information.

Continue with Scheme conformance implementation.

Write tests and docs for all new features as you progress. Document progress in the  MD files. Add to bench.rs if new things are added.

I believe that much of the documentation of the README the architecture documents and the scheme conformance document might be inaccurate. Please go through all the claims in those documents and research them inside the implementation code. Fix the documentation as needed. Write permanent tests for each thing you test.



macro replacement

I have removed most math functions that used to exist in my scheme because they were poorly implemented. If you need any math functions to test, reimplement them in stdlib.scm

Please improve and optimize the function of the evaluator, the enviornment, and of continuations in my lisp.

The following tests fails when ran without release mode: thread 'test_nested_stdlib_calls' (38627484) has overflowed its stack

Even with small saved arrays, each call to eval_preserving_stack adds a new call frame on the Rust stack. For deeply nested stdlib calls (like sqrt which calls sqrt-iter multiple times), this creates many nested calls.

The proper solution would be to convert let and similar constructs to use continuations instead of synchronous calls.


All possible things that can be stored inside the arena, should be stored inside the arena.