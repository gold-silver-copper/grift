#!/usr/bin/env bash
# Builds the chip example and runs it on QEMU's mps2-an385 board (a Cortex-M3, which runs the
# Cortex-M0 build), printing over semihosting. Needs qemu-system-arm on PATH.
set -euo pipefail
cd "$(dirname "$0")/../examples/chip"
cargo build --release
target=${CARGO_TARGET_DIR:-target}
exec qemu-system-arm -machine mps2-an385 -cpu cortex-m3 -nographic \
    -semihosting-config enable=on,target=native \
    -kernel "$target/thumbv6m-none-eabi/release/grift-chip"
