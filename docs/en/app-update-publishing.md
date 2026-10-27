# In-app update publishing

[简体中文](../zh-CN/应用内更新发布.md) · [Documentation index](README.md)

Official desktop releases use one protected GitHub Actions signing environment. Developers can build and test locally without the release private key. The client embeds the public key, downloads an update, verifies its signature and signed version, then offers **Restart and update**. Update signing is separate from Apple notarization and Windows code signing.

## One-time signing setup

The repository administrator must configure the `release` Environment before the first signed release:

- Allow deployment from version tags matching `v*` only; the workflow also validates the exact `vX.Y.Z` format and that the tagged commit belongs to `main`.
- Require a release reviewer and prevent the person starting the deployment from approving it.
- Store `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` as **Environment secrets**. Workflow YAML contains only Secret references; private values must never enter Git, logs, artifacts or installers.
- Keep an independent, encrypted recovery backup of the private key and password under the organization's access controls. GitHub Secrets is the release runtime store, not the only recovery copy.

Generate a key pair once in a secure administrator session if there is no existing release key:

```sh
node scripts/generate-updater-key.mjs --output /secure/signing-recovery
```

The script refuses repository paths and existing output directories. It creates an encrypted `updater.key`, separate `updater.password`, and public key file without placing the password in process arguments or shell history. Directory permissions are `700`, files `600`. Transfer recovery material to the organization's approved vault. Commit only the matching public key in `src-tauri/tauri.conf.json`. Future releases reuse this pair; a new version does not need a new key.

With the private key and its password available as restricted local files, run:

```sh
node scripts/configure-release.mjs \
  --key-file /secure/updater.key \
  --password-file /secure/updater.password \
  --reviewer luoshuai990529
```

This administrator operation configures environment protection and uploads the secret values from files without printing them. A repository Maintain role is insufficient to configure protected environments. If GitHub returns 403, an administrator must complete this step; a successful local build does not establish that the signing environment is ready.

In **Settings → Environments → release**, the administrator should also uncheck **Allow administrators to bypass configured protection rules**. GitHub's public REST API does not expose this switch, so inspect it in the UI. The automated preflight below checks reviewers, prevention of self-approval, and tag restrictions.

The release job checks environment protection before signing. You can also inspect it with:

```sh
node scripts/release.mjs verify-environment
```

The configured single-maintainer policy lets `luoshuai990529` request a release and manually approve the bot-started signing job. Keep prevention of self-approval enabled: the deployment initiator is the bot. This is one human confirmation, not independent two-person review. For two-person review, configure a separate reachable maintainer as reviewer.

## Apple credentials: administrator provisioning

The credential owner and repository administrator provision Apple credentials directly in **Zleap-AI/SAG-Wiki → Settings → Environments → release → Environment secrets → Add Secret**. Use the company publishing identity approved for Wiki; prefer credentials managed and rotated independently for Wiki. The signing workflow does not retrieve or transfer credentials from another repository.

1. The credential owner provides a valid **Developer ID Application** signing identity as a password-protected `.p12` containing the certificate and its private key. A public `.cer` alone cannot sign. Recover the complete identity from the approved vault or original signing machine; if unavailable, the Apple account owner must arrange a new identity. See [Apple's export instructions](https://developer.apple.com/documentation/Xcode/sharing-your-teams-signing-certificates).
2. Prepare the certificate's Base64 text and export password, an Apple account with access to the matching developer team, and that team's ID. Generate a dedicated app-specific password labelled for Wiki at **account.apple.com → Sign-In and Security → App-Specific Passwords**. This is not the Apple account's login password. See [Apple's app-specific password instructions](https://support.apple.com/en-us/102654).
3. A Wiki repository administrator adds the following five **Environment secrets** to the existing `release` environment:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE_BASE64` | Base64 of the Developer ID Application `.p12`, including its private key. |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password. |
| `APPLE_ID` | The Apple account email used for notarization. |
| `APPLE_APP_SPECIFIC_PASSWORD` | The app-specific password generated for Wiki. |
| `APPLE_TEAM_ID` | The Apple developer Team ID matching the certificate and account access. |

Keep the existing two `TAURI_SIGNING_*` secrets and the environment's required reviewers, prevention of self-approval, disabled administrator bypass and `v*` tag restrictions. Do not recreate the environment to add credentials. Private values go directly from the authorized administrator into GitHub's secret settings; never put them in source files, chat, issues, artifacts or logs. Keep an independent recovery copy in the organization's approved vault. The macOS signing job imports the identity into a temporary keychain and cleans it up afterward.

After setup, report only that the five names have been configured. Real signing, notarization and final-package acceptance are deferred until credentials are ready and the corrected workflow is available. Adding secrets or rerunning the old workflow does not notarize existing installers. Provisioning does not approve a version change, tag or Release: publication remains blocked until both platforms pass formal-signature acceptance and the user explicitly approves the release operation.

## Windows formal signing is pending

The Windows signing provider and resources are still being checked; no working signing service is established here. The protected `windows-verify` job **verifies only** and does not sign files. It intentionally rejects the current unsigned build until an approved signing provider is connected.

The NSIS installer, installed desktop executable, bundled CLI and NSIS uninstaller must each have a valid **embedded Authenticode signature**, the approved `WINDOWS_SIGNING_PUBLISHER`, and a trusted timestamp. The release assembler also requires proof from this run's successful protected Windows verification job, bound to the installer bytes and source. Updater signatures do not satisfy these checks. No next release is ready until real macOS signing/notarization and Windows formal-signature verification both pass.

## Build and release flow

1. Obtain separate user approval before a version PR or bump. Then update desktop and CLI versions, lock files and Changelog in the approved PR, and merge only after review and unsigned CI checks pass. This signing infrastructure change includes no version bump or version PR.
2. **Desktop build and release** runs full macOS and Windows desktop builds and tests for PRs, pushes to `main`, and manual dispatches: source validation, documentation builds, frontend and native tests, desktop packaging, macOS bundled CLI checks, and Windows installation smoke checks plus installed-CLI Jev E2E. The existing **PR E2E** also continues its lightweight CLI protocol regression on both platforms. Ordinary builds remain macOS ad-hoc signed and Windows unsigned, receive no release secrets, and retain the full regression coverage. These checks still do not replace representative upgrade and data-preservation acceptance.
3. After the signing resources are connected and the user explicitly approves the tag and release operation, create `vX.Y.Z` at the validated `main` commit, then run **Request desktop release** from `main` with that tag. Pushing a tag alone does not start signing. The request dispatches the tag workflow as `github-actions[bot]`; it never creates or moves tags. The workflow rejects mismatched versions and tags outside `main`.
4. After both platform builds pass, tag releases enter the `macos-sign` and `windows-verify` jobs protected by the `release` environment and require reviewer approval. The Windows job verifies the four executables and installed-package acceptance described above; it supplies no signing provider. The macOS job finalizes the Apple package. It downloads and verifies the tested artifacts without recompiling. It signs the bundled CLI separately with Developer ID, hardened runtime and a secure timestamp, then signs the outer app. After notarizing and stapling the app, it recreates, signs, notarizes and staples the DMG. Signature checks, Gatekeeper assessment, DMG/updater payload parity and final bundled CLI acceptance must pass before new signed macOS artifacts are uploaded.
5. The subsequent protected `release` job requires both jobs to succeed. It revalidates the original source, accepts the final macOS archive only from this run's successful `macos-sign` job, and requires the current protected Windows job's signature proof. It checks artifact digests, source relationships, commit and version before updater-signing the accepted packages and generating checksums/manifests. No original unsigned Windows installer can bypass the proof. Updater signing replaces neither Apple notarization nor Authenticode.
6. The workflow creates a **draft** GitHub Release with installers, signed update packages, manifests and build provenance. It does not automatically promote a partially built release.

Actions are pinned to full commit SHAs. Checkout does not persist credentials. Build jobs have read-only repository permissions; signing, publishing and provenance permissions are limited to the jobs that need them. Dependencies use committed npm and Cargo lock files. Do not overwrite published artifacts to repair a failed build; fix the source and release a higher version.

Inspection of historical installers confirms that **0.1.5 and 0.1.6 are notarized and accepted by Gatekeeper; 0.1.9 is ad-hoc signed, has no stapled tickets and is rejected by Gatekeeper.** Do not characterize every earlier release as unnotarized. The publicly published 0.2.4 installers remain unchanged, including their lack of Apple notarization and Windows formal signing. Those historical assets do not authorize another unsigned release. Apple credential setup and real signing/notarization validation are pending and the Windows signing provider is pending; a new version remains blocked until both actual platform checks pass.

Only after the separate version/tag/release approvals and signing readiness above, use the reviewed, clean `main` checkout:

```sh
git fetch origin main --tags
node scripts/release.mjs validate
release_commit=$(git rev-parse origin/main)
test "$(git rev-parse HEAD)" = "$release_commit" || exit 1
release_version=$(node -p "require('./package.json').version")
release_tag="v${release_version}"
git tag -a "$release_tag" "$release_commit" -m "SAG Wiki $release_version"
git push origin "refs/tags/$release_tag"
gh workflow run request-release.yml --repo Zleap-AI/SAG-Wiki --ref main -f tag="$release_tag"
```

If the tag already exists, keep it unchanged and run only the request command. Inspect the resulting run actor before approval; it must be `github-actions[bot]`.

The macOS `.dmg` is for manual installation; its `.app.tar.gz` is the updater payload. Windows uses the NSIS `.exe` for both purposes. The central signing step generates `.sig` files with the release version included in the signed comment. `requireSignedVersion` remains enabled.

## Build reuse and compilation caches

Tag releases first look for a successful push build on `main` from the exact same commit and the same repository/workflow. Both platform jobs, source validation and documentation must have passed in their latest respective attempts, and both artifacts must still be available. When found, the release skips duplicate original builds and tests. Otherwise it runs the full build. Lookup API failures also fall back to a full build; normal main builds do not query the reuse API. The `main` cache contains ordinary macOS ad-hoc and Windows unsigned build artifacts. Reuse never satisfies the publication gates: every tag release must complete fresh `macos-sign` finalization, notarization and acceptance, plus the protected `windows-verify` checks after a signing provider has been connected.

Before signing, the source run and jobs are checked again. Each archive is downloaded by artifact ID, verified against GitHub's SHA256 digest, and checked against its internal commit, version, platform and file digests. Signed macOS artifacts are kept separate from the original cache, with metadata recording original file digests and Apple signing information. The central release job verifies they came from this run's successful signing job, requires this run's protected Windows verification proof, and rechecks file digests and source relationships before updater signing. Expired or missing artifacts after selection stop publication; rerun the workflow to resolve a new source or rebuild. The protected `release` environment and signed-version enforcement remain required.

Rust caches are isolated by runner OS/architecture, target, Rust version, manifests, lockfiles and workflow content. Only trusted `main` push builds save them. Debug outputs are saved after native tests and removed before restoring release outputs to limit disk use. Cache hits accelerate compilation; they do not skip tests. Signing jobs do not restore compilation caches or save key material.

## Review and promote the draft

Before publishing, obtain explicit user approval and confirm full builds and regressions on both platforms, the Windows NSIS installation smoke check and bundled CLI E2E passed. Windows must also pass embedded Authenticode, approved publisher and trusted timestamp checks for the installer, desktop executable, CLI and NSIS uninstaller, with proof from the current protected verification job. macOS must also pass Developer ID/hardened runtime/timestamp verification for the final app and CLI, app/DMG notarization and staple validation, Gatekeeper assessment, payload parity between the mounted DMG and extracted updater, and actual bundled CLI acceptance. Confirm both platform entries and updater signatures exist. Download draft assets and verify their SHA256 checksums and build provenance. For a downloaded artifact:

```sh
gh attestation verify /path/to/downloaded-installer -R Zleap-AI/SAG-Wiki
```

Checksums establish byte identity; an attestation identifies build source and workflow. Neither replaces signature validation or an installation check. Hosted-runner smoke checks do not establish acceptance on every supported customer machine.

After an authorized release reviewer inspects the assets and release notes, reuse the reviewed `release_tag` from the creation step. In a new shell, explicitly set it to that reviewed tag; do not derive it again from a potentially changed checkout:

```sh
: "${release_tag:?Set release_tag to the reviewed release tag before publishing}"
gh release view "$release_tag" --repo Zleap-AI/SAG-Wiki
gh release edit "$release_tag" --repo Zleap-AI/SAG-Wiki --draft=false --latest
```

After promotion, verify public installer URLs and update manifests. README links to the latest stable Release, and the installation page reads its version, download URLs and actual asset sizes when opened. No version configuration or download-documentation PR is required.

## 0.1.9 key migration

Version 0.1.9 introduces a new updater key because the prior private key was unavailable. **Users on 0.1.8 or older clients that trust the previous key must manually download and install 0.1.9 once.** The old public key cannot validate packages signed by the new key. After this installation, future versions signed with the new key can use the in-app updater.

| Manifest | Intended clients | Behavior |
| --- | --- | --- |
| `latest-v2.json` | 0.1.9 and later with the new public key | Lists the current release and new signatures. |
| `latest.json` | Clients with the old public key | Preserves the original 0.1.8 manifest, URLs and signatures verbatim. A 0.1.8 client reports no newer version; older compatible clients may still install 0.1.8. |

Each latest Release retains the legacy `latest.json` so old clients do not encounter a missing manifest or an untrusted update. Do not relabel the old manifest as 0.1.9, replace old signatures, or disable verification. Release notes must state that old users need one manual upgrade; the retained legacy manifest does not notify them about 0.1.9.

Existing GitHub repository settings continue to work: the new binary selects `latest-v2.json` for the configured repository. A custom **Website** source keeps its explicit URL. Its operator must serve the new manifest at a separate URL and update that setting for migrated clients, while keeping the old channel intact where required.

## Failure handling and validation limits

- Missing signing resources or secrets, insufficient environment protection, mixed source commits, a missing platform package, failed Apple signing/notarization/staple/Gatekeeper checks, an inconsistent final macOS package, any missing or invalid Windows embedded signature/publisher/timestamp, missing current Windows verification proof, a corrupt updater signature or a signed-version mismatch must stop release creation. Do not fall back to unsigned platform builds or updater packages.
- A valid newer manifest can display an update notification before package signature verification. The client refuses an untrusted package during download/verification and keeps the installed version.
- Frontend tests cover progress, failures and retries, unsaved-document protection and repeated clicks. Native tests download signed fixtures over local HTTP, reject corrupt content and replace only temporary application files.
- The Jev E2E uses the actual service, CLI, persisted settings, indexing and retrieval against a loopback protocol fixture. It verifies integration, not real Jev inference quality or remote deployment availability.
- Closing settings does not interrupt an update download; quitting the app requires downloading again. Updates replace program files without intentionally clearing knowledge bases, conversations or original files. Keep normal user-data backups and validate representative upgrade installations before publishing.
