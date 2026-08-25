# Agent Note: Rename the k8s operator crate to `malstrom-k8s-operator`

Status: implemented

## Problem

The k8s operator crate was named `malstrom-operator` — one letter away from the stdlib
crate `malstrom-operators`. The two are unrelated and easily confused (and were, in the CI
exclusion saga: `--exclude malstrom-operator` vs `malstrom-operators`).

## Decision

Rename **every** `malstrom-operator` mention (that is not `malstrom-operators`) to
**`malstrom-k8s-operator`**, so no source- or deployment-level name can be confused with the
stdlib crate:

- **Crate**: package name (`malstrom-k8s/operator/Cargo.toml`) and the `[[bin]]` artifact.
- **Source**: the kube `PatchParams::apply` field-manager names in `finalizer.rs`,
  `job/create.rs`, `job/patch.rs`; the CRD `service_account` defaults in `crds/src/lib.rs`;
  the operator `build.rs`'s CRD-YAML output path.
- **Helm**: chart directory `operator/helm/malstrom-operator` → `operator/helm/malstrom-k8s-operator`
  (Chart.yaml name, values/local-values image repository + name, template helper defines),
  and the release names in `install-operator.sh` and `website/guide/Kubernetes.md`.
- **CI/images**: the dockerfile (`malstrom-operator.dockerfile` → `malstrom-k8s-operator.dockerfile`
  + its `COPY` path), the GHCR image tags in `ghcr.yaml`, the service list in
  `push-all-images.sh`, the `helm package` path in `pages.yaml`.
- **Docs**: `docs/overviews/01-project.md`.

The replacement is word-boundary-aware (`malstrom-operator` not followed by `s`), so
`malstrom-operators` is never touched.

## Alternatives considered

- **Crate-only rename** — disambiguates source, but the binary/image/helm/field-manager
  names still collide visually with the stdlib crate. Rejected in favor of full uniqueness.
- **Status quo** — the one-letter confusion persists. Rejected.

## Consequences

- `malstrom-operator` (exactly) no longer appears anywhere in the repo; the k8s operator is
  `malstrom-k8s-operator` throughout.
- Deployment names (image tag, helm release/chart, field manager, service account) change in
  lockstep; existing clusters using the old names must migrate when they next upgrade
  (pre-1.0, cheap).
- Verification: `cargo check -p malstrom-k8s-operator` and `cargo check --workspace` clean;
  the operator bin builds as `malstrom-k8s-operator`.
