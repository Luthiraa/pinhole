#!/bin/sh
set -eu
cd "$(dirname "$0")"
if [ "$(uname -s)" != Darwin ]; then
    echo 'Build the macOS package on a Mac.' >&2
    exit 1
fi
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}
cargo build --release --locked --target aarch64-apple-darwin --target x86_64-apple-darwin
mkdir -p bin
lipo -create target/aarch64-apple-darwin/release/pinhole target/x86_64-apple-darwin/release/pinhole -output bin/pinhole
codesign --force --sign "${PINHOLE_SIGN_IDENTITY:--}" --identifier com.luthiraa.pinhole bin/pinhole
codesign --verify --strict bin/pinhole
