# Contributing

This repository maintains the Windows and macOS desktop versions of SAG Wiki on `main`. Create a `feat/desktop-*` branch from `main` and target `main` when opening a pull request.

The fnOS version is maintained separately in the GitLab `zleap/data/sag-wiki-fnos` project. Shared fixes may be adapted between repositories with `git cherry-pick -x` or a separate implementation that records the source commit. Keep platform-specific packaging, permissions and release configuration in the relevant repository; do not automatically merge the two product histories.

## Desktop releases

Desktop release tags use `vX.Y.Z`. The signing Environment remains `release`. Desktop auto-update depends on the repository's Latest desktop release and `latest-v2.json`; keep release assets and update metadata consistent.

Every pull request must pass the desktop checks. Force pushes and deletion of `main` are prohibited. CI success does not replace Windows/macOS installation, upgrade and data-preservation acceptance.

## Desktop PR checks

Pull requests to `main` require `E2E macos` and `E2E windows`. These run the real CLI/service indexing and retrieval flow against a local Jev protocol fixture. They compile only the CLI test executable, not the desktop application or installers; they do not exercise the desktop GUI or real model inference.

Desktop builds, unit/component tests, documentation builds, installer smoke tests and release validation remain in `Desktop build and release`, triggered after pushes to `main` or by manual dispatch. Keep the required status check names in branch protection aligned with `pr-e2e.yml`.
