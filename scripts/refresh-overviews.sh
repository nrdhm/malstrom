#!/usr/bin/env bash
# Refresh the overview documents under docs/overviews/.
#
# What it does:
#   1. Stamps `> **Last refreshed:** <date> (<branch> @ <short-commit>)` into every
#      overview file that has the marker (inserts it after the title if missing).
#   2. Updates `> **Branch/commit:** ...` in branch-scoped overviews.
#   3. Diffs the dependency names documented in 03-dependencies.md against the
#      non-dev [dependencies] of malstrom-core/Cargo.toml and reports drift.
#
# Usage:  scripts/refresh-overviews.sh          (from anywhere in the repo)
# Requires: git, GNU sed, awk, grep.

set -euo pipefail

ROOT="$(git rev-parse --show-toplevel)"
OVERVIEWS_DIR="${1:-$ROOT/docs/overviews}"
DEPS_DOC="$OVERVIEWS_DIR/03-dependencies.md"
CARGO_TOML="$ROOT/malstrom-core/Cargo.toml"

BRANCH="$(git branch --show-current 2>/dev/null || true)"
[ -z "$BRANCH" ] && BRANCH="detached"
COMMIT="$(git rev-parse --short HEAD)"
DATE="$(date +%Y-%m-%d)"
STAMP="> **Last refreshed:** $DATE ($BRANCH @ $COMMIT)"
BRANCH_STAMP="> **Branch/commit:** $BRANCH @ $COMMIT ($DATE)"

echo "==> Stamping overview headers ($BRANCH @ $COMMIT)"
for f in "$OVERVIEWS_DIR"/*.md; do
    [ "$(basename "$f")" = "README.md" ] && continue
    if grep -q '^> \*\*Last refreshed:\*\*' "$f"; then
        sed -i -E "s|^> \*\*Last refreshed:\*\*.*|$STAMP|" "$f"
    else
        awk -v ins="$STAMP" '
            BEGIN { done = 0 }
            /^# / && !done { print; print ""; print ins; done = 1; next }
            { print }
        ' "$f" > "$f.tmp" && mv "$f.tmp" "$f"
    fi
    if grep -q '^> \*\*Branch/commit:\*\*' "$f"; then
        sed -i -E "s|^> \*\*Branch/commit:\*\*.*|$BRANCH_STAMP|" "$f"
    fi
    echo "    ok: $(basename "$f")"
done

if [ ! -f "$CARGO_TOML" ] || [ ! -f "$DEPS_DOC" ]; then
    echo "!! Skipping dependency diff (need $CARGO_TOML and $DEPS_DOC)"
    exit 0
fi

echo "==> Dependency list diff (malstrom-core/Cargo.toml vs $DEPS_DOC)"
# Non-dev dependency names from the manifest ([dependencies] ... next [section])
awk '
    /^\[dependencies\]/ { f = 1; next }
    /^\[/ { f = 0 }
    f && /^[A-Za-z0-9_-]+[[:space:]]*=/ { sub(/[ ="].*/, ""); print }
' "$CARGO_TOML" | sort -u > /tmp/deps-manifest.txt
# Documented dependency names (lines starting with **crate** in the doc)
grep -oE '^\*\*[A-Za-z0-9_-]+\*\*' "$DEPS_DOC" | sed -E 's/^\*\*|\*\*$//g' | sort -u > /tmp/deps-doc.txt

if diff -u /tmp/deps-doc.txt /tmp/deps-manifest.txt > /tmp/deps-diff.txt; then
    echo "    dependency lists in sync"
else
    cat /tmp/deps-diff.txt
    echo "    ^ update $DEPS_DOC: document new deps, delete sections for removed ones"
fi
