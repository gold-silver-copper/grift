#!/bin/bash

# Publish all crates to crates.io in dependency order
# Run from workspace root: ./publish.sh

set -e  # Exit on first error

echo "Publishing crates to crates.io in dependency order..."
echo ""

# Crates with no internal dependencies
echo "1/7 Publishing pwn_arena..."
cargo publish -p pwn_arena
sleep 200

echo "2/7 Publishing grift_macros..."
cargo publish -p grift_macros
sleep 200

# Depends on pwn_arena and grift_macros
echo "3/7 Publishing grift_parser..."
cargo publish -p grift_parser
sleep 200

# Depends on pwn_arena and grift_parser
echo "4/7 Publishing grift_eval..."
cargo publish -p grift_eval
sleep 200

# Depends on pwn_arena, grift_parser, grift_eval
echo "5/7 Publishing pwn_arena_embedded..."
cargo publish -p pwn_arena_embedded
sleep 200

# Depends on pwn_arena, grift_parser, grift_eval, pwn_arena_embedded
echo "6/7 Publishing grift_repl..."
cargo publish -p grift_repl
sleep 200

# Depends on all above
echo "7/7 Publishing grift..."
cargo publish -p grift

echo ""
echo "All crates published successfully!"
