#!/usr/bin/env bash
set -euo pipefail

BUNDLE_ID=video.origami.app

cd "$(dirname "$0")"
if [ -f .env ]; then
  set -a
  . ./.env
  set +a
fi
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=~ --remap-path-prefix=$PWD=origami"
[ -z "${SIGN_ID:-}" ] || : "${NOTARY_PROFILE:?set NOTARY_PROFILE to a notarytool keychain profile}"
version=$(sed -n 's/^version = "\(.*\)"$/\1/p' origami-app/Cargo.toml | head -1)
app=dist/Origami.app
bin=$app/Contents/MacOS

./ffmpeg.sh
sources=()
while IFS= read -r built; do
  source=${built%%:*}
  grep -qF "${built#*: }" THIRD-PARTY.md && grep -qF "$source.tar.xz" THIRD-PARTY.md || { echo "THIRD-PARTY.md does not name $source and how it was built" >&2; exit 1; }
  sources+=("ffmpeg/$source.tar.xz")
done < ffmpeg/configure.txt
cargo build --release --manifest-path origami-app/Cargo.toml
cargo build --release --manifest-path origami-engine/Cargo.toml

rm -rf dist
mkdir -p "$bin" "$app/Contents/Resources"
cp "${CARGO_TARGET_DIR:-origami-app/target}/release/origami-app" "$bin/"
cp "${CARGO_TARGET_DIR:-origami-engine/target}/release/origami" "$bin/"
cp ffmpeg/ffmpeg ffmpeg/ffprobe "$bin/"
[ "$(strings -a "$bin"/* | grep -cF "$HOME/")" = 0 ] || { echo "binaries contain $HOME" >&2; exit 1; }
[ -z "$(otool -L "$bin/ffmpeg" "$bin/ffprobe" | grep '^	' | grep -vE '^	(/usr/lib|/System/Library)/')" ] || { echo "ffmpeg links outside macOS" >&2; exit 1; }
cp LICENSE THIRD-PARTY.md THIRD-PARTY-LICENSES.txt Origami.icns "$app/Contents/Resources/"

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
  <key>CFBundleIconFile</key>
  <string>Origami</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleShortVersionString</key>
  <string>$version</string>
  <key>CFBundleVersion</key>
  <string>$version</string>
  <key>LSMinimumSystemVersion</key>
  <string>15.0</string>
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

sign=(codesign --force --options runtime --sign "${SIGN_ID:--}")
[ -z "${SIGN_ID:-}" ] || sign+=(--timestamp)
for tool in origami ffmpeg ffprobe; do
  "${sign[@]}" --identifier "$BUNDLE_ID.$tool" "$bin/$tool"
done
"${sign[@]}" --entitlements origami.entitlements "$app"
codesign --verify --deep --strict "$app"

mkdir dist/dmg
ditto "$app" dist/dmg/Origami.app
ln -s /Applications dist/dmg/Applications
hdiutil create -volname Origami -srcfolder dist/dmg -format UDZO dist/Origami.dmg
rm -rf dist/dmg
cp "${sources[@]}" dist/

if [ -n "${SIGN_ID:-}" ]; then
  codesign --force --timestamp --sign "$SIGN_ID" dist/Origami.dmg
  xcrun notarytool submit dist/Origami.dmg --keychain-profile "$NOTARY_PROFILE" --wait
  xcrun stapler staple dist/Origami.dmg
fi
