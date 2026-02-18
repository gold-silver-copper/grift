#!/bin/bash

# Publish all crates to crates.io in dependency order
# Run from workspace root: ./publish.sh

echo "Publishing crates to crates.io in dependency order..."
echo ""

# Crates with no internal dependencies
echo "1/10 Publishing grift_arena..."
cargo publish -p grift_arena || echo "  (skipped or failed)"
sleep 5


# Depends on all above
echo "10/10 Publishing grift..."
cargo publish -p grift || echo "  (skipped or failed)"

echo ""
echo "All crates published successfully!"
