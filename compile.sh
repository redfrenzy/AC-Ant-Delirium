#!/bin/bash
//@ [WHAT] Build script for ac_ant_delirium geode.
//@ [WHY] To automate WASM-native compilation and ensure "Elite Industry" output.
//@ [WHERE] ac_ant_delirium/compile.sh

set -e

# 1. Compile to WASM (Raw target without wasm-bindgen overhead)
echo "🚀 Building Pure Rust WASM Geode..."
cargo build --target wasm32-unknown-unknown --release

# 2. Prepare output directory
mkdir -p ../../bin

# 3. Move and rename the artifact
CP_SOURCE="target/wasm32-unknown-unknown/release/ac_ant_delirium.wasm"
CP_TARGET="../../bin/ac_ant_delirium.wasm"

if [ -f "$CP_SOURCE" ]; then
    cp "$CP_SOURCE" "$CP_TARGET"
    echo "✨ Artifact ready: $CP_TARGET"
    
    # 4. Show artifact size
    SIZE=$(du -h "$CP_TARGET" | cut -f1)
    echo "📊 Final Size: $SIZE"
else
    echo "❌ Error: WASM artifact not found at $CP_SOURCE"
    exit 1
fi
