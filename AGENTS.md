# AGENTS.md

Instructions for AI agents working in this repository.

## Process rules

- **Never commit unless explicitly asked to.** Do not run `git add`, `git commit`, or
  equivalent as part of a task unless the human explicitly requested a commit. Leave
  changes in the working tree (staged or unstaged) for the human to review and commit
  themselves.
- **Never stage or sweep in files you did not touch.** In particular, do not `git add -A`
  when the working tree contains the human's own staged/untracked changes (notes, config,
  secrets, …) — stage only the files your task actually changed.

## Structure

Prefer locality of behaviour over separation of concern.

Prefer making small files. Aim for one public type/function + associated types/functions.
Do not separate out closely related things.

- One public function + its return type + its error type should all be in the same file.
- One struct + its impl + the return and error types of all functions in the impl should be
  in the same file.

For functions and types always create doc comments. Document parameters, but not the return
type. Remember you can use `[<type name>]` in Rust to create links to other types where
useful. Doc comments should be brief.

Ideally each function which could error should have its own error type.

## Order

In files public types and functions come first. Impl blocks come directly after the type
they are for.

## Libraries to use

- `thiserror` for error types
- `tracing` for logging

## Visibility

Use the lowest visibility possible, in order of preference:

1. private
2. `pub(super)`
3. `pub(crate)`
4. `pub` (only for public API)
