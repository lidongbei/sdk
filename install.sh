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
  Linux)  OS_BASE="unknown-linux" ;;
  Darwin) OS_BASE="apple-darwin" ;;
  *)      echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

case "$ARCH" in
  x86_64)        ARCH_TAG="x86_64" ;;
  arm64|aarch64) ARCH_TAG="aarch64" ;;
  *)             echo "Unsupported architecture: $ARCH" >&2; exit 1 ;;
esac

# For Linux, prefer musl (fully static, no GLIBC dependency) for maximum
# compatibility. Fall back to gnu if musl build is unavailable.
if [ "$OS" = "Linux" ]; then
  TARGET="${ARCH_TAG}-${OS_BASE}-musl"
else
  TARGET="${ARCH_TAG}-${OS_BASE}"
fi

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
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

download_file() {
  local url="$1" dest="$2"
  if command -v curl &>/dev/null; then
    curl -fsSL "$url" -o "$dest"
  elif command -v wget &>/dev/null; then
    wget -qO "$dest" "$url"
  else
    echo "Error: curl or wget is required" >&2; exit 1
  fi
}

ARCHIVE="sdk-${TAG}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"

# If musl build unavailable (older release), fall back to gnu
if ! download_file "$URL" "$TMP/$ARCHIVE" 2>/dev/null; then
  if [[ "$TARGET" == *"-musl" ]]; then
    FALLBACK_TARGET="${TARGET/-musl/-gnu}"
    echo "musl build not found, falling back to ${FALLBACK_TARGET}..."
    TARGET="$FALLBACK_TARGET"
    ARCHIVE="sdk-${TAG}-${TARGET}.tar.gz"
    URL="https://github.com/${REPO}/releases/download/${TAG}/${ARCHIVE}"
    download_file "$URL" "$TMP/$ARCHIVE"
  else
    echo "Error: download failed for $URL" >&2; exit 1
  fi
fi

tar -xzf "$TMP/$ARCHIVE" -C "$TMP"
mkdir -p "$INSTALL_DIR"
cp "$TMP/sdk-${TAG}-${TARGET}/sdk" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"

# ── Configure shell profile ────────────────────────────────────────────────
# Detect current shell and select profile file
detect_profile() {
  local shell_name
  shell_name="$(basename "${SHELL:-}")"
  case "$shell_name" in
    bash)
      if [ "$(uname -s)" = "Darwin" ]; then
        echo "$HOME/.bash_profile"
      else
        echo "$HOME/.bashrc"
      fi
      ;;
    zsh)  echo "$HOME/.zshrc" ;;
    fish) echo "$HOME/.config/fish/config.fish" ;;
    *)    echo "" ;;
  esac
}

append_if_missing() {
  local profile="$1"
  local marker="$2"
  local line="$3"
  if [ -n "$profile" ] && ! grep -qF "$marker" "$profile" 2>/dev/null; then
    mkdir -p "$(dirname "$profile")"
    printf '\n%s\n' "$line" >> "$profile"
    echo "  ✓ written to $profile"
    return 0
  fi
  return 1
}

SHELL_NAME="$(basename "${SHELL:-}")"
PROFILE="$(detect_profile)"

# ── Report ─────────────────────────────────────────────────────────────────
echo ""
echo "✓ sdk ${TAG} installed → ${INSTALL_DIR}/${BIN}"
echo ""

PATH_EXPORT="export PATH=\"${INSTALL_DIR}:\$PATH\""
PATH_WRITTEN=false
HOOK_WRITTEN=false

if [ -n "$PROFILE" ] && [ "$SHELL_NAME" != "fish" ]; then
  HOOK_LINE="eval \"\$(sdk hook ${SHELL_NAME})\""

  if ! printf '%s' "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
    append_if_missing "$PROFILE" "$INSTALL_DIR" "$PATH_EXPORT" && PATH_WRITTEN=true
  fi
  append_if_missing "$PROFILE" "sdk hook ${SHELL_NAME}" "$HOOK_LINE" && HOOK_WRITTEN=true

elif [ "$SHELL_NAME" = "fish" ]; then
  FISH_PATH_LINE="fish_add_path ${INSTALL_DIR}"
  FISH_HOOK_LINE="sdk hook fish | source"

  if ! printf '%s' "$PATH" | tr ':' '\n' | grep -qxF "$INSTALL_DIR"; then
    append_if_missing "$PROFILE" "$INSTALL_DIR" "$FISH_PATH_LINE" && PATH_WRITTEN=true
  fi
  append_if_missing "$PROFILE" "sdk hook fish" "$FISH_HOOK_LINE" && HOOK_WRITTEN=true
fi

if [ "$PATH_WRITTEN" = true ] || [ "$HOOK_WRITTEN" = true ]; then
  echo "Shell profile updated: $PROFILE"
  echo ""
  echo "▸ Reload your shell to apply changes:"
  echo ""
  echo "    source \"$PROFILE\""
  echo ""
elif [ -z "$PROFILE" ]; then
  echo "▸ Unknown shell ($SHELL_NAME). Add these lines to your shell profile manually:"
  echo ""
  echo "    export PATH=\"${INSTALL_DIR}:\$PATH\""
  echo "    eval \"\$(sdk hook <shell>)\""
  echo ""
else
  echo "▸ Shell profile already configured: $PROFILE"
  echo ""
fi
