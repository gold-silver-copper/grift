Create a crate that is called simply "grift" that I can publish, it should reexport all functionality, be no_std no_alloc by default, should have a toggleable feature for the repl simply called std which is optional, be extremely minimalistic but reexport all features of every other crate in this workspace and have a minimal repo example. Make it installable through cargo, and have the binary name grift, which when ran launches a grift repl. It should have a very nice README

Refer to scheme spec md file and spec.html Do not use any external libraries. Consult and update Architecture documentation. Consult the spec.html

Split the lib.rs files into more manageable pieces for each crate, making them semantically grouped.

Continue with Scheme conformance implementation, whenever possible add the functions to stdlib.scm

Write tests and docs for all new features as you progress. Document progress in the scheme conformance MD file.