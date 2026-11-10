# Install, update, version check

**When to read this:** `massmailer` is not found on the PATH, the
installed version is older than the one this skill describes, or the
user asks how to install, update or remove it. For a complete first
setup start at `onboarding.md`.

**What you can do afterwards:** install or update the binary for the
user's platform without admin rights, verify the version, uninstall, or
build from source.

## Version check

```sh
massmailer --version        # e.g. "massmailer 0.1.0"
massmailer --help           # lists the commands the binary knows
```

If `bounces` is missing from `--help`, the binary predates the bounces
feature; update.

## Install or update with the scripts

No admin rights needed. Both scripts download the binary for the
current system from the latest GitHub release of
`schlingensiepen/massmailer`, verify its SHA-256 checksum against the
published `.sha256` file and abort on mismatch. Running a script again
updates an existing installation.

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/install.ps1 | iex
```

- Installs `massmailer.exe` to `%LOCALAPPDATA%\Programs\massmailer`.
- Adds that folder to the user PATH; a new terminal is needed before
  `massmailer` resolves.
- `$env:MASSMAILER_INSTALL_VERSION = 'v0.1.0'` before the command pins
  a release tag.

Linux and macOS:

```sh
curl -fsSL https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/install.sh | sh
```

- Installs to `~/.local/bin/massmailer`, or to `$MASSMAILER_INSTALL_DIR`
  if set.
- Prints a note if the directory is not on the PATH.
- `MASSMAILER_INSTALL_VERSION=v0.1.0` pins a release tag.
- Supported: Linux x86_64, macOS arm64 and x86_64. Other platforms:
  build from source.

Release assets per tag: `massmailer-x86_64-linux`,
`massmailer-aarch64-macos`, `massmailer-x86_64-macos`,
`massmailer-x86_64-windows.exe`, each with a `<asset>.sha256` file.
Downloading one by hand and checking the checksum is equivalent to the
scripts.

## Uninstall

```sh
curl -fsSL https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/uninstall.sh | sh
```

```powershell
irm https://raw.githubusercontent.com/schlingensiepen/massmailer/main/deploy/uninstall.ps1 | iex
```

Only the binary is removed; run folders and config files stay.

## Build from source

Requires a current stable Rust toolchain (the crate uses edition 2024).

```sh
git clone https://github.com/schlingensiepen/massmailer
cd massmailer
cargo build --release          # binary in target/release/massmailer
cargo test                     # end-to-end tests on the example, no network
```

Do not run a production send from a build directory that another
process may rebuild; copy the binary somewhere stable first.
