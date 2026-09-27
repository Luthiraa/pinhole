#!/bin/sh
set -eu
cd "$(dirname "$0")"
cargo build --release --locked
app="dist/Pinhole.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/pinhole "$app/Contents/MacOS/pinhole"
cp assets/Pinhole.icns "$app/Contents/Resources/Pinhole.icns"
sdk=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk
if [ ! -d "$sdk" ]; then sdk=$(xcrun --show-sdk-path); fi
swiftc -sdk "$sdk" -module-cache-path "$PWD/target/swift-modules" -O src/dialog.swift -o "$app/Contents/MacOS/pinhole-dialog"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>local.pinhole.app</string>
  <key>CFBundleName</key><string>Pinhole</string>
  <key>CFBundleDisplayName</key><string>Pinhole</string>
  <key>CFBundleIconFile</key><string>Pinhole.icns</string>
  <key>CFBundleExecutable</key><string>pinhole</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
</dict></plist>
PLIST
identity=${PINHOLE_SIGN_IDENTITY:--}
codesign --force --deep --sign "$identity" "$app"
codesign --verify --deep --strict "$app"
printf 'Created %s\n' "$app"
