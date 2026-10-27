#!/usr/bin/env bash
# Arma paint1.app (binario universal arm64 + x86_64) y el .dmg en target/paquetes/.
# Uso: packaging/macos/bundle.sh VERSIÓN   (en macOS, tras compilar ambos targets en release)
set -euo pipefail
version="$1"
cd "$(dirname "$0")/../.."

app=target/macos/paint1.app
rm -rf target/macos
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" target/paquetes

lipo -create -output "$app/Contents/MacOS/paint1" \
    target/aarch64-apple-darwin/release/paint1 \
    target/x86_64-apple-darwin/release/paint1
sed "s/@VERSION@/$version/g" packaging/macos/Info.plist > "$app/Contents/Info.plist"

iconset=target/macos/paint1.iconset
mkdir -p "$iconset"
for s in 16 32 128 256 512; do
    cp "assets/paint1-$s.png" "$iconset/icon_${s}x${s}.png"
done
cp assets/paint1-32.png "$iconset/icon_16x16@2x.png"
cp assets/paint1-64.png "$iconset/icon_32x32@2x.png"
cp assets/paint1-256.png "$iconset/icon_128x128@2x.png"
cp assets/paint1-512.png "$iconset/icon_256x256@2x.png"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/paint1.icns"

# Sin cuenta de desarrollador: firma ad hoc (en Apple Silicon todo binario debe estar firmado).
codesign --force --deep --sign - "$app"

dmg=target/macos/dmg
mkdir -p "$dmg"
cp -R "$app" "$dmg/"
ln -s /Applications "$dmg/Applications"
hdiutil create -volname "paint1 $version" -srcfolder "$dmg" -ov -format UDZO \
    "target/paquetes/paint1-$version-macos-universal.dmg"
