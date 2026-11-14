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
2. Serialized preparation allocates the next patch version, updates desktop/CLI manifests, locks, and Tauri configuration consistently, and generates deterministic release notes. It creates a release child commit of the captured main commit without writing back to protected main.
3. Create the version tag and explicitly dispatch the desktop workflow. A tag pushed using `GITHUB_TOKEN` cannot be relied upon to start another push workflow. Retries resume the existing tag for the source commit instead of moving or replacing it.
4. Source validation accepts normal version commits on main or strictly validated generated release children. Generated children must match their main parent and deterministic metadata changes; a tag name or commit message alone cannot authorize arbitrary branch code.
5. macOS/Windows native Rust tests and production builds run in parallel. Frontend, documentation, real installer, and bundled CLI/Jev checks remain required. macOS signing and notarization start once its own checks pass, independently of Windows.
6. After all required checks and provenance/digest validation, assemble and sign updates, attest the artifacts, and publish automatically. Final publication serializes version ordering so an older run finishing late cannot take the Latest mark.

The version in main may lag behind the latest generated release because release metadata is committed in a child rather than written back to main. Downloads and updates follow published releases. Routine changes no longer require manually editing every version file; both platforms and update manifests must still agree.

## Failure recovery and artifact reuse

Prefer **Re-run failed jobs** in Actions. If preparation created a tag but dispatch failed, retry the same source/tag. Do not delete/recreate tags, replace published artifacts, or force-move versions.

Reuse is scoped to the same source commit and individual platform/check stages. Verify the original producer and its latest attempt; a failed platform does not invalidate the other platform's valid output. Wait for earlier running producers with a bounded timeout and execute missing/invalid stages locally. A compilation-cache hit never substitutes for a passed test or installer check.

Verify macOS input archives before signing. After signing, still wait for Windows and all shared checks. Final validation binds the signed output to its original input rather than switching producers. Drafts are only an internal upload/validation stage: incomplete checks, corrupt artifacts, signing failures, or failed notarization cannot publish an incomplete release. Fix a published version with a higher version, not replacement bytes.

## Download verification and recovery

Use the macOS DMG for manual installation and the notarized app archive for updates; Windows NSIS EXE serves both installation and updating. Update signatures bind the version and remain mandatory in the client.

```sh
gh attestation verify /path/to/downloaded-installer --repo Zleap-AI/SAG-Wiki
```

Provenance, SHA256, and update signatures serve different purposes; none replaces installation/data-preservation checks. Hosted-runner checks do not establish acceptance on every customer device. Measure actual release latency from runs after activation.
