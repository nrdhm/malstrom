# AGENTS.md — Archived Agent Notes

Archived Agent Notes under the class directories are frozen historical snapshots, not
current authority. Never edit, reformat, update, delete, or move a sealed artifact; use an
active Agent Note or current documentation for new decisions and facts.

The archival change may only: relocate the note from `implemented/`, insert the identical
`Archived: YYYY-MM-DD` line below the `Status: implemented` line, and repair or delete
inbound links. Do not inspect, verify, or repair links *out of* archived notes.

Append the sealed note's sha256 to [`manifest.json`](manifest.json) as part of the archival
change (compute with `sha256sum`; keep `"version": 1` and the `{"files": {…}}` shape).
A changed or missing sealed artifact, an unknown class folder, or invalid archive metadata
indicates tampering or an incomplete archival change — fix the change, not the archive.
