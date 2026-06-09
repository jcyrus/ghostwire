#!/bin/bash

# ZeroDrop Installer
# "The server knows nothing. The terminal is everything."

set -e

REPO="jcyrus/zerodrop"
BINARY_NAME="zerodrop"
INSTALL_DIR="/usr/local/bin"
LOCAL_BIN="$HOME/.local/bin"

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${GREEN}👻 Initializing ZeroDrop Sequence...${NC}"

# Detect OS
OS="$(uname -s)"
ARCH="$(uname -m)"

PLATFORM=""
if [ "$OS" == "Linux" ]; then
    PLATFORM="linux"
elif [ "$OS" == "Darwin" ]; then
    PLATFORM="darwin"
else
    echo -e "${RED}❌ Unsupported OS: $OS${NC}"
    exit 1
fi

# Detect Architecture
if [ "$ARCH" == "x86_64" ]; then
    ARCH="amd64"
elif [ "$ARCH" == "aarch64" ] || [ "$ARCH" == "arm64" ]; then
    ARCH="arm64"
else
    echo -e "${RED}❌ Unsupported Architecture: $ARCH${NC}"
    exit 1
fi

ASSET_NAME="zerodrop-${PLATFORM}-${ARCH}"
if [ "$OS" == "Windows" ]; then
    ASSET_NAME="${ASSET_NAME}.exe"
fi

echo -e "Detected: ${GREEN}${OS} ${ARCH}${NC}"

# Get Latest Release URL
echo "Fetching latest frequency..."
LATEST_URL=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep "browser_download_url" | grep "${ASSET_NAME}" | cut -d '"' -f 4)

if [ -z "$LATEST_URL" ]; then
    echo -e "${RED}❌ Could not find release asset for ${ASSET_NAME}${NC}"
    echo "Please check https://github.com/${REPO}/releases"
    exit 1
fi

# Download
echo "Downloading payload..."
curl -L -o "$BINARY_NAME" "$LATEST_URL" --progress-bar

# Make Executable
chmod +x "$BINARY_NAME"

# Install
echo "Installing to system..."

if [ -w "$INSTALL_DIR" ]; then
    mv "$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    FINAL_PATH="$INSTALL_DIR/$BINARY_NAME"
elif command -v sudo >/dev/null 2>&1; then
    echo "Sudo required to move to $INSTALL_DIR"
    sudo mv "$BINARY_NAME" "$INSTALL_DIR/$BINARY_NAME"
    FINAL_PATH="$INSTALL_DIR/$BINARY_NAME"
else
    echo "Cannot write to $INSTALL_DIR and sudo not available."
    echo "Installing to $LOCAL_BIN instead..."
    mkdir -p "$LOCAL_BIN"
    mv "$BINARY_NAME" "$LOCAL_BIN/$BINARY_NAME"
    FINAL_PATH="$LOCAL_BIN/$BINARY_NAME"
    
    # Check PATH
    if [[ ":$PATH:" != *":$LOCAL_BIN:"* ]]; then
        echo -e "${RED}⚠️  Warning: $LOCAL_BIN is not in your PATH${NC}"
        echo "Add this to your shell config: export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
fi

echo -e "${GREEN}✅ ZeroDrop Installed Successfully!${NC}"
echo -e "Run with: ${GREEN}zerodrop${NC}"
