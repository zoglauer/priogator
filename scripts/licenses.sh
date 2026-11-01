#!/usr/bin/env bash
# Collect the licenses of all libraries built into Priogator into
# assets/third-party-licenses.txt, shown under About → Licenses.
# Run after adding or updating dependencies. Needs cargo-about:
#   cargo install --locked cargo-about --features cli
set -euo pipefail
cd "$(dirname "$0")/.."
command -v cargo >/dev/null || source "$HOME/.cargo/env"
cargo about generate --fail assets/licenses.hbs -o assets/third-party-licenses.txt
echo "Wrote assets/third-party-licenses.txt"
