#!/usr/bin/env bash
# Refresh the overview documents under docs/overviews/.
#
# What it does:
#   1. Re-stamps overviews whose content changed since their last stamp. The canonical
#      freshness header is:
#
#          > **Last refreshed:** YYYY-MM-DD
#
#      Branch-scoped overviews (named `NN-branch-<branch>.md`) additionally carry
#
#          > **Branch/commit:** <branch> @ <short-commit>
#
#      The branch is read from the filename, never from the current checkout, and its tip
#      is resolved with git — so a branch doc is stamped against the branch it *describes*,
#      no matter which branch the file happens to be checked out on. Generic overviews
#      carry a date only: which commit they reflect is git history's job, not the stamp's.
#   2. Untouched overviews are never rewritten — no churn on quiet weeks. A doc is stale
#      when it is untracked, has uncommitted edits, or was committed after its stamp date.
#   3. Reports branch-scoped docs whose branch tip moved past their stamp (refresh or
#      archive them) and docs whose branch no longer exists (dead-branch docs).
#   4. Verifies every numbered overview has an index row in the overviews README.
#   5. Diffs the dependency names documented in 03-dependencies.md against the
#      non-dev [dependencies] of malstrom-core/Cargo.toml and reports drift.
#
# Usage:
#   scripts/refresh-overviews.sh            stamp stale docs + report (default)
#   scripts/refresh-overviews.sh --check    report only; exit 1 if anything is stale
#   scripts/refresh-overviews.sh [--check] [DIR]   (DIR overrides docs/overviews)
#
# Requires: git, GNU sed, awk, grep.
#
# Rationale recorded in .agents/notes/implemented/process/2026-09-03-overview-refresh-mechanics.md

set -euo pipefail

MODE="refresh"
OVERVIEWS_DIR_ARG=""
for arg in "$@"; do
    case "$arg" in
        --check) MODE="check" ;;
        -h | --help)
            sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *) OVERVIEWS_DIR_ARG="$arg" ;;
    esac
done

ROOT="$(git rev-parse --show-toplevel)"
OVERVIEWS_DIR="${OVERVIEWS_DIR_ARG:-$ROOT/docs/overviews}"
DEPS_DOC="$OVERVIEWS_DIR/03-dependencies.md"
CARGO_TOML="$ROOT/malstrom-core/Cargo.toml"
INDEX="$OVERVIEWS_DIR/README.md"

CURRENT_BRANCH="$(git branch --show-current 2>/dev/null || true)"
TODAY="$(date +%Y-%m-%d)"
exit_code=0

# --- helpers ---------------------------------------------------------------

# branch encoded in the filename (`NN-branch-<branch>.md`), or empty for generic docs
branch_of_file() {
    local base="$1"
    if [[ "$base" =~ ^[0-9]+-branch-(.+)\.md$ ]]; then
        echo "${BASH_REMATCH[1]}"
    fi
}

# the YYYY-MM-DD currently stamped in the file, or empty
stamp_date_of() {
    grep -m1 '^> \*\*Last refreshed:\*\*' "$1" 2>/dev/null \
        | grep -oE '[0-9]{4}-[0-9]{2}-[0-9]{2}' | head -1
}

# date the file was last committed on the current branch, or empty
last_commit_date_of() {
    git log -1 --format=%ad --date=short -- "$1" 2>/dev/null || true
}

# 0 = stale (content newer than its stamp), 1 = current
# "content change date" = mtime for untracked or uncommitted files, last commit date
# otherwise; a doc is stale when that date is newer than its stamp (day granularity,
# ISO dates compare lexicographically). Refresh stamps the file the same day it is run,
# so refresh followed by --check passes.
is_stale() {
    local f="$1" cur last mtime
    cur="$(stamp_date_of "$f")"
    if git ls-files --error-unmatch -- "$f" >/dev/null 2>&1; then
        if git diff --quiet HEAD -- "$f"; then
            last="$(last_commit_date_of "$f")" # clean and committed
        else
            last="$(date -r "$f" +%Y-%m-%d)"   # uncommitted edits
        fi
    else
        last="$(date -r "$f" +%Y-%m-%d)"       # untracked (new doc)
    fi
    [ -z "$cur" ] && return 0                  # no stamp yet
    [ -n "$last" ] && [[ "$last" > "$cur" ]] && return 0
    return 1
}

# short commit a branch currently points at (local first, then origin/), or empty
branch_tip() {
    local branch="$1"
    local sha
    sha="$(git rev-parse --short "$branch" 2>/dev/null || true)"
    if [ -z "$sha" ]; then
        sha="$(git rev-parse --short "origin/$branch" 2>/dev/null || true)"
    fi
    echo "$sha"
}

# rewrite the header block with the canonical stamp (date + optional branch/commit)
stamp_file() {
    local f="$1" doc_branch="$2"
    local stamp br_stamp tip
    stamp="> **Last refreshed:** $TODAY"
    br_stamp=""
    if [ -n "$doc_branch" ]; then
        tip="$(branch_tip "$doc_branch")"
        [ -n "$tip" ] && br_stamp="> **Branch/commit:** $doc_branch @ $tip"
    fi
    local tmp
    tmp="$(mktemp)"
    awk -v stamp="$stamp" -v brstamp="$br_stamp" '
        BEGIN { hdr = 0; ins = 0 }
        {
            if (!hdr && /^# /) {
                hdr = 1
                print
                print ""
                print stamp
                if (brstamp != "") print brstamp
                print ""
                ins = 1
                next
            }
            if (ins) {
                # drop the old marker, any lingering Branch/commit line, old
                # multi-line stamp continuations, and blanks in the header gap
                if ($0 ~ /^> \*\*Last refreshed:\*\*/) next
                if ($0 ~ /^> \*\*Branch\/commit:\*\*/) next
                if ($0 ~ /^>/ && $0 !~ /^> \*\*/) next
                if ($0 ~ /^[[:space:]]*$/) next
                ins = 0
            }
            print
        }
    ' "$f" > "$tmp"
    mv "$tmp" "$f"
}

# --- overview files --------------------------------------------------------

echo "==> Overviews ($MODE mode; on $CURRENT_BRANCH @ $(git rev-parse --short HEAD))"
for f in "$OVERVIEWS_DIR"/*.md; do
    [ -f "$f" ] || continue
    base="$(basename "$f")"
    [ "$base" = "README.md" ] && continue

    doc_branch="$(branch_of_file "$base")"

    if is_stale "$f"; then
        if [ -n "$doc_branch" ]; then
            if [ -n "$(branch_tip "$doc_branch")" ]; then
                if [ "$MODE" = "check" ]; then
                    echo "  [stale]  $base — would stamp on $doc_branch"
                    exit_code=1
                else
                    stamp_file "$f" "$doc_branch"
                    echo "  stamped $base ($doc_branch @ $(branch_tip "$doc_branch"))"
                fi
            else
                echo "  [dead]   $base — branch $doc_branch no longer exists; archive or drop this doc"
                [ "$MODE" = "check" ] && exit_code=1
            fi
        else
            if [ "$MODE" = "check" ]; then
                echo "  [stale]  $base — would stamp"
                exit_code=1
            else
                stamp_file "$f" ""
                echo "  stamped $base"
            fi
        fi
    else
        if [ -n "$doc_branch" ]; then
            tip="$(branch_tip "$doc_branch")"
            stamped="$(grep -m1 '^> \*\*Branch/commit:\*\*' "$f" 2>/dev/null \
                | grep -oE '@ [0-9a-f]{7,}' | head -1 | sed 's/@ //' || true)"
            if [ -n "$tip" ] && [ -n "$stamped" ] && [ "$tip" != "$stamped" ]; then
                echo "  [tip]    $base — stamped @ $stamped, $doc_branch now @ $tip; refresh on $doc_branch"
                [ "$MODE" = "check" ] && exit_code=1
            else
                echo "  ok       $base"
            fi
        else
            echo "  ok       $base"
        fi
    fi
done

# --- index table -----------------------------------------------------------

echo "==> Index check ($INDEX)"
missing=""
if [ -f "$INDEX" ]; then
    for f in "$OVERVIEWS_DIR"/[0-9][0-9]-*.md; do
        [ -f "$f" ] || continue
        base="$(basename "$f")"
        if ! grep -qE "\]\($base\)" "$INDEX"; then
            missing="$missing $base"
        fi
    done
    # rows that point at files which do not exist
    dangling="$(grep -oE '\[[^]]+\]\([0-9][0-9]-[^)]+\.md\)' "$INDEX" \
        | sed -E 's/.*\(([0-9][0-9]-[^)]+\.md)\)/\1/' | sort -u \
        | while read -r base; do [ -f "$OVERVIEWS_DIR/$base" ] || echo " $base"; done)"
else
    echo "  !! no $INDEX — create it with a row per numbered overview"
    [ "$MODE" = "check" ] && exit_code=1
fi
if [ -n "$missing" ]; then
    echo "  [missing rows]$missing — add a table row for each numbered overview"
    [ "$MODE" = "check" ] && exit_code=1
else
    echo "  every numbered overview has an index row"
fi
if [ -n "${dangling:-}" ]; then
    echo "  [dangling rows]$dangling — index rows reference deleted files"
    [ "$MODE" = "check" ] && exit_code=1
fi

# --- dependency drift ------------------------------------------------------

if [ ! -f "$CARGO_TOML" ] || [ ! -f "$DEPS_DOC" ]; then
    echo "==> Dependency diff skipped (need $CARGO_TOML and $DEPS_DOC)"
    exit "$exit_code"
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

exit "$exit_code"
