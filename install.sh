#!/bin/bash
set -e

REPO="AnatolyRugalev/rpi-imager-tui"
ARCH=$(uname -m)

case $ARCH in
    x86_64)
        ASSET="rpi-imager-tui-linux-amd64"
        ;;
    aarch64|arm64)
        ASSET="rpi-imager-tui-linux-aarch64"
        ;;
    *)
        echo "Unsupported architecture: $ARCH"
        exit 1
        ;;
esac

# Determine install location
if [ "$(id -u)" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

TARGET="$INSTALL_DIR/rpi-imager-tui"
URL="https://github.com/$REPO/releases/latest/download/$ASSET"

echo "Downloading $ASSET..."
if command -v curl >/dev/null; then
    curl -L -o "$TARGET" "$URL"
elif command -v wget >/dev/null; then
    wget -O "$TARGET" "$URL"
else
    echo "Error: curl or wget is required"
    exit 1
fi

chmod +x "$TARGET"
echo "Installed to $TARGET"

if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo "Warning: $INSTALL_DIR is not in your PATH"
fi

echo "Starting rpi-imager-tui..."
"$TARGET"
