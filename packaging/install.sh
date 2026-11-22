#!/usr/bin/env sh
set -eu

REPO="HamidRezaAshkiyan/folio"
VERSION="${FOLIO_VERSION:-latest}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64|aarch64) ASSET="folio-macos-arm64.tar.gz" ;;
      x86_64)        ASSET="folio-macos-x64.tar.gz" ;;
      *) echo "Unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64) ASSET="folio-linux-x64.tar.gz" ;;
      *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS (use install.ps1 or folio-windows-x64-setup.exe on Windows)" >&2
    exit 1
    ;;
esac

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/${REPO}/releases/latest/download/${ASSET}"
else
  URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"
fi

INSTALL_DIR="${FOLIO_INSTALL_DIR:-$HOME/.local/bin}"
if [ -w "/usr/local/bin" ] && [ -z "${FOLIO_INSTALL_DIR:-}" ]; then
  INSTALL_DIR="/usr/local/bin"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading ${URL}..."
curl -fsSL "$URL" -o "${TMP_DIR}/${ASSET}"

tar -xzf "${TMP_DIR}/${ASSET}" -C "$TMP_DIR"
mkdir -p "$INSTALL_DIR"
install -m 755 "${TMP_DIR}/folio" "${INSTALL_DIR}/folio"

echo "Installed folio to ${INSTALL_DIR}/folio"
case ":$PATH:" in
  *":${INSTALL_DIR}:"*) ;;
  *)
    echo ""
    echo "Add this to your shell profile (~/.zshrc or ~/.bashrc):"
    echo "  export PATH=\"${INSTALL_DIR}:\$PATH\""
    ;;
esac
echo ""
echo "Run 'folio --tui' or 'folio --gui' to get started."
