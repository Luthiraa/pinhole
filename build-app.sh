#!/bin/sh
set -eu
cd "$(dirname "$0")"
cargo build --release --locked
app="dist/Screenlink.app"
mkdir -p "$app/Contents/MacOS"
cp target/release/screenlink "$app/Contents/MacOS/screenlink"
sdk=/Library/Developer/CommandLineTools/SDKs/MacOSX15.4.sdk
if [ ! -d "$sdk" ]; then sdk=$(xcrun --show-sdk-path); fi
swiftc -sdk "$sdk" -module-cache-path "$PWD/target/swift-modules" -O src/dialog.swift -o "$app/Contents/MacOS/screenlink-dialog"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>local.screenlink.app</string>
  <key>CFBundleName</key><string>Screenlink</string>
  <key>CFBundleExecutable</key><string>screenlink</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
</dict></plist>
PLIST
identity=${SCREENLINK_SIGN_IDENTITY:--}
codesign --force --deep --sign "$identity" "$app"
codesign --verify --deep --strict "$app"
printf 'Created %s\n' "$app"
