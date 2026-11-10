#!/bin/sh
# massmailer - uninstall (Linux, macOS)
#   curl -fsSL https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/uninstall.sh | sh
# Removes the binary; run folders and config files are left untouched.
set -eu
dir="${MASSMAILER_INSTALL_DIR:-$HOME/.local/bin}"
rm -f "$dir/massmailer" && echo "massmailer has been removed from $dir."
