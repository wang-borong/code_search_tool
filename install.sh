#!/usr/bin/env bash

# Copyright (C) 2022-2026 Jason Wang
# install.sh - Installer for fcs (Fuzzy Code Search)

set -euo pipefail

# --- 1. Detect OS and CPU Architecture ---

OS="$(uname -s)"
case "${OS}" in
    Linux)     OS_NAME="linux" ;;
    Darwin)    OS_NAME="apple-darwin" ;;
    MSYS*|MINGW*|CYGWIN*) OS_NAME="pc-windows-msvc" ;;
    *)
        echo "Unsupported Operating System: ${OS}" >&2
        exit 1
        ;;
esac

ARCH="$(uname -m)"
case "${ARCH}" in
    x86_64|amd64) ARCH_NAME="x86_64" ;;
    aarch64|arm64) ARCH_NAME="aarch64" ;;
    i386|i686) ARCH_NAME="i686" ;;
    armv7l|armhf) ARCH_NAME="arm" ;;
    *)
        echo "Unsupported Architecture: ${ARCH}" >&2
        exit 1
        ;;
esac

# For Linux, detect if we should use musl libc
LIBC="gnu"
if [ "${OS_NAME}" = "linux" ]; then
    if ldd /bin/ls 2>&1 | grep -q "musl" || [ -f /lib/ld-musl-x86_64.so.1 ] || [ -f /lib/ld-musl-aarch64.so.1 ]; then
        LIBC="musl"
    fi
fi

# --- 2. Map to target names ---

if [ "${OS_NAME}" = "apple-darwin" ]; then
    TARGET="${ARCH_NAME}-apple-darwin"
elif [ "${OS_NAME}" = "pc-windows-msvc" ]; then
    TARGET="${ARCH_NAME}-pc-windows-msvc"
else
    # Linux
    if [ "${ARCH_NAME}" = "arm" ]; then
        if [ "${LIBC}" = "musl" ]; then
            TARGET="arm-unknown-linux-musleabihf"
        else
            TARGET="arm-unknown-linux-gnueabihf"
        fi
    else
        TARGET="${ARCH_NAME}-unknown-linux-${LIBC}"
    fi
fi

# Package suffix: zip for Windows, tar.gz for Linux/macOS
SUFFIX=".tar.gz"
if [[ "${TARGET}" == *"-pc-windows-"* ]]; then
    SUFFIX=".zip"
fi

echo "Detected target: ${TARGET}"

# --- 3. Resolve Download URL from GitHub ---

REPO="wang-borong/code_search_tool"
API_URL="https://api.github.com/repos/${REPO}/releases/latest"

echo "Fetching latest release information from GitHub..."
RELEASE_JSON=$(curl -s "${API_URL}")

# Try to parse tag and download URL using GitHub API
TAG=$(echo "${RELEASE_JSON}" | grep -m1 '"tag_name":' | cut -d '"' -f 4 || true)
DOWNLOAD_URL=""

if [ -n "${TAG}" ]; then
    echo "Latest release tag (via API): ${TAG}"
    DOWNLOAD_URL=$(echo "${RELEASE_JSON}" | grep "browser_download_url" | grep "${TARGET}${SUFFIX}" | cut -d '"' -f 4 | head -n 1 || true)
fi

# Fallback: if API fails or is rate limited, use redirect headers and construct URL
if [ -z "${DOWNLOAD_URL}" ]; then
    echo "GitHub API rate limit exceeded or target not found. Using fallback detection..."
    REDIRECT_URL=$(curl -sI "https://github.com/${REPO}/releases/latest" | grep -i "location:" | awk '{print $2}' | tr -d '\r' || true)
    if [ -n "${REDIRECT_URL}" ]; then
        TAG="${REDIRECT_URL##*/}"
        if [ -n "${TAG}" ]; then
            echo "Latest release tag (via redirect): ${TAG}"
            DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${TAG}/fcs-${TAG}-${TARGET}${SUFFIX}"
        fi
    fi
fi

if [ -z "${DOWNLOAD_URL}" ]; then
    echo "Error: Could not find or resolve release package for target ${TARGET}." >&2
    exit 1
fi

# --- 4. Download and Extract package ---

TMP_DIR=$(mktemp -d)
clean_up() {
    rm -rf "${TMP_DIR}"
}
trap clean_up EXIT

echo "Downloading ${DOWNLOAD_URL}..."
curl -L -o "${TMP_DIR}/fcs_pkg${SUFFIX}" "${DOWNLOAD_URL}"

echo "Extracting package..."
mkdir -p "${TMP_DIR}/extracted"
if [ "${SUFFIX}" = ".zip" ]; then
    if command -v unzip &>/dev/null; then
        unzip -q "${TMP_DIR}/fcs_pkg.zip" -d "${TMP_DIR}/extracted"
    elif command -v 7z &>/dev/null; then
        7z x -y "${TMP_DIR}/fcs_pkg.zip" -o"${TMP_DIR}/extracted" >/dev/null
    else
        echo "Error: Neither unzip nor 7z is installed to extract the zip file." >&2
        exit 1
    fi
else
    tar -xzf "${TMP_DIR}/fcs_pkg${SUFFIX}" -C "${TMP_DIR}/extracted"
fi

# Locate the binary recursively inside extracted files
BIN_FILE=$(find "${TMP_DIR}/extracted" -type f \( -name "fcs" -o -name "fcs.exe" \) | head -n 1)
if [ -z "${BIN_FILE}" ]; then
    echo "Error: Could not find binary 'fcs' or 'fcs.exe' in the downloaded package." >&2
    exit 1
fi

# --- 5. Install the binary ---

INSTALL_DIR="/usr/local/bin"
if [ -w "${INSTALL_DIR}" ]; then
    cp "${BIN_FILE}" "${INSTALL_DIR}/fcs"
    chmod +x "${INSTALL_DIR}/fcs"
    echo "Successfully installed fcs to ${INSTALL_DIR}/fcs"
else
    if command -v sudo &>/dev/null; then
        echo "Requesting sudo privileges to install to ${INSTALL_DIR}..."
        sudo cp "${BIN_FILE}" "${INSTALL_DIR}/fcs"
        sudo chmod +x "${INSTALL_DIR}/fcs"
        echo "Successfully installed fcs to ${INSTALL_DIR}/fcs"
    else
        # Fall back to user local directory
        USER_BIN_DIR="${HOME}/.local/bin"
        mkdir -p "${USER_BIN_DIR}"
        cp "${BIN_FILE}" "${USER_BIN_DIR}/fcs"
        chmod +x "${USER_BIN_DIR}/fcs"
        echo "Successfully installed fcs to ${USER_BIN_DIR}/fcs"
        if [[ ":$PATH:" != *":${USER_BIN_DIR}:"* ]]; then
            echo "Warning: ${USER_BIN_DIR} is not in your PATH. Please add it to your shell configuration."
        fi
    fi
fi

# --- 6. Verify neovim dependency ---

if ! command -v nvim &>/dev/null; then
    echo ""
    echo "=========================================================================="
    echo "WARNING: neovim ('nvim') is not installed on this system."
    echo "fcs requires neovim to preview and edit files."
    echo "Please install neovim before using fcs for the best experience."
    echo "=========================================================================="
    echo ""
else
    echo "Check: neovim is installed."
fi
