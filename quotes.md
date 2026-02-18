  Vau & wrap actually provide a more fundamental abstraction than lambda does, in the sense that they can be used to build lambdas and some other extra stuff. So, if the Lisp interpreter is like software's Maxwell equations, I see the vau-based interpreter as a bit like software's Schroedinger equation, with the special built-in global environment functions (like the definition of vau itself, or the define function, or the if function) filling the roles of the different fundamental particles that obey it, which can be composed to form all matter. Maybe vau expressions are like complex-valued quantum wave functions, while wraps are like the resulting probability distributions https://gliese1337.blogspot.com/2012/04/schrodingers-equation-of-software.html

Axis of Eval

leaving only the 25 special forms defined by Common Lisp. (Scheme manages with seven or eight, nyah nyah.)

May John's Environment in Heaven have a sound beta rule reduction and he get to meet his vau the ultimate creator.

Thank you for disagreeing. The relative complexity of macros vs fexpr is interesting and your reply uncovered unspoken assumptions in my thinking.

First class environments being another thing sacrificed on the alter of performance decades ago.

Every single time this argument comes up, the hygiene zealots wind up falling back on arguments about what other programmers can't possibly reason about (in spite of historic evidence that they can) and about what power other programmer's don't need (in spite of historic evidence that they do) and Scheme continues its slow death spiral into a bondage and discipline language of the soft that could only be loved by a "fascist with a read only mind".

All this is not the dream of a "fascist with a read-only mind" or even the result of a bondage & discipline fetish. I seek simplicity and freedom through liberating constraints. They don't even need to be 'in-your-face' constraints - I favor an 'incentives' based approach: working with a lower-layer where feasible (aka 'Principle of Least Expression') is not enforced, but is encouraged by improved modularity, reusability, security, safety, and performance.
