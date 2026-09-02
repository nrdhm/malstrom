# Reviews & design notes

Code reviews with a concrete remediation sketch, one topic per pair of files.

| # | Doc | Scope |
|---|-----|-------|
| 01 | [sources-module-review.md](sources-module-review.md) | Smells in `sources/` — the stateless-as-fake-stateful adapter and eleven concrete issues |
| 02 | [sources-module-redesign.md](sources-module-redesign.md) | The replacement: one `SourceImpl`/`SourcePartition` trait, stateless = `PartitionState = ()`, convenience constructors, framework-owned discovery/completion |
| 03 | [new-scheduler-tests-review.md](new-scheduler-tests-review.md) | `new-scheduler-tests` vs the implemented test-plan note — counts/bug-fix claims verified, dead code to remove, and algorithm simplifications |

Conventions mirror [`../overviews/README.md`](../overviews/README.md): link to code, don't
duplicate it; each file carries a `> **Last refreshed:**` header.
