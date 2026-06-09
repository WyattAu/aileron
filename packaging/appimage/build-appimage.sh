#!/usr/bin/env bash
# build-appimage.sh — Build an AppImage for Aileron
#
# Usage:
#   ./packaging/appimage/build-appimage.sh [--release-tag <tag>]
#
# Requirements:
#   - Rust toolchain (cargo)
#   - appimagetool (https://github.com/AppImage/AppImageKit)
#   - Standard build deps: webkit2gtk-4.1, gtk3, openssl, etc.
#
# The script:
#   1. Builds the release binary
#   2. Creates an AppDir/ hierarchy
#   3. Packages into an AppImage via appimagetool

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
APP_NAME="aileron"
APP_ID="com.github.WyattAu.aileron"
VERSION="${1:-$(grep '^version' "$REPO_ROOT/src/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')}"

BUILD_DIR="$REPO_ROOT/target/release"
APPDIR="$REPO_ROOT/target/appdir"

# ---------------------------------------------------------------------------
# Parse optional --release-tag
# ---------------------------------------------------------------------------
RELEASE_TAG=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-tag) RELEASE_TAG="$2"; shift 2 ;;
    *) shift ;;
  esac
done

if [[ -n "$RELEASE_TAG" ]]; then
  VERSION="${RELEASE_TAG#v}"
fi

echo "==> Building Aileron AppImage v${VERSION}"

# ---------------------------------------------------------------------------
# 1. Build release binary
# ---------------------------------------------------------------------------
echo "==> Building release binary..."
cd "$REPO_ROOT"
cargo build --release --locked

BINARY="$BUILD_DIR/$APP_NAME"
if [[ ! -f "$BINARY" ]]; then
  echo "ERROR: Release binary not found at $BINARY" >&2
  exit 1
fi

# ---------------------------------------------------------------------------
# 2. Create AppDir structure
# ---------------------------------------------------------------------------
echo "==> Creating AppDir structure..."
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$APPDIR/usr/share/man/man1"

# Copy binary
install -Dm755 "$BINARY" "$APPDIR/usr/bin/$APP_NAME"

# Copy desktop file
install -Dm644 "$SCRIPT_DIR/aileron.desktop" \
  "$APPDIR/usr/share/applications/$APP_ID.desktop"

# Copy icon (use the SVG from resources/)
if [[ -f "$REPO_ROOT/resources/aileron.svg" ]]; then
  install -Dm644 "$REPO_ROOT/resources/aileron.svg" \
    "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_NAME.svg"
fi

# Copy man page if present
if [[ -f "$REPO_ROOT/man/aileron.1" ]]; then
  install -Dm644 "$REPO_ROOT/man/aileron.1" \
    "$APPDIR/usr/share/man/man1/aileron.1"
fi

# Symlink binary at AppDir root (AppImage convention)
ln -sf "usr/bin/$APP_NAME" "$APPDIR/$APP_NAME"

# ---------------------------------------------------------------------------
# 3. Generate .DirIcon (symlink to icon for older AppImage parsers)
# ---------------------------------------------------------------------------
if [[ -f "$APPDIR/usr/share/icons/hicolor/scalable/apps/$APP_NAME.svg" ]]; then
  ln -sf "usr/share/icons/hicolor/scalable/apps/$APP_NAME.svg" "$APPDIR/.DirIcon"
fi

# ---------------------------------------------------------------------------
# 4. Package with appimagetool
# ---------------------------------------------------------------------------
if command -v appimagetool &>/dev/null; then
  echo "==> Packaging with appimagetool..."
  APPIMAGE_OUT="$REPO_ROOT/target/${APP_NAME}-${VERSION}-x86_64.AppImage"
  appimagetool "$APPDIR" "$APPIMAGE_OUT" --no-appstream
  echo "==> AppImage created: $APPIMAGE_OUT"
else
  echo "==> appimagetool not found — skipping AppImage packaging."
  echo "    Install: https://github.com/AppImage/AppImageKit"
  echo "    Then run: appimagetool $APPDIR $REPO_ROOT/target/${APP_NAME}-${VERSION}-x86_64.AppImage --no-appstream"
  echo ""
  echo "==> AppDir structure created at: $APPDIR"
  echo "    You can test by running: $APPDIR/usr/bin/$APP_NAME"
fi

echo "==> Done."
