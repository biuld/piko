#!/usr/bin/env bash
# Build a minimal macOS Piko.app with AppIcon.icns and piko-gui + piko-hostd.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
APP_NAME="Piko"
BUNDLE_ID="dev.piko.gui"
OUT_DIR="${1:-$ROOT/target/Piko.app}"
ICON_SRC="$ROOT/packages/gui/assets/app/AppIcon.icns"

echo "==> building piko-gui and piko-hostd (release)"
cargo build -p piko-gui -p piko-hostd --release --manifest-path "$ROOT/Cargo.toml"

GUI_BIN="$ROOT/target/release/piko-gui"
HOSTD_BIN="$ROOT/target/release/piko-hostd"

if [[ ! -f "$GUI_BIN" || ! -f "$HOSTD_BIN" ]]; then
  echo "error: expected release binaries under target/release/" >&2
  exit 1
fi
if [[ ! -f "$ICON_SRC" ]]; then
  echo "error: missing $ICON_SRC" >&2
  exit 1
fi

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/Contents/MacOS" "$OUT_DIR/Contents/Resources"

cp "$GUI_BIN" "$OUT_DIR/Contents/MacOS/piko-gui"
cp "$HOSTD_BIN" "$OUT_DIR/Contents/MacOS/piko-hostd"
cp "$ICON_SRC" "$OUT_DIR/Contents/Resources/AppIcon.icns"
chmod +x "$OUT_DIR/Contents/MacOS/piko-gui" "$OUT_DIR/Contents/MacOS/piko-hostd"

# Wrapper so the GUI finds sibling hostd when launched from the bundle.
cat > "$OUT_DIR/Contents/MacOS/Piko" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
export PIKO_HOSTD_PATH="${PIKO_HOSTD_PATH:-$DIR/piko-hostd}"
exec "$DIR/piko-gui" "$@"
EOF
chmod +x "$OUT_DIR/Contents/MacOS/Piko"

cat > "$OUT_DIR/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleDevelopmentRegion</key>
	<string>en</string>
	<key>CFBundleExecutable</key>
	<string>Piko</string>
	<key>CFBundleIconFile</key>
	<string>AppIcon</string>
	<key>CFBundleIdentifier</key>
	<string>${BUNDLE_ID}</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleName</key>
	<string>${APP_NAME}</string>
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleShortVersionString</key>
	<string>0.1.0</string>
	<key>CFBundleVersion</key>
	<string>0.1.0</string>
	<key>LSMinimumSystemVersion</key>
	<string>13.0</string>
	<key>NSHighResolutionCapable</key>
	<true/>
	<key>NSPrincipalClass</key>
	<string>NSApplication</string>
</dict>
</plist>
EOF

echo "built $OUT_DIR"
echo "open with: open \"$OUT_DIR\""
