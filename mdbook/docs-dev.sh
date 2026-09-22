#!/usr/bin/env bash
# Serve the internals book (mdBook) with live reload at http://127.0.0.1:3000.
#
# Requires: mdbook + mdbook-mermaid (`cargo install mdbook mdbook-mermaid`).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/mdbook"

# Generate the Mermaid assets on first run (they are git-ignored).
[ -f mermaid.min.js ] || mdbook-mermaid install .

mdbook serve