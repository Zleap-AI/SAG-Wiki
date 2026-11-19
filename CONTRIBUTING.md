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

Pull requests to `main` require `E2E macos` and `E2E windows`. The test plan
compares the base commit with the merge candidate, counting both sides of a
rename. Documentation-only changes complete these checks on Linux without
compiling the CLI. Frontend changes run the frontend suite and browser fixture. The Linux test
plan job builds the browser workspace once for both native E2E jobs.
Changes to Rust core, client, CLI or protocol run the relevant service and CLI
fixtures. Shared build inputs and unfamiliar paths select all suites. A failed
plan fails both required checks.

The Jev, HTTP API and browser fixtures each have a separate temporary data
directory and select ephemeral ports. At most two run together per platform.
The PR Jev fixture covers the base endpoint and a representative full endpoint.
Packaged release checks retain the complete endpoint matrix. CLI and client
tests keep their existing outer fixture limit. These checks use
local model fixtures and compile the CLI, without building installers or
exercising the desktop GUI or real model inference.

Desktop builds, the complete native test suite, installation smoke tests and
release validation run in `Desktop build and release`. Native result reuse
checks the embedded prompts and fixtures alongside Rust inputs. Top-level
frontend test files are covered by the frontend suite.

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
Native Rust tests are selected per Cargo package: protocol, client, core, CLI,
and desktop. Reuse requires matching suite inputs, including dependent Rust
packages, fixtures, dependencies, compiler configuration and platform mappings,
and proof that the suite ran successfully on all three platforms for a published
ancestor. A skipped suite in a newer release does not count as fresh evidence.
If no matching verified execution is found, run that suite on all three platforms.
Only desktop build and test jobs install the JavaScript workspace. Production
CLI compilation uses the portable web artifact from the current release run.
The Intel CLI compiler runs on Apple Silicon with an explicit x86_64 target.
It verifies Rosetta and checks the CLI and QPDF architectures before uploading
the input. Intel desktop packaging, installer validation and signing still run
on Intel runners. The compiler host forms part of the dependency cache key. Only automatic CLI/desktop version tokens
are ignored. This includes the model-capability and SAG-tool JSON embedded by Rust;
add any new external compile-time inputs to the selector and its regression tests.
Changed or unverifiable inputs run the affected suites on native runners. Reused
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

Use `platform=macos-intel` and `comparison=core-debug-only` to compare the
desktop baseline with debug metadata disabled only for `sag-core` on an Intel
runner. Both profiles configure the same third-party dependencies and run the
same named tests. Check paired build-plus-test time and retain the Cargo timing
reports to identify the compilation unit responsible for any difference.

Three paired Intel runs supported disabling debug information for `sag-core`
in the Intel Core and CLI test suites. Both suites pass the same Cargo package
override so the CLI suite can reuse the core library. Other crates and platforms
retain debug level 1 and the nonincremental CGU16 baseline. The Intel change
reduces source detail in core test backtraces. It applies only to the test
profile and keeps assertions and overflow checks enabled. Production release
LTO, codegen units and optimization settings remain unchanged.

Run `Rust test profile A/B` on a branch with `platform=macos-intel` and
`comparison=intel-core-cgu` to compare the current package-scoped debug=0
baseline with `sag-core` CGU32 and CGU64. The experiment rotates the profiles
over three paired rounds, runs the same named Core tests and then the two
release CLI test commands. Compare the paired Core-plus-CLI wall times from
`scripts/test-profile-ab.mjs summarize`. In [run 44503310286](https://github.com/Zleap-AI/SAG-Wiki/actions/runs/44503310286),
CGU32 improved all three paired Core-plus-CLI times by 80, 93, and 251 seconds.
CGU64 regressed one round by 176 seconds. Intel Core and CLI now use CGU32
only for `sag-core` in the test profile. The baseline remains CGU16 for future
comparisons. Benchmark runs create neither releases nor trusted checkpoints.

The draft release job reports per-job GitHub queue and execution time, and
reports checkpoint waiting separately from artifact download and validation.
Publication uses the verified checksum asset ID and the exact draft release ID.
A transient failure may be retried within the same publication lock after checking
the draft state. Source, checksum, signing and platform checks still gate publication.

For an internal/build-only PR that does not need its own release, maintainers may add `release:skip` before merging. PR checks remain required. Automatic preparation checks merged PR associations for every product commit in the captured main push (merge, squash or rebase); all must have this label to skip tag creation and publication. Mixed pushes containing an ordinary feature PR still release. API errors stop preparation without creating a tag. This is a per-push decision: a later feature release includes the accumulated changes. Labels are read when the workflow runs; adding one after preparation has started does not cancel an existing release.

If GitHub has not yet indexed the merged PR, or a product push has no uniquely verifiable merged PR (for example a direct push), preparation stops without creating a tag. Retry after indexing; explicit manual releases remain available.
