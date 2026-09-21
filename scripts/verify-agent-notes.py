#!/usr/bin/env python3
"""Verify the Agent Notes tree.

Checks the rules in .agents/notes/README.md and the per-directory AGENTS.md files:

1. Tree shape:
   - lifecycle dirs are one of proposed/implemented/rejected/archived
   - class dirs are one of feature/bug-fix/simplification/architecture/process/testing
   - filenames match yyyy-mm-dd-topic-title.md
   - no unexpected files (for example a central INDEX.md)

2. Note format:
   - header block: `# Agent Note: <title>` / blank / `Status: <status>`
   - Status agrees with the lifecycle folder
   - archived notes carry `Archived: YYYY-MM-DD` right after Status
   - required/forbidden sections per lifecycle

3. Markdown links:
   - every relative link inside the notes tree resolves to an existing file
   - absolute/root-relative links are reported as failures, because the README
     requires links to survive moves between folders

Exit status is 0 when clean and 1 when any problem is found.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

LIFECYCLES = {"proposed", "implemented", "rejected", "archived"}
CLASSES = {"feature", "bug-fix", "simplification", "architecture", "process", "testing"}

HEADER_RE = re.compile(r"^# Agent Note: (.+)$")
STATUS_RE = re.compile(r"^Status: (.+)$")
DATE_RE = re.compile(r"^(\d{4}-\d{2}-\d{2})-(.+)\.md$")
ARCHIVED_RE = re.compile(r"^Archived: (\d{4}-\d{2}-\d{2})$")
LINK_RE = re.compile(r"\]\(([^)]+)\)")
SCHEME_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9+.-]*:")

REQUIRED_SECTIONS = {
    "proposed": {
        "## Problem",
        "## Proposal",
        "## Alternatives considered",
        "## Acceptance criteria",
        "## Risks",
    },
    "implemented": {
        "## Problem",
        "## Decision",
        "## Alternatives considered",
        "## Consequences",
    },
    "rejected": {
        "## Problem",
        "## Proposal",
        "## Alternatives considered",
    },
    "archived": {"## Problem"},
}
FORBIDDEN_IMPLEMENTED = {
    "## Proposal",
    "## Plan",
    "## Migration plan",
    "## Acceptance criteria",
}

# Files that are part of the rulebook rather than notes.
SKIP_NAMES = {"AGENTS.md", "README.md"}


class Checker:
    def __init__(self, root: Path) -> None:
        self.root = root
        self.errors: list[str] = []

    def error(self, path: Path, message: str) -> None:
        self.errors.append(f"{path.relative_to(self.root).as_posix()}: {message}")

    def check_tree(self, path: Path) -> None:
        for child in sorted(path.iterdir()):
            if child.is_dir():
                if child.name in LIFECYCLES:
                    self.check_lifecycle(child)
                else:
                    self.error(child, f"unknown top-level directory {child.name!r}")
            elif child.is_file():
                if child.name == ".gitkeep" or child.name in SKIP_NAMES:
                    continue
                self.error(child, "unexpected file at notes root")

    def check_lifecycle(self, path: Path) -> None:
        for child in sorted(path.iterdir()):
            if child.is_dir():
                if child.name in CLASSES:
                    self.check_class(path.name, child)
                else:
                    self.error(child, f"unknown class directory {child.name!r} under {path.name}/")
            elif child.is_file():
                if child.name == ".gitkeep" or child.name in SKIP_NAMES:
                    continue
                if child.name == "manifest.json" and path.name == "archived":
                    continue
                self.error(child, f"unexpected file {child.name!r} in lifecycle {path.name}/")

    def check_class(self, lifecycle: str, path: Path) -> None:
        for child in sorted(path.iterdir()):
            if child.is_dir():
                self.error(child, "unexpected nested directory")
            elif child.is_file():
                if child.name == ".gitkeep":
                    continue
                self.check_note(lifecycle, path.name, child)

    def check_note(self, lifecycle: str, cls: str, path: Path) -> None:
        if not DATE_RE.match(path.name):
            self.error(path, f"filename does not match yyyy-mm-dd-topic-title.md: {path.name!r}")
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines()

        # Header block: `# Agent Note: <title>` / blank / `Status: <status>`
        if not lines or not HEADER_RE.match(lines[0]):
            self.error(path, "line 1 must be `# Agent Note: <title>`")
        if len(lines) < 2 or lines[1] != "":
            self.error(path, "line 2 must be blank")
        if len(lines) < 3 or not STATUS_RE.match(lines[2]):
            self.error(path, "line 3 must be `Status: <status>`")
        else:
            status = STATUS_RE.match(lines[2]).group(1)
            self.check_status(lifecycle, status, path)

        if lifecycle == "archived":
            if len(lines) < 4 or not ARCHIVED_RE.match(lines[3]):
                self.error(path, "archived note must have `Archived: YYYY-MM-DD` on line 4")

        headings = {line for line in lines if line.startswith("## ")}
        required = REQUIRED_SECTIONS.get(lifecycle, set())
        for section in sorted(required - headings):
            self.error(path, f"missing required section {section}")
        if lifecycle == "implemented":
            for section in sorted(FORBIDDEN_IMPLEMENTED & headings):
                self.error(path, f"implemented note contains forbidden section {section}")

        self.check_links(path, text)

    def check_status(self, lifecycle: str, status: str, path: Path) -> None:
        if lifecycle == "proposed" and status != "proposed":
            self.error(path, f"proposed note has Status {status!r}, expected 'proposed'")
        elif lifecycle == "implemented" and status != "implemented":
            self.error(path, f"implemented note has Status {status!r}, expected 'implemented'")
        elif lifecycle == "rejected" and not status.startswith("rejected — "):
            self.error(path, f"rejected note Status must start 'rejected — ', got {status!r}")
        elif lifecycle == "archived" and status != "implemented":
            self.error(path, f"archived note has Status {status!r}, expected 'implemented'")

    def check_links(self, path: Path, text: str) -> None:
        for match in LINK_RE.finditer(text):
            target = match.group(1).strip()
            target_path = target.split("#", 1)[0].strip()
            if not target_path:
                continue
            if target_path.startswith("/"):
                self.error(path, f"absolute link {target!r}; use a relative link that survives moves")
                continue
            if SCHEME_RE.match(target_path):
                continue  # external URL / mailto
            destination = (path.parent / target_path).resolve()
            if not destination.exists():
                self.error(path, f"broken link {target!r} (resolved to {destination})")

    def run(self) -> int:
        self.check_tree(self.root)
        if self.errors:
            for error in self.errors:
                print(f"ERROR: {error}", file=sys.stderr)
            return 1
        print(f"OK: valid Agent Notes tree under {self.root}")
        return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--root",
        default=".agents/notes",
        type=Path,
        help="path to the notes tree (default: .agents/notes)",
    )
    args = parser.parse_args()
    checker = Checker(args.root)
    return checker.run()


if __name__ == "__main__":
    sys.exit(main())