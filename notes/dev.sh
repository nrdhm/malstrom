#!/usr/bin/env bash
# Build / serve a local mdBook rendering of the Agent Notes.
#
# Agent Notes (`.agents/notes/`) are repository-internal decision records and are
# NOT part of the published docs; this is a dev-only review tool that renders them
# with Mermaid and navigation. It works on a generated copy so the notes tree is
# never modified and no committed `SUMMARY.md` trips `verify-agent-notes.py`.
#
# Usage:
#   notes/dev.sh            # build then serve with live reload (http://127.0.0.1:3001)
#   notes/dev.sh build      # build to notes/book/ and exit
#
# Requires: mdbook + mdbook-mermaid (`cargo install mdbook mdbook-mermaid`).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NOTES="$ROOT/.agents/notes"
SRC="$ROOT/notes/src"
MODE="${1:-serve}"

rm -rf "$SRC"
mkdir -p "$SRC"
cp -r "$NOTES/." "$SRC/"

# Links that escape the notes tree (to repo files) cannot resolve inside the book;
# rewrite them to stable GitHub blob URLs in the disposable copy only. Links that
# resolve inside the copy are left untouched.
NOTES_REL=".agents/notes" python3 - "$SRC" "$ROOT" <<'PY'
import os, re, sys
src, root = sys.argv[1], sys.argv[2]
notes_rel = os.environ["NOTES_REL"]
base = "https://github.com/MalstromDevelopers/malstrom/blob/main/"
for dirpath, _, files in os.walk(src):
    for name in files:
        if not name.endswith(".md"):
            continue
        p = os.path.join(dirpath, name)
        rel_dir = os.path.relpath(dirpath, src)
        text = open(p).read()
        def fix(m):
            target = m.group(1)
            if target.startswith(("http://", "https://", "#", "mailto:")):
                return m.group(0)
            path, sep, frag = target.partition("#")
            in_tree = os.path.abspath(os.path.join(src, rel_dir, path))
            src_abs = os.path.abspath(src)
            if in_tree.startswith(src_abs + os.sep) and os.path.exists(in_tree):
                return m.group(0)
            # map to the repo-relative path this note describes; only rewrite when
            # the target actually exists in the repo (ignore illustrative examples).
            repo_rel = os.path.normpath(os.path.join(notes_rel, rel_dir, path))
            if os.path.exists(os.path.join(root, repo_rel)):
                url = base + repo_rel + (sep + frag if sep else "")
                return f"]({url})"
            return m.group(0)
        new = re.sub(r"\]\(([^)]+?)\)", fix, text)
        if new != text:
            open(p, "w").write(new)
PY

# Generate SUMMARY.md for the copied tree (mdBook requires it).
python3 - "$SRC" > "$SRC/SUMMARY.md" <<'PY'
import os, sys
src = sys.argv[1]

def title(path):
    for line in open(path):
        if line.startswith("# "):
            return line[2:].strip()
    return os.path.splitext(os.path.basename(path))[0]

def slug(path):
    return os.path.relpath(path, src)

print("# Summary")
print()
intro = os.path.join(src, "README.md")
if os.path.exists(intro):
    print(f"- [How Agent Notes work]({slug(intro)})")
    print()

for life in ("proposed", "implemented", "rejected", "archived"):
    d = os.path.join(src, life)
    if not os.path.isdir(d):
        continue
    print(f"# {life.capitalize()}")
    print()
    for cls in sorted(os.listdir(d)):
        cdir = os.path.join(d, cls)
        if not os.path.isdir(cdir):
            continue
        notes = sorted(f for f in os.listdir(cdir) if f.endswith(".md"))
        if not notes:
            continue
        # A draft chapter (`- [class]()`) groups its nested notes in the sidebar.
        # mdBook only treats level-1 `#` headings as part titles, so `##` would be
        # dropped — nesting is the way to show the class grouping.
        print(f"- [{cls}]()")
        for f in notes:
            p = os.path.join(cdir, f)
            print(f"  - [{title(p)}]({slug(p)})")
        print()
PY

cd "$ROOT/notes"
# Generated Mermaid assets (git-ignored); install once.
[ -f mermaid.min.js ] || mdbook-mermaid install .
mdbook-mermaid install . >/dev/null 2>&1 || true

if [ "$MODE" = "build" ]; then
    mdbook build
else
    mdbook serve --port 3001 --open 2>/dev/null || mdbook serve --port 3001
fi