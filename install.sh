#!/usr/bin/env bash
# One-line install:
#   curl -fsSL https://raw.githubusercontent.com/lidongbei/sdk/main/install.sh | bash
set -euo pipefail

REPO="lidongbei/sdk"
INSTALL_DIR="${SDK_INSTALL_DIR:-$HOME/.sdk/bin}"
BIN="sdk"

# ── Detect platform ────────────────────────────────────────────────────────
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)  OS_TAG="unknown-linux-gnu" ;;
  Darwin) OS_TAG="apple-darwin" ;;
  *)      echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64)       ARCH_TAG="x86_64" ;;
  arm64|aarch64) ARCH_TAG="aarch64" ;;
  *)            echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

TARGET="${ARCH_TAG}-${OS_TAG}"

# ── Fetch latest tag ───────────────────────────────────────────────────────
echo "Fetching latest release..."

# Use the web redirect approach to avoid GitHub API unauthenticated rate limits.
# Falls back to the JSON API (with optional GITHUB_TOKEN) if the redirect fails.
TAG=$(curl -fsSL -o /dev/null -w '%{url_effective}' \
  "https://github.com/${REPO}/releases/latest" \
  | grep -oE '[^/]+$')

if [ -z "$TAG" ]; then
  AUTH_HEADER=""
  if [ -n "${GITHUB_TOKEN:-}" ]; then
    AUTH_HEADER="Authorization: Bearer ${GITHUB_TOKEN}"
  fi
  TAG=$(curl -fsSL ${AUTH_HEADER:+-H "$AUTH_HEADER"} \
    "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name"' \
    | sed 's/.*"tag_name": *"\([^"]*\)".*/\1/')
fi

if [ -z "$TAG" ]; then
  echo "Error: could not determine latest release tag" >&2
  exit 1
fi

echo "Installing sdk ${TAG} (${TARGET})..."

# ── Download & extract ─────────────────────────────────────────────────────
ARCHIVE="sdk-${TAG}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

if command -v curl &>/dev/null; then
  curl -fsSL "$URL" -o "$TMP/$ARCHIVE"
elif command -v wget &>/dev/null; then
  wget -qO "$TMP/$ARCHIVE" "$URL"
else
  echo "Error: curl or wget is required" >&2; exit 1
fi

tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
mkdir -p "$INSTALL_DIR"
cp "$TMP/sdk-${TAG}-${TARGET}/sdk" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"

# ── Report ─────────────────────────────────────────────────────────────────
echo ""
echo "✓ sdk ${TAG} installed → ${INSTALL_DIR}/${BIN}"
echo ""

if ! printf '%s' "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
  echo "▸ Add sdk to your PATH by appending to your shell profile:"
  echo ""
  echo "    export PATH=\"\$HOME/.sdk/bin:\$PATH\""
  echo ""
fi

echo "▸ Enable the shell hook (version auto-switching) by adding to your profile:"
echo ""
echo "    # bash"
echo "    eval \"\$(sdk hook bash)\""
echo ""
echo "    # zsh"
echo "    eval \"\$(sdk hook zsh)\""
echo ""
echo "    # fish"
echo "    sdk hook fish | source"
echo ""
