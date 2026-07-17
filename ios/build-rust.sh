#!/usr/bin/env bash
#
# Build torg-ffi for iOS and assemble ios/TorgFFI.xcframework plus the Swift bindings.
# Run on macOS (needs Xcode command-line tools and the rustup iOS targets). See ios/README.md.
#
set -euo pipefail

# This script runs under a non-login shell, which doesn't source ~/.zprofile,
# so the rustup PATH entry (~/.cargo/bin) may be missing. Add it ourselves.
if ! command -v rustup >/dev/null 2>&1; then
  [ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
fi

cd "$(dirname "$0")/.."          # repo root
CRATE=torg-ffi
LIB=libtorg_ffi.a
GEN=ios/Generated
HEADERS=ios/Headers
BUILD=ios/build
XCF=ios/TorgFFI.xcframework

echo "==> ensuring iOS targets are installed"
rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios

echo "==> building the static lib for device + simulator"
cargo build -p "$CRATE" --release --target aarch64-apple-ios
cargo build -p "$CRATE" --release --target aarch64-apple-ios-sim
cargo build -p "$CRATE" --release --target x86_64-apple-ios

echo "==> fat simulator lib (arm64 + x86_64)"
mkdir -p "$BUILD"
lipo -create \
  "target/aarch64-apple-ios-sim/release/$LIB" \
  "target/x86_64-apple-ios/release/$LIB" \
  -output "$BUILD/libtorg_ffi-sim.a"

echo "==> generating Swift bindings"
# uniffi-bindgen reads the crate's exported metadata from a host build; the generated Swift is
# target-independent, so a plain host build is enough (and avoids linking issues).
cargo build -p "$CRATE"
# On macOS the host lib is .dylib, on Linux .so — only one exists, so `ls` of both
# always errors on the missing one. Swallow that so `set -euo pipefail` doesn't abort here.
HOSTLIB=$(ls target/debug/libtorg_ffi.dylib target/debug/libtorg_ffi.so 2>/dev/null | head -1 || true)
rm -rf "$GEN" "$HEADERS"
mkdir -p "$GEN" "$HEADERS"
cargo run -q -p "$CRATE" --bin uniffi-bindgen -- generate \
  --library "$HOSTLIB" --language swift --out-dir "$GEN"
# The xcframework expects the C header alongside a file literally named module.modulemap.
mv "$GEN/torg_ffiFFI.h" "$HEADERS/"
mv "$GEN/torg_ffiFFI.modulemap" "$HEADERS/module.modulemap"

echo "==> assembling $XCF"
rm -rf "$XCF"
xcodebuild -create-xcframework \
  -library "target/aarch64-apple-ios/release/$LIB" -headers "$HEADERS" \
  -library "$BUILD/libtorg_ffi-sim.a" -headers "$HEADERS" \
  -output "$XCF"

echo
echo "Done:"
echo "  $XCF"
echo "  $GEN/torg_ffi.swift   (add this to the app target)"
echo
echo "Next:  cd ios && xcodegen generate && open TorgSpike.xcodeproj"
echo "       (or create an iOS App target in Xcode and add TorgSpike/, Generated/torg_ffi.swift, and the xcframework)"
