#!/usr/bin/env bash
# ==============================================================================
# CLI-NER - Build & Local Install Script
# Repository: https://github.com/fabrizioriccardo73/cli-ner
# ==============================================================================
# Compiles cli-ner in release mode using Cargo and installs the binary to your PATH.
#
# Usage:
#   ./build-and-install.sh [options]
#
# Options:
#   -d, --dir <PATH>   Custom installation directory (default: auto-detected)
#   -h, --help         Show this help message
# ==============================================================================

set -euo pipefail

# Visual formatting
BOLD="\033[1m"
GREEN="\033[0;32m"
BLUE="\033[0;34m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
RESET="\033[0m"

BINARY_NAME="cli-ner"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TARGET_DIR=""

# Parse options
while [[ $# -gt 0 ]]; do
    case "$1" in
        -d|--dir)
            TARGET_DIR="$2"
            shift 2
            ;;
        -h|--help)
            echo -e "${BOLD}CLI-NER Build & Local Installer${RESET}"
            echo -e "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  -d, --dir <PATH>   Specify custom installation directory"
            echo "  -h, --help         Show this help message"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${RESET}"
            echo "Run '$0 --help' for usage."
            exit 1
            ;;
    esac
done

cd "$SCRIPT_DIR"

echo -e "${BOLD}${BLUE}"
echo "   ___ _    ___     _  _ ___ ___ "
echo "  / __| |  |_ _|___| \| | __| _ \\"
echo " | (__| |__ | |____| .\` | _||   /"
echo "  \___|____|___|   |_|\_|___|_|_\\"
echo -e "${RESET}"
echo -e "${BOLD}CLI-NER: Local Release Build & Installation${RESET}\n"

# 1. Check prerequisites
echo -e "${BLUE}==>${RESET} Checking prerequisites..."
if ! command -v cargo &>/dev/null; then
    echo -e "${RED}Error: Cargo is not installed or not in PATH.${RESET}"
    echo -e "Please install Rust and Cargo via rustup: ${BOLD}https://rustup.rs${RESET}"
    echo -e "Run: ${BOLD}curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh${RESET}"
    exit 1
fi

if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Cargo.toml not found in $SCRIPT_DIR.${RESET}"
    exit 1
fi

RUSTC_VERSION="$(rustc --version 2>/dev/null || echo 'unknown')"
echo -e "    Found: ${GREEN}${RUSTC_VERSION}${RESET}"

# 2. Build release binary
echo -e "\n${BLUE}==>${RESET} Compiling ${BOLD}${BINARY_NAME}${RESET} in release mode (optimized)..."
cargo build --release

RELEASE_BIN="${SCRIPT_DIR}/target/release/${BINARY_NAME}"
if [ ! -f "$RELEASE_BIN" ]; then
    echo -e "${RED}Error: Built binary not found at ${RELEASE_BIN}.${RESET}"
    exit 1
fi

# 3. Determine installation destination
if [ -n "$TARGET_DIR" ]; then
    INSTALL_DIR="$TARGET_DIR"
    mkdir -p "$INSTALL_DIR"
else
    if [ -d "$HOME/.cargo/bin" ] && [[ ":$PATH:" == *":$HOME/.cargo/bin:"* ]]; then
        INSTALL_DIR="$HOME/.cargo/bin"
    elif [ -d "$HOME/.local/bin" ] && [[ ":$PATH:" == *":$HOME/.local/bin:"* ]]; then
        INSTALL_DIR="$HOME/.local/bin"
    elif [ -w "/usr/local/bin" ]; then
        INSTALL_DIR="/usr/local/bin"
    elif [ -d "$HOME/.cargo/bin" ]; then
        INSTALL_DIR="$HOME/.cargo/bin"
    else
        INSTALL_DIR="$HOME/.local/bin"
        mkdir -p "$INSTALL_DIR"
    fi
fi

DEST_BIN="${INSTALL_DIR}/${BINARY_NAME}"
echo -e "\n${BLUE}==>${RESET} Installing binary to ${BOLD}${DEST_BIN}${RESET}..."

# Copy binary (use sudo if target is not writable)
if [ -w "$INSTALL_DIR" ] || [ ! -e "$INSTALL_DIR" ]; then
    mkdir -p "$INSTALL_DIR"
    cp -f "$RELEASE_BIN" "$DEST_BIN"
    chmod +x "$DEST_BIN"
else
    echo -e "${YELLOW}Notice: Target directory ${INSTALL_DIR} requires administrative privileges.${RESET}"
    sudo cp -f "$RELEASE_BIN" "$DEST_BIN"
    sudo chmod +x "$DEST_BIN"
fi

# 4. Verify installation
echo -e "\n${BLUE}==>${RESET} Verifying installation..."
if [ -x "$DEST_BIN" ]; then
    INSTALLED_VER="$("$DEST_BIN" --version 2>/dev/null || echo 'installed')"
    echo -e "${GREEN}${BOLD}✓ Successfully installed ${INSTALLED_VER}!${RESET}"
else
    echo -e "${RED}Error: Failed to verify executable at ${DEST_BIN}.${RESET}"
    exit 1
fi

# Check PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "\n${YELLOW}Note:${RESET} ${BOLD}${INSTALL_DIR}${RESET} is not in your current PATH."
    echo -e "Add it to your shell profile (~/.zshrc or ~/.bashrc):"
    echo -e "  ${BOLD}export PATH=\"${INSTALL_DIR}:\$PATH\"${RESET}"
fi

echo -e "\n${BOLD}Quick Start:${RESET}"
echo -e "  ${BOLD}cli-ner doctor${RESET}     # Run system diagnostics"
echo -e "  ${BOLD}cli-ner scan${RESET}       # Analyze reclaimable disk space"
echo -e "  ${BOLD}cli-ner clean${RESET}      # Safe cleanup dry-run"
echo -e "  ${BOLD}cli-ner tui${RESET}        # Launch interactive terminal UI"
echo -e "  ${BOLD}cli-ner --help${RESET}     # View full documentation\n"
