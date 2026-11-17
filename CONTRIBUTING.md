# Contributing

This repository maintains the Windows and macOS desktop versions of SAG Wiki on `main`. Create a `feat/desktop-*` branch from `main` and target `main` when opening a pull request.

The fnOS version is maintained separately in the GitLab `zleap/data/sag-wiki-fnos` project. Shared fixes may be adapted between repositories with `git cherry-pick -x` or a separate implementation that records the source commit. Keep platform-specific packaging, permissions and release configuration in the relevant repository; do not automatically merge the two product histories.

## Desktop releases

Before merging a product PR, append user-facing Chinese bullets and matching English bullets to `CHANGELOG.md` under an explicit version heading, such as `## 0.3.4`. Use one single-line `- ` bullet per change, with Chinese first and `### English` before the matching English bullets in the same order. Keep published entries under their actual release versions; do not use a pending-release placeholder heading. PR checks reject missing, unchanged or placeholder release notes for product changes; explicitly labeled `release:skip` internal PRs are exempt. Documentation, CI and release-tooling-only PRs need no product notes. `CHANGELOG.md` is the only editable source for version history. Run `npm --prefix docs-site run changelog:generate` to regenerate both localized changelogs; never edit their generated content directly. `npm --prefix docs-site run changelog:lint` checks every version in PR CI, including documentation-only PRs. PR checks, new-tag preparation and draft publication reject missing or inconsistent entries; a successful documentation build alone is insufficient.

Each functional change may end with related Issue and PR Markdown links, for example `Fix formula rendering. ([PR #142](https://github.com/Zleap-AI/SAG-Wiki/pull/142))`. Both languages retain these links. Links must accompany a functional description; link-only entries, commit subjects, commit hashes and commit links are not allowed.

Fetch remote version tags before writing notes and use the next version computed by automation as the heading. Resolve version conflicts before merging. Preparation uses the reviewed notes for that version and aligns application, CLI and lockfile versions in a detached release commit without rewriting changelog headings on main. Missing or invalid notes stop preparation before tagging. Updater notes contain only bilingual product changes, without commit subjects, commit references, generated boilerplate or signing-migration prose. GitHub Release descriptions append a download table for Windows x64, Apple Silicon and Intel; the updater notes asset remains product-only. Historical prepared tags remain verifiable.

Desktop release tags use `vX.Y.Z`. The signing Environment remains `release`. Desktop auto-update depends on the repository's Latest desktop release and `latest-v2.json`; keep release assets and update metadata consistent.

Every pull request must pass the desktop checks. Force pushes and deletion of `main` are prohibited. CI success does not replace Windows/macOS installation, upgrade and data-preservation acceptance.

## Desktop PR checks

Pull requests to `main` require `E2E macos` and `E2E windows`. These run the real CLI/service indexing and retrieval flow against a local Jev protocol fixture. They compile only the CLI test executable, not the desktop application or installers; they do not exercise the desktop GUI or real model inference.

Desktop builds, unit/component tests, documentation builds, installer smoke tests and release validation remain in `Desktop build and release`, dispatched for prepared release tags or manually. Keep the required status check names in branch protection aligned with `pr-e2e.yml`.

## CI resource use

On Windows, run desktop Rust tests with
`pwsh -NoProfile -File scripts/test-windows-desktop.ps1`. The locked Tauri
build helper links its Common Controls v6 manifest into application binaries
only. The wrapper compiles the test harnesses, embeds the manifest into exactly
the test executables reported by Cargo, and runs the complete desktop test suite.
This prevents `STATUS_ENTRYPOINT_NOT_FOUND` for `comctl32!TaskDialogIndirect`
without modifying application binaries or skipping tests. It requires the
Windows SDK manifest tool (`mt.exe`). macOS uses the regular `cargo test` command.

Frontend assets are built once and frontend unit tests run once on Linux. All
three platforms retain production builds and packaged/installed CLI checks.
Native Rust results may be reused only when native source, fixtures, dependencies,
compiler configuration and platform mappings match a successful published ancestor
with all three native jobs successful. Only automatic CLI/desktop version tokens
are ignored. This includes the model-capability and SAG-tool JSON embedded by Rust;
add any new external compile-time inputs to the selector and its regression tests.
Changed or unverifiable inputs run complete Rust suites on native runners. Reused
results are recorded by lightweight Ubuntu jobs without allocating Macs. Python
and Safari native checks run in the corresponding production build. No required
check or approval is added. Each Mac signer starts after its own build; final
publication still requires all three platforms, frontend, documentation and artifact
checks from the exact release commit. Mac staging contains only the App archive;
the protected signer creates and verifies the final DMG. Legacy two-file Mac
checkpoints remain reusable. Complete Jev E2E runs once on identical signed Mac
binaries, with content, identity, signature and ticket checks retained for both
updater archives and mounted DMGs. Each stable Release must include Windows x64
and both Mac installers; existing assembly and publication requirements remain.

CI sets `CARGO_TARGET_DIR` to `src-tauri/target` in each worker. CLI and desktop
production compilation run on independent workers for each platform, then join
before packaging. Desktop compilation temporarily omits resource copying while
preserving its original embedded configuration; packaging restores the actual,
verified CLI and QPDF resources. No placeholder executable is accepted. Each
platform waits only for its own CLI producer. Standalone local builds retain
their existing commands and output locations. Rust dependency caches are partitioned by OS, architecture, target,
toolchain, profile, manifests, lockfiles and Cargo configuration, rather than
commit SHA or workflow text. Trusted main dispatches save ordinary Actions
caches. PR E2E only restores its separate `debug` compatibility namespace and
never writes it; Desktop uses `debug1`. They do not share fresh snapshots, and
PRs can fall back to a legacy snapshot or normal compilation. Because
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
Signed intermediate artifacts are retained for 3 days. Safari development
installers are built separately when needed, not as part of desktop releases.
Public Release asset names and platform requirements are unchanged.

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

For an internal/build-only PR that does not need its own release, maintainers may add `release:skip` before merging. PR checks remain required. Automatic preparation checks merged PR associations for every product commit in the captured main push (merge, squash or rebase); all must have this label to skip tag creation and publication. Mixed pushes containing an ordinary feature PR still release. API errors stop preparation without creating a tag. This is a per-push decision: a later feature release includes the accumulated changes. Labels are read when the workflow runs; adding one after preparation has started does not cancel an existing release.

If GitHub has not yet indexed the merged PR, or a product push has no uniquely verifiable merged PR (for example a direct push), preparation stops without creating a tag. Retry after indexing; explicit manual releases remain available.
