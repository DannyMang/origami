#!/usr/bin/env bash
set -euo pipefail

BUNDLE_ID=com.origami.app

cd "$(dirname "$0")"
[ -z "${SIGN_ID:-}" ] || : "${NOTARY_PROFILE:?set NOTARY_PROFILE to a notarytool keychain profile}"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' origami-app/Cargo.toml | head -1)
app=dist/Origami.app
bin=$app/Contents/MacOS

cargo build --release --manifest-path origami-app/Cargo.toml
cargo build --release --manifest-path origami-engine/Cargo.toml

rm -rf dist
mkdir -p "$bin"
cp "${CARGO_TARGET_DIR:-origami-app/target}/release/origami-app" "$bin/"
cp "${CARGO_TARGET_DIR:-origami-engine/target}/release/origami" "$bin/"
if [ -n "${FFMPEG_DIR:-}" ]; then
  cp "$FFMPEG_DIR/ffmpeg" "$FFMPEG_DIR/ffprobe" "$bin/"
fi

cat > "$app/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>Origami</string>
  <key>CFBundleDisplayName</key>
  <string>Origami</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleExecutable</key>
  <string>origami-app</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>Origami records voiceovers from your microphone.</string>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Origami script</string>
      <key>CFBundleTypeRole</key>
      <string>Editor</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>com.netscape.javascript-source</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
EOF
plutil -lint "$app/Contents/Info.plist"

if [ -n "${SIGN_ID:-}" ]; then
  for tool in origami ffmpeg ffprobe; do
    [ ! -e "$bin/$tool" ] || codesign --force --timestamp --options runtime --sign "$SIGN_ID" "$bin/$tool"
  done
  codesign --force --timestamp --options runtime --entitlements origami.entitlements --sign "$SIGN_ID" "$app"
fi

hdiutil create -volname Origami -srcfolder "$app" -format UDZO dist/Origami.dmg

if [ -n "${SIGN_ID:-}" ]; then
  codesign --force --timestamp --sign "$SIGN_ID" dist/Origami.dmg
  xcrun notarytool submit dist/Origami.dmg --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple dist/Origami.dmg
fi
