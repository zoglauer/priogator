#!/usr/bin/env bash
# Build Priogator.app in target/release/.
#   scripts/bundle-macos.sh            build only
#   scripts/bundle-macos.sh --install  also copy it to ~/Applications
#   scripts/bundle-macos.sh --release  universal app (Apple Silicon + Intel)
#                                      packed into target/release/Priogator-<version>.dmg
# VERSION=1.2.3 overrides the version from Cargo.toml.
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup puts cargo here; not every shell has it on PATH.
command -v cargo >/dev/null || source "$HOME/.cargo/env"

mode=${1:-}
version=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)}
app=target/release/Priogator.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"

if [[ "$mode" == "--release" ]]; then
    targets=(aarch64-apple-darwin x86_64-apple-darwin)
    for t in "${targets[@]}"; do
        cargo build --release --target "$t"
    done
    lipo -create -output "$app/Contents/MacOS/Priogator" \
        target/aarch64-apple-darwin/release/priogator \
        target/x86_64-apple-darwin/release/priogator
else
    cargo build --release
    cp target/release/priogator "$app/Contents/MacOS/Priogator"
fi

# Icon: render all sizes macOS wants from the 1024 px PNG.
iconset=$(mktemp -d)/AppIcon.iconset
mkdir -p "$iconset"
for size in 16 32 128 256 512; do
    sips -z $size $size assets/icon.png --out "$iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z $double $double assets/icon.png --out "$iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$iconset" -o "$app/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$iconset")"

cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>Priogator</string>
    <key>CFBundleDisplayName</key><string>Priogator</string>
    <key>CFBundleIdentifier</key><string>com.andreaszoglauer.priogator</string>
    <key>CFBundleExecutable</key><string>Priogator</string>
    <key>CFBundleIconFile</key><string>AppIcon</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$version</string>
    <key>CFBundleVersion</key><string>$version</string>
    <key>LSMinimumSystemVersion</key><string>11.0</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

# Ad-hoc signature, required to run on Apple Silicon.
codesign --force --sign - "$app"
echo "Built $app"

if [[ "$mode" == "--release" ]]; then
    dmg=target/release/Priogator-$version.dmg
    staging=$(mktemp -d)
    cp -R "$app" "$staging/"
    ln -s /Applications "$staging/Applications"
    cp assets/dmg-read-me-first.txt "$staging/Read Me First.txt"
    rm -f "$dmg"
    hdiutil create -volname "Priogator $version" -srcfolder "$staging" \
        -ov -format UDZO "$dmg" >/dev/null
    rm -rf "$staging"
    echo "Built $dmg"
fi

if [[ "$mode" == "--install" ]]; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/Priogator.app"
    cp -R "$app" "$HOME/Applications/"
    echo "Installed to ~/Applications/Priogator.app"
fi
