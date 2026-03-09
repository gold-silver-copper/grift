Make sure that the following features are implemented properly in Grift according to the Kernal / vau-calculus spec:

The boolean data type consists of two values, which are called true and false, and have
respectively external representations #t and #f. There are no possible mutations of
either of these two values, and the boolean type is encapsulated.
A combiner that always returns a boolean value is called a predicate. Conversely,
specifying that a combiner is a predicate means that it always returns a boolean value.
By convention, the names of predicates always end in “?”.
Library features associated with type boolean will be described in §6.1.
Rationale:
The essential purpose of boolean values in the language is to serve as the results of
conditional tests, for which purpose only boolean values are permitted by Kernel. (See
rationale under Partitioning of types, §3.5.)
4.1.1 boolean?
(boolean? . objects)
The primitive type predicate for type boolean.
Because there are only two values in the type, predicate boolean? could be constructed as a library combiner. However, designating it a primitive type predicate
satisfies the requirements for partitioning of types from §3.5.
4.2 Equivalence under mutation (optional)
Rationale:
Kernel has two general-purpose equivalence predicates, whereas R5RS Scheme has
three. The two Kernel predicates correspond to the abstract notions of equivalence up to
mutation (equal?, §4.3), and in the presence of mutation (eq?, in this module). Scheme
assigns the abstract notion of equivalence in the presence of mutation to an intermediate
predicate eqv?, and uses predicate eq? for the technically stronger equivalence between
objects whose eqv? -ness can be verified especially quickly in any particular implementation. In language design terms, Scheme introduces a third equivalence predicate for
the express purpose of promoting implementation-dependent intrusion of concrete performance issues on the abstract semantics of the language — which directly violates Kernel’s
principles on simplicity and generality as well as its guideline on efficiency (G5 of §0.1.2).
The criterion for another module to assume this one is that the assuming module
supports mutation that could cause objects to be equal? but not eq? . For example,
Pair mutation assumes this module, but Environment mutation does not.
For cross-implementation compatibility, the behavior of eq? is defined in terms of a
comprehensive implementation of Kernel. For example, two pairs returned by different
calls to cons are not eq?, even if they have the same car and cdr and the implementation doesn’t support pair mutation; and two empty environments returned by different
calls to make-environment are not eq?, even if the implementation doesn’t support environment mutation. The latter case shows how the implementation-independence can
impact implementations even if they don’t support eq?, since the behavior of required
predicate equal? on environments is tied to that of eq? (which is, in turn, why module
Environment mutation does not require this module).
4.2.1 eq?
(eq? object1 object2 )
Predicate that returns true iff object1 and object2 are effectively (§3.7) the same
object, even in the presence of mutation. The following universal rules constrain, but
do not uniquely determine, its behavior. Its behavior for objects of a particular type
described in this report may be further constrained in the description of the type.
1 The eq? predicate must be reflexive, symmetric, and transitive.
2 If the two objects (object1 and object2 ) are non-interchangeable in any way that
could affect the behavior of a Kernel program, using any features in the implementation or the report, but without first postulating that eq? distinguishes
them, eq? must return false. For example:
– If the two objects are observably not of the same type, eq? must return
false.
– If one of the objects is mutable and the other is immutable, eq? must
return false.
– If both objects are mutable, and mutating one will not necessarily cause
the same mutation to the other, eq? must return false.
– If the objects have different external representations (as do, for example,
the two boolean values), eq? must return false.
3 For any particular two objects, the result returned by eq? is always the same.
This applicative will be generalized to handle zero or more arguments in §6.5.1.
4.3 Equivalence up to mutation
4.3.1 equal?
(equal? object1 object2 )
Predicate that returns true iff object1 and object2 “look” the same as long as
nothing is mutated. This is a weaker predicate than eq? ; that is, equal? must
return true whenever eq? would return true. The following universal rules constrain,
but do not uniquely determine, the behavior of equal? . Its behavior for objects of
each type described in this report will be uniquely determined, modulo the behavior
of eq?, by further constraints as necessary in the description of the type.
1 The equal? predicate must be reflexive, symmetric, and transitive.
2 If eq? would return true, equal? must return true
If the two objects (object1 and object2 ) are non-interchangeable in any way that
could affect the behavior of a Kernel program that (a) performs no mutation
and (b) doesn’t use eq? (neither directly nor indirectly), then equal? must
return false. For example:
– If the two objects are observably not of the same type, equal? must return
false.
– If the objects have different external representations, equal? must return
false.
– If the objects are both numbers, and numerically equal, but have different
inexactness bounds (e.g., one is exact and the other isn’t; §12.2), equal?
must return false.
4 If equal? is not required to return false by the preceding rule, and this fact
can be determined with certainty by a (correct) Kernel program that (a) is
independent of the objects (they don’t refer to it and it only refers to them as
parameters), (b) examines the objects only passively (doesn’t use them or parts
of them as combiners in evaluation), (c) performs no mutation, and (d) always
terminates (provided the quantity of actual data within the runtime system is
finite), then equal? must return true. For example,
– Suppose variables x and y are set up by evaluating the following sequence
of expressions.
(define! x (list 1))
(define! y (list 1 1))
(append! x x)
(append! y y)
Then (equal? x y) would evaluate to #t.

5 For any particular two objects, the result returned by equal? is always the
same during a period of time over which no mutation occurs.
It is generally recommended that equal? return false in all cases where these rules
do not require it to return true.
This applicative will be generalized to handle zero or more arguments in §6.6.1.
Rationale:
The Kernel predicate equal?, unlike its Scheme counterpart, has to terminate for all
possible arguments (since it isn’t given dispensation to do otherwise). The set of cases in
which equal? is required, by Rule 3 above, to return false is formally undecidable; that
doesn’t interfere with termination of equal?, but does guarantee that the terminating
predicate returns false in some cases where it isn’t required to. (Terminating predicate eq?
must similarly return false in some unrequired cases; see §4.10.) The set of cases in which
equal? is required to return true, by Rule 4 above, might appear at first glance to be
undecidable but, in practice, it is unproblematically decidable. Because Rule 4 stipulates
that the determining program can only examine the objects passively, the determining
program cannot get bogged down in comparing the formally undecidable active behavior of
algorithms; degree of encapsulation doesn’t actually matter to this point, as, for example,
if the body of a compound combiner were made publicly visible so that it could be used
in the determination, different algorithms that do the same thing would be un-equal?
(hence un-eq? ) exactly because the combiners could by supposition be distinguished via
their syntactically distinct bodies.
4.4 Symbols
Two symbols are eq? (§4.2.1) iff they have the same external representation. Symbols
are immutable, and the symbol type is encapsulated.
The external representations of symbols are usually identifiers (§2.1). However,
symbols with other external representations may be created; see §13.1.1.
Rationale:
Symbols are useful as lookup keys for environments (§§3.2, 4.8), thus providing the
base case for nontrivial evaluation (§3.3), exactly because they are isomorphic to their
external representations.
4.4.1 symbol?
(symbol? . objects)
The primitive type predicate for type symbol.
4.5 Control
The inert data type is provided for use with control combiners. It consists of a
single immutable value, having external representation #inert. The inert type is
encapsulated.
Library features of the core Control module will be described in §§5.1, 5.6, and 6.9.
Rationale:
Some combiners are called for their side effects, not their results. In the C family of
languages, functions called for effect have return type void. The later Scheme reports
describe the results of for-effect procedures as ‘unspecified’, which is a politically necessary hedge because different Scheme implementations already in place follow a variety of
conventions concerning the return values of such procedures. Unfortunately, some Scheme
implementations allow for-effect procedures to return useful information, which creates a
temptation for programmers to write anti-portable code by using the result. Kernel avoids
this regrettable turn of events by explicitly requiring the result of each for-effect combiner
to be inert. Since the inert type is encapsulated, its one instance doesn’t carry any usable
information beyond its identity and type, which are isomorphic. (But see §3.7.)
Merely replacing “unspecified” with “inert” in the descriptions of standard combiners
would violate the spirit of G1b of §0.1.2, which calls for duplicability of built-in facilities
by the programmer. Hence the inert value must also have a full external representation
(as opposed to an output-only representation, §3.6), to facilitate explicit programmer
declaration of for-effect combiners.
4.5.1 inert?
(inert? . objects)
The primitive type predicate for type inert.
4.5.2 if
(if htesti hconsequenti halternativei)
The if operative first evaluates htesti in the dynamic environment (that is, the
environment in which the (if ...) combination is evaluated). If the result is not of
type boolean, an error is signaled. If the result is true, hconsequenti is then evaluated
in the dynamic environment as a tail context (§3.10). Otherwise, halternativei is
evaluated in the dynamic environment as a tail context.
Rationale:
On the exclusion of non-boolean results from conditional tests, see the rationale under
Partitioning of types, §3.5.
In R5RS Scheme, the halternativei operand to if is optional; and if it is omitted,
and htesti evaluates to false, the result is ‘unspecified’ — which would mean, in Kernel,
that the result would be inert. For consistency with the design purpose of #inert —
which is to convey no information— two-operand if ought to return #inert regardless
of whether hconsequenti is evaluated; but at that point, it becomes evident that the twoand three-operand operations are really separate, and by rights ought not to be lumped
into a single operative (which lumping doesn’t square well with the uniformity guideline,
G1 of §0.1.2, anyway); instead, if both operations are supported they should be given
different names. The two-operand form, though, is just a specialized shorthand; so both
clarity (thus accident-avoidance, G3 of §0.1.2) and simplicity are against its inclusion in
the language. (Similar issues arise for cond, §5.6.1.)
4.6 Pairs and lists
A pair is an object that refers to two other objects, called its car and cdr. The Kernel
data type pair is encapsulated.
The null data type consists of a single immutable value, called nil or the empty
list and having external representation (), with or without whitespace between the
parentheses. It is immutable, and the null type is encapsulated.
If a and d are external representations of respectively the car and cdr of a pair p,
then (a . d) is an external representation of p. If the cdr of p is nil, then (a) is also
an external representation of p. If the cdr of p is a pair p2, and (r) is an external
representation of p2, then (a r) is an external representation of p.
When a pair is output (as by write, §15.1.8), an external representation with the
fewest parentheses is used; in the case of a finite list, only one set of parentheses is
required beyond those used in representing the elements of the list. For example, an
object with external representation (1 . (2 . (3 . ()))) would be output using,
modulo whitespace, external representation (1 2 3).
Combiners for mutating pairs will be presented in a separate, optional Pair mutation module (§4.7). Otherwise, library features associated with these types will be
described in §§5.2, 5.4, 5.7, and 6.3.
This module assumes the Numbers module (§12). (See §§5.7, 6.3.)
Rationale:
The Numbers module is used to measure and index lists.
4.6.1 pair?
(pair? . objects)
The primitive type predicate for type pair.
4.6.2 null?
(null? . objects)
The primitive type predicate for type null.
4.6.3 cons
(cons object1 object2 )
A new pair object is constructed and returned, whose car and cdr referents are
respectively object1 and object2 .
Note that the general laws governing mutation (§3.8: constructed objects are mutable unless otherwise stated) and the general laws governing eq? (§4.2.1: independently mutable objects aren’t eq? ) conspire to guarantee that the objects returned
by two different calls to cons are not eq?
