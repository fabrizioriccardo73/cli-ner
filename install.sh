#!/usr/bin/env bash
# ==============================================================================
# CLI-NER Installer Script for macOS
# Repository: https://github.com/fabrizioriccardo73/cli-ner
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/fabrizioriccardo73/cli-ner/master/install.sh | bash
# ==============================================================================

set -e

# Visual colors
BOLD="\033[1m"
GREEN="\033[0;32m"
BLUE="\033[0;34m"
YELLOW="\033[0;33m"
RED="\033[0;31m"
RESET="\033[0m"

REPO="fabrizioriccardo73/cli-ner"
BINARY_NAME="cli-ner"

echo -e "${BOLD}${BLUE}"
echo "   ___ _    ___     _  _ ___ ___ "
echo "  / __| |  |_ _|___| \| | __| _ \\"
echo " | (__| |__ | |____| .\` | _||   /"
echo "  \___|____|___|   |_|\_|___|_|_\\"
echo -e "${RESET}"
echo -e "${BOLD}Advanced, safe, and documented CLI for macOS disk space cleanup.${RESET}\n"

# Check OS
OS="$(uname -s)"
if [ "$OS" != "Darwin" ]; then
    echo -e "${RED}Error: cli-ner is designed and optimized specifically for macOS.${RESET}"
    echo "Detected OS: $OS"
    exit 1
fi

# Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
    arm64|aarch64)
        TARGET="aarch64-apple-darwin"
        ;;
    x86_64)
        TARGET="x86_64-apple-darwin"
        ;;
    *)
        echo -e "${RED}Error: Unsupported architecture: $ARCH${RESET}"
        exit 1
        ;;
esac

echo -e "${BLUE}==>${RESET} Detected architecture: ${BOLD}${ARCH}${RESET} (${TARGET})"

# Fetch latest release tag from GitHub API
echo -e "${BLUE}==>${RESET} Finding latest release..."
LATEST_RELEASE=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" 2>/dev/null | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || true)

if [ -z "$LATEST_RELEASE" ]; then
    echo -e "${YELLOW}Warning: Could not fetch latest release tag via GitHub API. Falling back to default release.${RESET}"
    LATEST_RELEASE="v0.1.1"
fi

echo -e "${BLUE}==>${RESET} Latest version: ${BOLD}${LATEST_RELEASE}${RESET}"

TARBALL_NAME="cli-ner-${LATEST_RELEASE}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_RELEASE}/${TARBALL_NAME}"

# Temporary directory
TMP_DIR=$(mktemp -d)
cleanup() {
    rm -rf "$TMP_DIR"
}
trap cleanup EXIT

echo -e "${BLUE}==>${RESET} Downloading ${TARBALL_NAME}..."
if ! curl -fsSL --progress-bar "$DOWNLOAD_URL" -o "${TMP_DIR}/${TARBALL_NAME}"; then
    echo -e "${RED}Error: Failed to download release asset from:${RESET} $DOWNLOAD_URL"
    echo -e "If this version was just tagged, the binary may still be building on GitHub Actions."
    echo -e "You can also install via Cargo: ${BOLD}cargo install --git https://github.com/${REPO}${RESET}"
    exit 1
fi

echo -e "${BLUE}==>${RESET} Extracting archive..."
tar -xzf "${TMP_DIR}/${TARBALL_NAME}" -C "$TMP_DIR"

EXTRACTED_BIN="${TMP_DIR}/cli-ner-${LATEST_RELEASE}-${TARGET}/${BINARY_NAME}"
if [ ! -f "$EXTRACTED_BIN" ]; then
    # In case tarball structure is flat:
    EXTRACTED_BIN="${TMP_DIR}/${BINARY_NAME}"
fi

if [ ! -f "$EXTRACTED_BIN" ]; then
    echo -e "${RED}Error: Binary not found in archive.${RESET}"
    exit 1
fi

chmod +x "$EXTRACTED_BIN"

# Determine Install Destination
INSTALL_DIR="/usr/local/bin"
USE_SUDO=0

if [ -w "$INSTALL_DIR" ]; then
    DEST_PATH="${INSTALL_DIR}/${BINARY_NAME}"
elif [ -d "$HOME/.local/bin" ] && [[ ":$PATH:" == *":$HOME/.local/bin:"* ]]; then
    INSTALL_DIR="$HOME/.local/bin"
    DEST_PATH="${INSTALL_DIR}/${BINARY_NAME}"
else
    mkdir -p "$HOME/.local/bin"
    INSTALL_DIR="$HOME/.local/bin"
    DEST_PATH="${INSTALL_DIR}/${BINARY_NAME}"
fi

echo -e "${BLUE}==>${RESET} Installing to ${BOLD}${DEST_PATH}${RESET}..."
cp "$EXTRACTED_BIN" "$DEST_PATH"

echo -e "\n${GREEN}${BOLD}✓ cli-ner installed successfully!${RESET}\n"

# Verify PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}Note:${RESET} ${INSTALL_DIR} is not in your current PATH."
    echo -e "Add it to your shell configuration (e.g. ~/.zshrc or ~/.bashrc):"
    echo -e "  ${BOLD}export PATH=\"\$HOME/.local/bin:\$PATH\"${RESET}\n"
fi

echo -e "To get started, try running:"
echo -e "  ${BOLD}cli-ner doctor${RESET}     # Check system diagnostics"
echo -e "  ${BOLD}cli-ner scan${RESET}       # Analyze disk space usage"
echo -e "  ${BOLD}cli-ner clean${RESET}      # Safe dry-run cleanup"
echo -e "  ${BOLD}cli-ner --help${RESET}     # Explore all available commands\n"
