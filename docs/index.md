# Malstrom Internals

Maintainer documentation for Malstrom: architecture overviews and per-topic code reviews.

This book is built with [mdBook](https://rust-lang.github.io/mdBook/) and rendered with
Mermaid diagrams (`mdbook-mermaid`). It is the **maintainer** surface — rationale, module
maps, and review notes. The user-facing documentation lives on the
[website](https://malstrom.io); the two surfaces are deliberately separate.

## Contents

- **Overviews** — concise, code-linked docs on how the repository and the kernel are put
  together: the project layout, dependencies, module graph, architecture, channels, and a
  survey of remote-transport designs.
- **Reviews** — code reviews with concrete remediation sketches, one topic per file.

## Building locally

```sh
cargo install mdbook mdbook-mermaid   # once
scripts/refresh-overviews.sh          # re-stamp stale overviews (optional)
mdbook/docs-dev.sh                    # http://127.0.0.1:3000  (or: cd mdbook && mdbook serve)
```

The book project lives in [`mdbook/`](../mdbook) (`book.toml`, generated Mermaid assets,
build output); these `docs/*.md` files are its sources. The Rust API reference is built with:

```sh
cargo doc --workspace --no-deps --open
```

## Conventions

The overviews follow the conventions recorded in
[Overviews](overviews/README.md): one topic per file, numbered in reading order, code linked
rather than duplicated, and a `> **Last refreshed:**` freshness header maintained by
`scripts/refresh-overviews.sh`. Links to code and Agent Notes use stable GitHub URLs so they
resolve both here and on the repository.