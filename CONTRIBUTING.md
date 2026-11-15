# Contributing

This repository maintains the Windows and macOS desktop versions of SAG Wiki on `main`. Create a `feat/desktop-*` branch from `main` and target `main` when opening a pull request.

The fnOS version is maintained separately in the GitLab `zleap/data/sag-wiki-fnos` project. Shared fixes may be adapted between repositories with `git cherry-pick -x` or a separate implementation that records the source commit. Keep platform-specific packaging, permissions and release configuration in the relevant repository; do not automatically merge the two product histories.

## Desktop releases

Desktop release tags use `vX.Y.Z`. The signing Environment remains `release`. Desktop auto-update depends on the repository's Latest desktop release and `latest-v2.json`; keep release assets and update metadata consistent.

Every pull request must pass the desktop checks. Force pushes and deletion of `main` are prohibited. CI success does not replace Windows/macOS installation, upgrade and data-preservation acceptance.

## Desktop PR checks

Pull requests to `main` require `E2E macos` and `E2E windows`. These run the real CLI/service indexing and retrieval flow against a local Jev protocol fixture. They compile only the CLI test executable, not the desktop application or installers; they do not exercise the desktop GUI or real model inference.

Desktop builds, unit/component tests, documentation builds, installer smoke tests and release validation remain in `Desktop build and release`, triggered after pushes to `main` or by manual dispatch. Keep the required status check names in branch protection aligned with `pr-e2e.yml`.

## CI resource use

On Windows, run desktop Rust tests with
`pwsh -NoProfile -File scripts/test-windows-desktop.ps1`. The locked Tauri
build helper links its Common Controls v6 manifest into application binaries
only. The wrapper compiles the test harnesses, embeds the manifest into exactly
the test executables reported by Cargo, and runs the complete desktop test suite.
This prevents `STATUS_ENTRYPOINT_NOT_FOUND` for `comctl32!TaskDialogIndirect`
without modifying application binaries or skipping tests. It requires the
Windows SDK manifest tool (`mt.exe`). macOS uses the regular `cargo test` command.

Frontend unit tests run once on Linux in the main desktop workflow. Both native
platforms retain Rust tests, installer builds and real packaged/installed CLI
checks. macOS signing starts after its verified installer build, while Rust tests and
Windows work continue. Publication still requires the exact commit whose
frontend, documentation and both platforms' tests and builds succeeded.

CI sets `CARGO_TARGET_DIR` to `src-tauri/target` in each worker. CLI and desktop
production compilation run on independent workers for each platform, then join
before packaging. Desktop compilation temporarily omits resource copying while
preserving its original embedded configuration; packaging restores the actual,
verified CLI and QPDF resources. No placeholder executable is accepted. Each
platform waits only for its own CLI producer. Standalone local builds retain
their existing commands and output locations. Rust dependency caches are partitioned by OS, architecture, target,
toolchain, profile, manifests, lockfiles and Cargo configuration, rather than
commit SHA or workflow text. Trusted main dispatches save ordinary Actions
caches; PR E2E restores the identical debug cache without writing it. Because
Actions caches are tag-scoped, successful tag jobs also export dependency-only
artifacts for subsequent tags. Consumers verify the repository, workflow, live
tag and main ancestry (including deterministic release children), latest passed
validation/native job attempt, artifact lifetime and digest, and payload
metadata before extraction. Compatibility ignores only local SAG package version
changes; third-party versions, features, profiles, Cargo configuration, platform
and exact Rust compiler remain part of the key. CLI/desktop compilation caches
are partitioned by role, and test debug/codegen settings are part of compatibility. App/test executables and local
SAG libraries are excluded. An unavailable or rejected cache falls back to
compilation and never substitutes for tests or installer validation. Artifacts
expire after 14 days; a compatible hit avoids exporting the same dependencies
again. Feature-branch dispatches exercise a local archive round trip and upload
only `validation-rust-deps-*` artifacts, which trusted consumers reject. Export
and upload are bounded, optional steps; include their overhead in CI timings.

Tested main installers remain available for 14 days so a release can reuse them;
Safari development installers and signed intermediate artifacts are retained for
3 days. Public Release assets are unchanged.

Dependency archives use the runner's existing zstd with multithreaded compression and streaming tar I/O. Restore validates the complete staged content and decompressor result before atomically replacing dependency files. Cache failure remains a normal-compilation fallback.

The manual `Rust test profile A/B` workflow compares debug=1/CGU=16,
line-tables-only/CGU=16 and line-tables-only/CGU=64 on the same runner in rotated
order, with three paired rounds per platform. Each profile warms matching
dependencies before measuring local core compilation. Reports include Cargo
unit timings, warm-up and execution times, and named-test/ignored inventories;
all core tests execute. Assertions and overflow checks remain enabled, and
production release profiles are independent. Compare matched medians rather
than treating a single hosted-runner measurement as a release-speed guarantee.

PR E2E uses the same `RUST_TEST_THREADS=2` outer fixture budget as Desktop.
Individual concurrency scenarios retain all client processes, including the
20-process standalone test. This controls CI resource contention without
changing the production service startup deadline. Failed CLI assertions include
child exit status, stdout and stderr for diagnosis.

For a focused code-generation comparison, dispatch that workflow with
`comparison=cgu-only` and `platform=macos`: debug stays at 1 and only CGU16/64
changes. The default `debug-and-cgu` comparison covers both debug metadata and
codegen candidates. Use three paired rounds before adopting a setting; a lower
single-run time is insufficient evidence. Benchmark artifacts never substitute
for native release checks.

The measured candidates have not established a material, stable net reduction
in core compilation plus execution. Desktop native tests therefore retain
`CARGO_PROFILE_TEST_DEBUG=1` and the existing nonincremental CGU16 baseline;
release LTO, codegen units and optimization settings remain unchanged. Evaluate
both compile and test execution costs rather than selecting by compiler time
alone.
