#!/bin/sh
set -eu
cd "$(dirname "$0")"
if [ "$(uname -s)" != Darwin ]; then
    echo 'Build the macOS package on a Mac.' >&2
    exit 1
fi
export MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-11.0}
cargo build --release --locked --target aarch64-apple-darwin --target x86_64-apple-darwin
mkdir -p Pinhole.app/Contents/MacOS
lipo -create target/aarch64-apple-darwin/release/pinhole target/x86_64-apple-darwin/release/pinhole -output Pinhole.app/Contents/MacOS/pinhole
pinhole_version=$(node -p "require('./package.json').version")
cat > Pinhole.app/Contents/Info.plist <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.luthiraa.pinhole</string>
<key>CFBundleName</key><string>Pinhole</string>
<key>CFBundleDisplayName</key><string>Pinhole</string>
<key>CFBundleExecutable</key><string>pinhole</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleVersion</key><string>$pinhole_version</string>
<key>CFBundleShortVersionString</key><string>$pinhole_version</string>
<key>LSMinimumSystemVersion</key><string>11.0</string>
<key>LSUIElement</key><true/>
</dict></plist>
PLIST
codesign --force --sign "${PINHOLE_SIGN_IDENTITY:--}" Pinhole.app
codesign --verify --strict Pinhole.app
