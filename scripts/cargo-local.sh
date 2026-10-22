#!/bin/sh
# Keep all development cache, toolchain and temporary files in this checkout.
set -eu
usb_repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
export CARGO_HOME="$usb_repo_root/.dev/cargo-home"
export TMPDIR="$usb_repo_root/.dev/tmp"
export CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-true}"
mkdir -p "$CARGO_HOME" "$TMPDIR"
if [ -x "$CARGO_HOME/bin/cargo" ] && [ -d "$usb_repo_root/.dev/rustup" ]; then
    export RUSTUP_HOME="$usb_repo_root/.dev/rustup"
    exec "$CARGO_HOME/bin/cargo" "$@"
fi
exec cargo "$@"
