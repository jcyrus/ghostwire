#!/bin/bash

# GhostWire Uninstaller
# "The server knows nothing. The terminal is everything."

set -e

BINARY_NAME="ghostwire"
INSTALL_DIR="/usr/local/bin"
LOCAL_BIN="$HOME/.local/bin"
CONFIG_DIR="$HOME/.config/ghostwire"

# Colors
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo -e "${YELLOW}👻 GhostWire Uninstaller${NC}"
echo ""

# Function to remove binary
remove_binary() {
    local install_path="$1"
    if [ -f "$install_path/$BINARY_NAME" ]; then
        echo -e "📦 Removing binary from: ${GREEN}$install_path${NC}"
        if [ -w "$install_path" ]; then
            rm -f "$install_path/$BINARY_NAME"
        else
            echo -e "${YELLOW}⚠️  Requires sudo permissions${NC}"
            sudo rm -f "$install_path/$BINARY_NAME"
        fi
        echo -e "${GREEN}✓${NC} Binary removed"
        return 0
    fi
    return 1
}

# Check for binary in common locations
FOUND=false

echo "🔍 Searching for GhostWire installation..."
echo ""

# Check /usr/local/bin
if remove_binary "$INSTALL_DIR"; then
    FOUND=true
fi

# Check ~/.local/bin
if remove_binary "$LOCAL_BIN"; then
    FOUND=true
fi

if [ "$FOUND" = false ]; then
    echo -e "${YELLOW}⚠️  GhostWire binary not found in standard locations${NC}"
    echo "   Checked:"
    echo "   - $INSTALL_DIR"
    echo "   - $LOCAL_BIN"
    echo ""
fi

# Ask about configuration files
if [ -d "$CONFIG_DIR" ]; then
    echo ""
    echo -e "${YELLOW}📁 Configuration directory found:${NC} $CONFIG_DIR"
    echo "   This includes:"
    echo "   - config.toml (your settings)"
    echo "   - security_audit.log (encryption logs)"
    echo "   - logs/ (application logs)"
    echo ""
    read -p "Do you want to remove configuration files? [y/N] " -n 1 -r
    echo ""
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo -e "🗑️  Removing configuration directory..."
        rm -rf "$CONFIG_DIR"
        echo -e "${GREEN}✓${NC} Configuration removed"
    else
        echo -e "${GREEN}✓${NC} Configuration preserved"
    fi
else
    echo -e "${GREEN}✓${NC} No configuration directory found"
fi

echo ""
if [ "$FOUND" = true ]; then
    echo -e "${GREEN}✅ GhostWire has been uninstalled${NC}"
    echo ""
    echo "To reinstall:"
    echo "  curl -sL https://ghostwire.fly.dev/install | bash"
else
    echo -e "${YELLOW}⚠️  No GhostWire installation found${NC}"
fi

echo ""
echo -e "${GREEN}👻 Transmission Ended${NC}"
