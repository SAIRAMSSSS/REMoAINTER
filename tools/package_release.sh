#!/usr/bin/env bash
set -euo pipefail

# REMoAINTER Release Packaging Script
# Author: Sairam (@SAIRAMSSSS)
# Version: v0.2.0

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

VERSION="0.2.0"
PACKAGE_NAME="REMoAINTER-v${VERSION}-linux-x86_64"
DIST_DIR="$REPO_ROOT/dist"
STAGING_DIR="$DIST_DIR/$PACKAGE_NAME"
ARCHIVE_PATH="$DIST_DIR/${PACKAGE_NAME}.tar.gz"

echo "============================================================"
echo " Packaging REMoAINTER v${VERSION} Release Archive"
echo "============================================================"

# 1. Build release binary
echo "==> Building optimized release binary with Cargo..."
cargo build --release --manifest-path "$REPO_ROOT/rust/Cargo.toml"

BINARY_SRC="$REPO_ROOT/rust/target/release/remoainter"
if [ ! -f "$BINARY_SRC" ]; then
    echo "ERROR: Compiled binary not found at $BINARY_SRC" >&2
    exit 1
fi

# 2. Prepare clean staging directory
echo "==> Preparing staging directory: $STAGING_DIR"
rm -rf "$DIST_DIR"
mkdir -p "$STAGING_DIR/bin"

# 3. Copy binary and strip debug symbols
echo "==> Installing release binary..."
cp "$BINARY_SRC" "$STAGING_DIR/bin/remoainter"
if command -v strip >/dev/null 2>&1; then
    echo "==> Stripping debug symbols..."
    strip "$STAGING_DIR/bin/remoainter"
fi
ln -sf bin/remoainter "$STAGING_DIR/remoainter"

# 4. Copy launcher and set permissions
echo "==> Copying launcher and documentation..."
cp "$REPO_ROOT/run.sh" "$STAGING_DIR/run.sh"
chmod +x "$STAGING_DIR/run.sh" "$STAGING_DIR/bin/remoainter"

# 5. Copy assets
echo "==> Bundling core PBR assets, environments, and test models..."
cp -r "$REPO_ROOT/assets" "$STAGING_DIR/assets"
# Remove any external or dangling symlinks to ensure a 100% self-contained bundle
find "$STAGING_DIR/assets" -type l -delete

# 6. Copy docs & license
cp "$REPO_ROOT/README.md" "$STAGING_DIR/README.md"
if [ -f "$REPO_ROOT/LICENSE" ]; then
    cp "$REPO_ROOT/LICENSE" "$STAGING_DIR/LICENSE"
fi
mkdir -p "$STAGING_DIR/docs"
cp -r "$REPO_ROOT/docs/"* "$STAGING_DIR/docs/"

# 7. Create tar.gz archive
echo "==> Creating compressed tarball: $ARCHIVE_PATH"
tar -czf "$ARCHIVE_PATH" -C "$DIST_DIR" "$PACKAGE_NAME"

# 8. Compute SHA-256 checksums
echo "==> Generating SHA256 checksum..."
cd "$DIST_DIR"
sha256sum "${PACKAGE_NAME}.tar.gz" > SHA256SUMS
cat SHA256SUMS

# 9. Verify size constraints (< 500 MB)
ARCHIVE_SIZE_BYTES=$(stat -c%s "$ARCHIVE_PATH" 2>/dev/null || stat -f%z "$ARCHIVE_PATH")
ARCHIVE_SIZE_MB=$(( ARCHIVE_SIZE_BYTES / 1024 / 1024 ))
echo "============================================================"
echo " Release Package Created Successfully!"
echo " Package:   $ARCHIVE_PATH"
echo " Size:      ${ARCHIVE_SIZE_MB} MB (${ARCHIVE_SIZE_BYTES} bytes)"
echo " Checksum:  $(cat SHA256SUMS)"
echo "============================================================"

if [ "$ARCHIVE_SIZE_MB" -ge 500 ]; then
    echo "ERROR: Archive exceeds size limit of 500 MB!" >&2
    exit 1
fi
