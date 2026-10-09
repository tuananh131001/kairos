#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD="$ROOT/build"
APP_DIR="$ROOT/apps/macos"
APP="$BUILD/Kairos.app"
CONFIGURATION="${CONFIGURATION:-Release}"
INSTALL=0

for arg in "$@"; do
  case "$arg" in
    --install) INSTALL=1 ;;
    *) echo "usage: $0 [--install]" >&2; exit 64 ;;
  esac
done

export MACOSX_DEPLOYMENT_TARGET=14.0
mkdir -p "$BUILD"

echo "==> Building universal kairosd"
rustup target add aarch64-apple-darwin x86_64-apple-darwin >/dev/null 2>&1 || true
cargo build --manifest-path "$ROOT/Cargo.toml" --release -p kairosd --target aarch64-apple-darwin
cargo build --manifest-path "$ROOT/Cargo.toml" --release -p kairosd --target x86_64-apple-darwin
lipo -create -output "$BUILD/kairosd" \
  "$ROOT/target/aarch64-apple-darwin/release/kairosd" \
  "$ROOT/target/x86_64-apple-darwin/release/kairosd"
lipo -info "$BUILD/kairosd"

has_xcode() {
  xcodebuild -version >/dev/null 2>&1
}

pick_identity() {
  if [ -n "${KAIROS_SIGN_IDENTITY:-}" ]; then
    echo "$KAIROS_SIGN_IDENTITY"
    return
  fi
  local found
  found="$(security find-identity -v -p codesigning 2>/dev/null | awk -F'"' '/Apple Development/ {print $2; exit}')"
  if [ -n "$found" ]; then
    echo "$found"
  else
    echo "-"
  fi
}

IDENTITY="$(pick_identity)"
rm -rf "$APP"

if has_xcode; then
  echo "==> Building Kairos.app with xcodebuild"
  command -v xcodegen >/dev/null || { echo "xcodegen is required (brew install xcodegen)" >&2; exit 1; }
  (cd "$APP_DIR" && xcodegen generate --quiet)
  SIGN_ARGS=()
  if [ "$IDENTITY" = "-" ]; then
    SIGN_ARGS=(CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY=- DEVELOPMENT_TEAM=)
  fi
  xcodebuild -project "$APP_DIR/Kairos.xcodeproj" -scheme Kairos -configuration "$CONFIGURATION" \
    -derivedDataPath "$BUILD/DerivedData" ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO \
    "${SIGN_ARGS[@]}" build | tail -n 5
  ditto "$BUILD/DerivedData/Build/Products/$CONFIGURATION/Kairos.app" "$APP"
else
  echo "==> Xcode not found; building Kairos.app with SwiftPM (Command Line Tools)"
  SLICES=()
  for arch in arm64 x86_64; do
    SWIFT_ARGS=(-c release --arch "$arch" --product Kairos --scratch-path "$BUILD/swiftpm-$arch")
    (cd "$APP_DIR" && swift build "${SWIFT_ARGS[@]}")
    SLICES+=("$(cd "$APP_DIR" && swift build "${SWIFT_ARGS[@]}" --show-bin-path)/Kairos")
  done
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
  lipo -create -output "$APP/Contents/MacOS/Kairos" "${SLICES[@]}"
  sed -e 's/$(EXECUTABLE_NAME)/Kairos/' \
      -e 's/$(PRODUCT_BUNDLE_IDENTIFIER)/com.kairos.app/' \
      -e 's/$(MARKETING_VERSION)/0.1.0/' \
      -e 's/$(CURRENT_PROJECT_VERSION)/1/' \
      -e 's/$(MACOSX_DEPLOYMENT_TARGET)/14.0/' \
      "$APP_DIR/Kairos/Resources/Info.plist" > "$APP/Contents/Info.plist"
  cp "$APP_DIR/Kairos/Resources/AppIcon.icns" "$APP/Contents/Resources/"
  printf 'APPL????' > "$APP/Contents/PkgInfo"
fi

echo "==> Embedding kairosd and LaunchAgent"
cp "$BUILD/kairosd" "$APP/Contents/MacOS/kairosd"
mkdir -p "$APP/Contents/Library/LaunchAgents"
cp "$APP_DIR/Kairos/Resources/com.kairos.daemon.plist" "$APP/Contents/Library/LaunchAgents/"
plutil -lint "$APP/Contents/Info.plist" "$APP/Contents/Library/LaunchAgents/com.kairos.daemon.plist"

echo "==> Signing with identity: $IDENTITY"
if [ "$IDENTITY" = "-" ]; then
  echo "warning: ad-hoc signing; the Screen Recording grant will not survive rebuilds. Set KAIROS_SIGN_IDENTITY or install an Apple Development certificate." >&2
fi
codesign --force --sign "$IDENTITY" --identifier com.kairos.daemon --timestamp=none "$APP/Contents/MacOS/kairosd"
codesign --force --sign "$IDENTITY" --identifier com.kairos.app --timestamp=none "$APP"
codesign --verify --deep --strict "$APP"
echo "Built $APP"

if [ "$INSTALL" = 1 ]; then
  echo "==> Installing to /Applications"
  rm -rf /Applications/Kairos.app
  ditto "$APP" /Applications/Kairos.app
  echo "Installed /Applications/Kairos.app"
fi
