# In-app update publishing

[简体中文](../zh-CN/应用内更新发布.md) · [Documentation index](README.md)

Product changes on main automatically prepare a version, create a tag, run checks, sign, and publish an official release. Documentation-only, CI-only, and release-tooling changes do not publish. Per-release human approval and manual draft publication are removed; PR checks and main branch protection remain.

## One-time activation and signing setup

The `release` Environment retains signing-secret isolation and access restricted to `v*` tags, with no required reviewers or wait timer. An administrator must migrate an existing environment after these changes are merged:

```sh
node scripts/migrate-release-environment.mjs
node scripts/migrate-release-environment.mjs --apply
node scripts/release.mjs verify-environment
```

The first command previews the change. Migration does not create tags, dispatch builds, publish releases, or read/rotate existing secrets. Apply is refused while older release runs or pending deployments exist. Cancel those runs first, migrate the environment, then retry failed jobs as needed. Preparing this PR does not change the live environment.

Reuse existing signing keys. Generate a pair only when none exists:

```sh
node scripts/generate-updater-key.mjs --output /secure/signing-recovery
node scripts/configure-release.mjs --key-file /secure/updater.key --password-file /secure/updater.password
```

If initial environment verification reports that administrators can bypass protection, disable administrator bypass in Settings → Environments → release and verify again. This is one-time environment setup, not per-release approval.

Keep restricted key files outside the repository and maintain an independent encrypted recovery backup. GitHub Secrets must not be the only recovery copy. The client public key must match and `requireSignedVersion` remains enabled. Apple Developer ID and notarization credentials also belong in environment secrets; private values must not enter source, logs, or artifacts.

## Automatic releases from main

1. After a product change reaches main, the preparation workflow classifies the push. Documentation, CI, and release-tooling-only changes skip publication; mixed product changes qualify.
2. Serialized preparation retains a manually selected version (such as `0.3.0`) when it has reviewed release notes and exceeds every occupied tag; otherwise it allocates the next patch version, updates desktop/CLI manifests, locks, and Tauri configuration consistently, and generates deterministic release notes. It creates a release child commit of the captured main commit without writing back to protected main.
3. Create the version tag and explicitly dispatch the desktop workflow. A tag pushed using `GITHUB_TOKEN` cannot be relied upon to start another push workflow. Retries resume the existing tag for the source commit instead of moving or replacing it.
4. Source validation accepts normal version commits on main or strictly validated generated release children. Generated children must match their main parent and deterministic metadata changes; a tag name or commit message alone cannot authorize arbitrary branch code.
5. macOS/Windows native Rust tests and production builds run in parallel. Frontend, documentation, real installer, and bundled CLI/Jev checks remain required. macOS signing and notarization start after its verified installer build, while Rust tests and Windows work continue.
6. After all required checks and provenance/digest validation, assemble and sign updates, attest the artifacts, and publish automatically. Final publication serializes version ordering so an older run finishing late cannot take the Latest mark.

When several product changes belong in one desktop version, collect them in one integration pull request with one bilingual release notes section. Run the checks on the combined changes, then merge the integration pull request once. The merge authorizes automatic publication after the release checks pass.

The version in main may lag behind the latest generated release because release metadata is committed in a child rather than written back to main. Downloads and updates follow published releases. Routine changes no longer require manually editing every version file; both platforms and update manifests must still agree.

For each platform, compile the CLI and desktop on independent workers, then join verified inputs before producing the real installer. Desktop compilation temporarily suppresses resource copying while retaining the original embedded configuration. Final packaging includes the actual CLI/QPDF resources and retains installer and CLI/Jev checks. macOS waits only for its own CLI producer. A packaged-build checkpoint binds its CLI producer and latest attempt, so a failed or superseded CLI retry cannot authorize stale output. CLI and desktop dependency archives are separated by role.

## Website update source migration

Before merging the desktop release changes into main, verify the current website update feed and exercise download, signature verification, installation, restart and data preservation on Apple Silicon, Intel Mac and Windows x64. Deploy the localized documentation and verify the help links before desktop users receive the update.

Starting in 0.3.13, the desktop app selects the official update source using the saved interface language. Simplified Chinese uses the Zleap website. English, Traditional Chinese, Japanese, Korean and Spanish use GitHub. Custom sources are preserved. Keep the release on GitHub so existing clients on the modern signing channel can upgrade.

After GitHub publishes the release, mirror the same signed packages to the website. Verify all published downloads and their digests before updating `https://zleap.com/update/latest-v2.json`. Change only the platform download URLs in the mirrored modern manifest. Keep the GitHub release assets and the frozen `latest.json` (`0.1.8`). Users on the legacy signing channel can install the current stable version manually.

## Failure recovery and artifact reuse

Prefer **Re-run failed jobs** in Actions. If preparation created a tag but dispatch failed, retry the same source/tag. Do not delete/recreate tags, replace published artifacts, or force-move versions.

Reuse is scoped to the same source commit and individual platform/check stages. Verify the original producer and its latest attempt; a failed platform does not invalidate the other platform's valid output. Wait for earlier running producers with a bounded timeout and execute missing/invalid stages locally. A compilation-cache hit never substitutes for a passed test or installer check.

Verify macOS input archives before signing. After signing, still wait for Windows and all shared checks. Final validation binds the signed output to its original input rather than switching producers. Drafts are only an internal upload/validation stage: incomplete checks, corrupt artifacts, signing failures, or failed notarization cannot publish an incomplete release. Fix a published version with a higher version, not replacement bytes.

Rust dependency caches are separate from verified release checkpoints: they never skip tests or installer checks. Ordinary Actions caches cannot cross tag boundaries, so successful trusted tag jobs export dependency-only artifacts, retained for 14 days. Later runs may restore a compatible archive after checking the original repository/workflow, live tag and validated source, latest successful native job attempt, artifact lifetime, metadata and SHA256. Compatibility includes the platform, architecture, target, exact Rust compiler, profile, Cargo manifests, lockfiles and configuration; only local SAG package version changes are ignored. Application/test executables and local SAG libraries are excluded. Invalid or unavailable caches trigger normal compilation. A compatible hit avoids another export. Branch CI measures archive round trips under a separate validation namespace that cannot supply trusted caches; it does not sign or publish. Cache transfer adds optional, bounded I/O, so report it separately from compilation and distinguish branch measurements from actual signed-release latency.

## Download verification and recovery

Use the macOS DMG for manual installation and the notarized app archive for updates; Windows NSIS EXE serves both installation and updating. Update signatures bind the version and remain mandatory in the client.

```sh
gh attestation verify /path/to/downloaded-installer --repo Zleap-AI/SAG-Wiki
```

Provenance, SHA256, and update signatures serve different purposes; none replaces installation/data-preservation checks. Hosted-runner checks do not establish acceptance on every customer device. Measure actual release latency from runs after activation.

Dependency archives use the runner's existing zstd with multithreaded compression and streaming tar I/O. Restore validates the complete staged content and decompressor result before atomically replacing dependency files. Cache failure remains a normal-compilation fallback.

### Skip a release for an individual PR

Add the `release:skip` PR label **before merging** when an internal or build change does not need a separate release. All required PR checks still run. The preparation workflow verifies every product commit in that main push against its merged PR; all must be labeled to skip version/tag creation and publication. Merge, squash and rebase merges are supported. A mixed push with an unlabeled feature PR still releases; a PR lookup error stops preparation without creating a tag. Later feature PRs release normally and include the accumulated changes. This label does not cancel releases already prepared or affect explicit tag/manual releases.

If GitHub has not yet indexed the merged PR, or a product push has no uniquely verifiable merged PR (for example a direct push), preparation stops without creating a tag. Retry after indexing; explicit manual releases remain available.
