#!/bin/bash
# Build script for hwc compiler

set -e

echo "🔨 Building hwc Compiler..."

# Build in release mode
cargo build --release

echo "✅ Build complete!"
echo ""
echo "Executable: target/release/hwc"
echo ""
echo "Try:"
echo "  ./target/release/hwc --help"
