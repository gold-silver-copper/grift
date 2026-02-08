# Publishing Guide

This workspace uses centralized version management to make publishing to crates.io easier and less error-prone.

## Version Management

All crate versions and inter-crate dependencies are managed centrally in the root `Cargo.toml`:

- **Package version**: Defined once in `[workspace.package]`
- **Inter-crate dependencies**: Defined once in `[workspace.dependencies]`
- **Individual crates**: Inherit these using `workspace = true`

## How to Bump Version

To release a new version (e.g., from 1.3.1 to 1.3.2):

1. **Edit the root `Cargo.toml`** and update versions in TWO places:

```toml
[workspace.package]
version = "1.3.2"  # Update here (line 17)

[workspace.dependencies]
grift_arena = { version = "1.3.2", path = "crates/grift_arena" }  # Update here
grift_macros = { version = "1.3.2", path = "crates/grift_macros" }  # And here
grift_util = { version = "1.3.2", path = "crates/grift_util" }  # And here
grift_core = { version = "1.3.2", path = "crates/grift_core" }  # And here
grift_parser = { version = "1.3.2", path = "crates/grift_parser" }  # And here
grift_eval = { version = "1.3.2", path = "crates/grift_eval" }  # And here
grift_repl = { version = "1.3.2", path = "crates/grift_repl" }  # And here
grift_std = { version = "1.3.2", path = "crates/grift_std" }  # And here
grift_arena_embedded = { version = "1.3.2", path = "crates/grift_arena_embedded" }  # And here
```

2. **That's it!** Individual crate `Cargo.toml` files don't need any changes.

3. **Verify** everything works:
```bash
cargo build --workspace
cargo test --workspace
```

4. **Publish** using the publish script:
```bash
./publish.sh
```

## Benefits

### Before (Error-Prone)
- Had to update version in root `[workspace.package]`
- Had to update version in every crate dependency (20+ places)
- Missing one location would cause publishing failures like:
  ```
  error: failed to select a version for the requirement `grift_macros = "^1.3.1"`
  candidate versions found which didn't match: 1.3.0, 1.2.0, ...
  ```

### After (Single Source of Truth)
- Update version in only 10 places (all in one file)
- All crates automatically use the new version
- No more version mismatches during publishing
- Consistent versioning across the entire workspace

## Publishing Order

The `publish.sh` script publishes crates in dependency order:

1. `grift_arena` (no dependencies)
2. `grift_util` (no dependencies)
3. `grift_macros` (depends on grift_util)
4. `grift_core` (depends on grift_arena, grift_macros)
5. `grift_parser` (depends on grift_arena, grift_macros, grift_core)
6. `grift_std` (depends on grift_core)
7. `grift_eval` (depends on grift_arena, grift_parser)
8. `grift_arena_embedded` (depends on grift_arena, grift_parser, grift_eval)
9. `grift_repl` (depends on grift_arena, grift_parser, grift_eval, grift_arena_embedded, grift_std)
10. `grift` (depends on all above)

This order ensures that dependencies are available on crates.io before crates that depend on them are published.
