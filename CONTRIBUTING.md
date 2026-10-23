# Contributing

SAG Wiki uses one repository with two product branches:

| Platform | Base branch | Required build | Release tag |
| --- | --- | --- | --- |
| Windows / macOS desktop | `main` | Windows x64 / macOS arm64 | `vX.Y.Z` |
| fnOS | `fnos/develop` | x86-64 FPK only | `fnos-vX.Y.Z` |

Create `feat/desktop-*` from `main` or `feat/fnos-*` from `fnos/develop`, and explicitly select that base when opening a PR. Do not merge the entire product branches into each other.

Split shared changes (`core`, `protocol`, `client`, `cli`, shared UI) from platform adapters. Open a second PR from the destination branch using `git cherry-pick -x <commit>` when compatible; otherwise implement the platform adaptation and record the original PR/SHA. Explain missing or unnecessary backports. Keep platform dependencies, lockfiles, packaging, permissions and workflows in their own branch.

Use `platform:desktop`, `platform:fnos`, `platform:shared` and `needs-sync` labels to track platform impact and outstanding backports. Every PR must pass its own platform checks. Force pushes and deletion of product branches are prohibited.

## fnOS releases

Only x86 FPKs are built in GitHub Actions. Local ARM packaging remains available but is not a release requirement. A successful push to `fnos/develop` publishes a development prerelease named `fnos-dev-<SHA>-<run_id>`. This unique tag does not increment the FPK's internal version or promise same-version upgrades.

For a stable release, bump package/lock, backend/CLI Cargo versions and packaging manifest consistently, verify upgrade compatibility, and merge through the protected branch. Create an immutable `fnos-vX.Y.Z` tag on that reviewed commit, then run **Request fnOS release** from `main`. The workflow verifies ancestry and versions and releases only the tested artifacts from its own run. Never move a release tag or overwrite existing assets.

All fnOS releases explicitly opt out of GitHub Latest. Desktop auto-update depends on the repository's Latest desktop release and `latest-v2.json`; do not mark a fnOS release as Latest manually. Find fnOS packages by their `fnos-` tags in [Releases](https://github.com/Zleap-AI/SAG-Wiki/releases).

The desktop signing Environment remains `release`; fnOS uses `fnos-release` with no desktop signing keys. CI success does not substitute for x86 device install/upgrade and data-preservation acceptance.

## Desktop PR checks

Pull requests to `main` require `E2E macos` and `E2E windows`. These run the real CLI/service indexing and retrieval flow against a local Jev protocol fixture. They compile only the CLI test executable, not the desktop application or installers; they do not exercise the desktop GUI or real model inference.

Desktop builds, unit/component tests, documentation builds, installer smoke tests and release validation remain in `Desktop build and release`, triggered after pushes to `main` or by manual dispatch. Keep the required status check names in branch protection aligned with `pr-e2e.yml`.
