#!/bin/sh
# =============================================================================
#  massmailer - install / update (Linux, macOS)
#
#  Usage:
#    curl -fsSL https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/install.sh | sh
#
#  Downloads the binary for this system from the latest GitHub release to
#  ~/.local/bin/massmailer (or $MASSMAILER_INSTALL_DIR) and verifies the
#  SHA-256 checksum. Running it again updates an existing installation.
#  Optional: MASSMAILER_INSTALL_VERSION=v0.1.0 selects a version.
# =============================================================================
set -eu

repo="schlingensiepen/massmailer"
os=$(uname -s)
arch=$(uname -m)
case "$os/$arch" in
  Linux/x86_64|Linux/amd64)      asset="massmailer-x86_64-linux" ;;
  Darwin/arm64|Darwin/aarch64)   asset="massmailer-aarch64-macos" ;;
  Darwin/x86_64)                 asset="massmailer-x86_64-macos" ;;
  *) echo "massmailer: no prebuilt binary for $os/$arch; build from source with cargo." >&2; exit 1 ;;
esac
if [ -n "${MASSMAILER_INSTALL_VERSION:-}" ]; then
  base="https://github.com/$repo/releases/download/$MASSMAILER_INSTALL_VERSION"
else
  base="https://github.com/$repo/releases/latest/download"
fi
dir="${MASSMAILER_INSTALL_DIR:-$HOME/.local/bin}"
mkdir -p "$dir"
tmp=$(mktemp "$dir/.massmailer.XXXXXX")
trap 'rm -f "$tmp"' EXIT

echo "massmailer: downloading $base/$asset ..."
curl -fsSL "$base/$asset" -o "$tmp"
expected=$(curl -fsSL "$base/$asset.sha256" | awk '{print $1}' | tr 'A-F' 'a-f')
if command -v sha256sum >/dev/null 2>&1; then
  actual=$(sha256sum "$tmp" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$tmp" | awk '{print $1}')
fi
if [ "$expected" != "$actual" ]; then
  echo "Checksum mismatch (expected $expected, got $actual). Aborting." >&2
  exit 1
fi
chmod 755 "$tmp"
mv -f "$tmp" "$dir/massmailer"
trap - EXIT
"$dir/massmailer" --version
case ":$PATH:" in
  *":$dir:"*) ;;
  *) echo "Note: $dir is not in your PATH. Add it, e.g.: echo 'export PATH=\"$dir:\$PATH\"' >> ~/.profile" ;;
esac
echo "massmailer installed to $dir/massmailer. Try: massmailer --help"
