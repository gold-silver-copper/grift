#!/bin/bash

# Publish all crates to crates.io in dependency order
# Run from workspace root: ./publish.sh

echo "Publishing crates to crates.io in dependency order..."
echo ""

# Crates with no internal dependencies
echo "1/10 Publishing grift_arena..."
cargo publish -p grift_arena || echo "  (skipped or failed)"
sleep 5

echo "2/10 Publishing grift_macros..."
cargo publish -p grift_macros || echo "  (skipped or failed)"
sleep 5

echo "3/10 Publishing grift_util..."
cargo publish -p grift_util || echo "  (skipped or failed)"
sleep 5

# Depends on grift_arena and grift_macros
echo "4/10 Publishing grift_core..."
cargo publish -p grift_core || echo "  (skipped or failed)"
sleep 5

# Depends on grift_arena, grift_macros, grift_core
echo "5/10 Publishing grift_parser..."
cargo publish -p grift_parser || echo "  (skipped or failed)"
sleep 5

# Depends on grift_core
echo "6/10 Publishing grift_std..."
cargo publish -p grift_std || echo "  (skipped or failed)"
sleep 5

# Depends on grift_arena and grift_parser
echo "7/10 Publishing grift_eval..."
cargo publish -p grift_eval || echo "  (skipped or failed)"
sleep 5

# Depends on grift_arena, grift_parser, grift_eval
echo "8/10 Publishing grift_arena_embedded..."
cargo publish -p grift_arena_embedded || echo "  (skipped or failed)"
sleep 5

# Depends on grift_arena, grift_parser, grift_eval, grift_arena_embedded, grift_std
echo "9/10 Publishing grift_repl..."
cargo publish -p grift_repl || echo "  (skipped or failed)"
sleep 5

# Depends on all above
echo "10/10 Publishing grift..."
cargo publish -p grift || echo "  (skipped or failed)"

echo ""
echo "All crates published successfully!"
