#!/usr/bin/env bash
# Build Prioritizer.app in target/release/.
#   scripts/bundle-macos.sh            build only
#   scripts/bundle-macos.sh --install  also copy it to ~/Applications
set -euo pipefail
cd "$(dirname "$0")/.."
# rustup puts cargo here; not every shell has it on PATH.
command -v cargo >/dev/null || source "$HOME/.cargo/env"

cargo build --release

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
app=target/release/Prioritizer.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp target/release/prioritizer "$app/Contents/MacOS/Prioritizer"

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
    <key>CFBundleName</key><string>Prioritizer</string>
    <key>CFBundleDisplayName</key><string>Prioritizer</string>
    <key>CFBundleIdentifier</key><string>local.prioritizer</string>
    <key>CFBundleExecutable</key><string>Prioritizer</string>
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

if [[ "${1:-}" == "--install" ]]; then
    mkdir -p "$HOME/Applications"
    rm -rf "$HOME/Applications/Prioritizer.app"
    cp -R "$app" "$HOME/Applications/"
    echo "Installed to ~/Applications/Prioritizer.app"
fi
