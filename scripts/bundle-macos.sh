#!/usr/bin/env bash
# Builds GetCraft.app from the cargo binary.
#   scripts/bundle-macos.sh [debug|release]   (default: release, universal when both targets exist)
# Signing and notarization happen in CI (see .github/workflows/release.yml).
set -euo pipefail
cd "$(dirname "$0")/.."

PROFILE="${1:-release}"
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c "import json,sys; print(next(p['version'] for p in json.load(sys.stdin)['packages'] if p['name']=='getcraft'))")
BUNDLE_ID="${GETCRAFT_BUNDLE_ID:-net.brnbch.getcraft}"

if [[ "$PROFILE" == "release" ]]; then
  cargo build --release -p getcraft --target aarch64-apple-darwin
  cargo build --release -p getcraft --target x86_64-apple-darwin
  BIN=target/getcraft-universal
  lipo -create -output "$BIN" \
    target/aarch64-apple-darwin/release/getcraft target/x86_64-apple-darwin/release/getcraft
else
  cargo build -p getcraft
  BIN=target/debug/getcraft
fi

APP=target/bundle/GetCraft.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/GetCraft"
if [[ -f assets/getcraft.icns ]]; then cp assets/getcraft.icns "$APP/Contents/Resources/GetCraft.icns"; fi
# License texts and notices travel with every copy (required for the bundled app icons).
cp LICENSE-MIT LICENSE-APACHE NOTICE ATTRIBUTION.md assets/LICENSE-icon.txt "$APP/Contents/Resources/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>GetCraft</string>
  <key>CFBundleDisplayName</key><string>GetCraft</string>
  <key>CFBundleIdentifier</key><string>${BUNDLE_ID}</string>
  <key>CFBundleExecutable</key><string>GetCraft</string>
  <key>CFBundleIconFile</key><string>GetCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
</dict>
</plist>
PLIST
echo "$APP"
