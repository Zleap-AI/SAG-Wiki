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

## Manually configure Apple signing and notarization secrets

Configure Apple credentials under **Zleap-AI/SAG-Wiki → Settings → Environments → release**, the same protected environment used for updater signing. Secrets in SAG's `desktop-release` environment are not automatically available to Wiki. GitHub cannot reveal an existing Secret's value. Use the original credential backup or export the certificate again from the Mac that holds its private key. Keep the existing reviewers, prevention of self-approval, disabled administrator bypass, and `v*` tag restrictions.

1. Obtain a **Developer ID Application** `.p12` backup containing both the certificate and its private key. A public `.cer` certificate alone cannot sign an app. Without the original `.p12`, open **Keychain Access → login → My Certificates** on the original Mac, find the Developer ID Application entry and confirm it has a private key, then export it as `.p12` with a securely stored export password. Do not substitute an Apple Development or App Store distribution certificate.
2. On that Mac, encode the `.p12` as one Base64 line directly to the clipboard. Replace the sample path with your certificate's path. This command does not print its contents:

   ```sh
   openssl base64 -A -in /secure/DeveloperID-Application.p12 | pbcopy
   ```

3. Select **Add environment secret** in Wiki's `release` environment. Name it `APPLE_CERTIFICATE_BASE64`, paste the clipboard contents as its value, and save. Then clear the clipboard:

   ```sh
   printf '' | pbcopy
   ```

4. Add `APPLE_CERTIFICATE_PASSWORD` with that `.p12` export password. If the password is lost, export again from the original Mac holding the private key; GitHub cannot recover it.
5. Add `APPLE_ID` with the Apple account email authorized to submit notarization for this developer team.
6. Sign in to [account.apple.com](https://account.apple.com/), open **Sign-In and Security → App-Specific Passwords**, and generate a password for Wiki releases. Store it as `APPLE_APP_SPECIFIC_PASSWORD`. Use this generated app-specific password rather than the account's normal sign-in password.
7. Find the Team ID in your [Apple Developer account's Membership details](https://developer.apple.com/account/#/membership) and store it as `APPLE_TEAM_ID`. It must match the team of the Developer ID Application certificate.

The five Apple Environment secrets use the same names as SAG's release workflow. Retain the two existing `TAURI_SIGNING_*` secrets:

| Secret | Value |
| --- | --- |
| `APPLE_CERTIFICATE_BASE64` | Single-line Base64 of the `.p12` containing its private key. |
| `APPLE_CERTIFICATE_PASSWORD` | The `.p12` export password. |
| `APPLE_ID` | The Apple account email used for notarization. |
| `APPLE_APP_SPECIFIC_PASSWORD` | The app-specific password generated for notarization. |
| `APPLE_TEAM_ID` | The Apple developer Team ID matching the certificate. |

Handle values only in GitHub's Secret input and the organization's approved credential vault. Do not paste them into chat, issues, source files or logs. The workflow imports the certificate into a temporary keychain with a password generated for that job and cleans it up afterward. Adding Secrets does not sign existing installers; the first new release still requires environment approval and actual signing, notarization and installation acceptance.

## Build and release flow

1. Update desktop and CLI versions, lock files and Changelog in a PR. Merge the PR into `main` only after review and unsigned CI checks pass.
2. **Desktop build and release** runs full macOS and Windows desktop builds and tests for PRs, pushes to `main`, and manual dispatches: source validation, documentation builds, frontend and native tests, desktop packaging, macOS bundled CLI checks, and Windows installation smoke checks plus installed-CLI Jev E2E. The existing **PR E2E** also continues its lightweight CLI protocol regression on both platforms. Ordinary builds use macOS ad-hoc signatures and receive no release secrets. These checks still do not replace representative upgrade and data-preservation acceptance.
3. Create `vX.Y.Z` at the validated `main` commit, then run **Request desktop release** from `main` with that tag. Pushing a tag alone does not start signing. The request dispatches the tag workflow as `github-actions[bot]`; it never creates or moves tags. The workflow rejects mismatched versions and tags outside `main`.
4. After both platforms pass, tag releases enter the `macos-sign` job protected by the `release` environment and require reviewer approval. It downloads and verifies the tested artifacts without recompiling. It signs the bundled CLI separately with Developer ID, hardened runtime and a secure timestamp, then signs the outer app. After notarizing and stapling the app, it recreates, signs, notarizes and staples the DMG. Signature checks, Gatekeeper assessment, DMG/updater payload parity and final bundled CLI acceptance must pass before new signed macOS artifacts are uploaded.
5. The subsequent protected `release` job revalidates the original build source and accepts a macOS archive only from this workflow run's successful `macos-sign` job. It verifies GitHub's artifact digest, the relationship between original and signed artifact digests, commit and version before updater-signing the final macOS archive and existing Windows installer and generating checksums/manifests. Updater signing does not replace Apple notarization.
6. The workflow creates a **draft** GitHub Release with installers, signed update packages, manifests and build provenance. It does not automatically promote a partially built release.

Actions are pinned to full commit SHAs. Checkout does not persist credentials. Build jobs have read-only repository permissions; signing, publishing and provenance permissions are limited to the jobs that need them. Dependencies use committed npm and Cargo lock files. Do not overwrite published artifacts to repair a failed build; fix the source and release a higher version.

**Published macOS artifacts through 0.2.4 remain unnotarized.** This workflow does not modify them; release a higher version to provide artifacts accepted by the new pipeline. Windows code signing remains unconfigured, with the NSIS installer retaining its current unsigned status and full installation smoke checks plus installed-CLI E2E.

For example, from the reviewed, clean `main` checkout:

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

Tag releases first look for a successful push build on `main` from the exact same commit and the same repository/workflow. Both platform jobs, source validation and documentation must have passed in their latest respective attempts, and both artifacts must still be available. When found, the release skips duplicate original builds and tests. Otherwise it runs the full build. Lookup API failures also fall back to a full build; normal main builds do not query the reuse API. The `main` cache always contains artifacts without production Apple signing or notarization. Every tag release must run fresh `macos-sign` finalization, notarization and acceptance, including when the original build is reused.

Before signing, the source run and jobs are checked again. Each archive is downloaded by artifact ID, verified against GitHub's SHA256 digest, and checked against its internal commit, version, platform and file digests. Signed macOS artifacts are kept separate from the original cache, with metadata recording original file digests and Apple signing information. The central release job verifies they came from this run's successful signing job and rechecks their file digests and source relationship before updater signing. Expired or missing artifacts after selection stop publication; rerun the workflow to resolve a new source or rebuild. The protected `release` environment and signed-version enforcement remain required.

Rust caches are isolated by runner OS/architecture, target, Rust version, manifests, lockfiles and workflow content. Only trusted `main` push builds save them. Debug outputs are saved after native tests and removed before restoring release outputs to limit disk use. Cache hits accelerate compilation; they do not skip tests. Signing jobs do not restore compilation caches or save key material.

## Review and promote the draft

Before publishing, confirm full builds and regressions on both platforms, the Windows NSIS installation smoke check and bundled CLI E2E passed. macOS must also pass Developer ID/hardened runtime/timestamp verification for the final app and CLI, app/DMG notarization and staple validation, Gatekeeper assessment, payload parity between the mounted DMG and extracted updater, and actual bundled CLI acceptance. Confirm both platform entries and updater signatures exist. Download draft assets and verify their SHA256 checksums and build provenance. For a downloaded artifact:

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

- Missing signing secrets, insufficient environment protection, mixed source commits, a missing platform package, failed Apple signing/notarization/staple/Gatekeeper checks, a final macOS package inconsistent with its source, a corrupt updater signature or a signed-version mismatch must stop release creation. Do not fall back to the original macOS build or unsigned updater packages.
- A valid newer manifest can display an update notification before package signature verification. The client refuses an untrusted package during download/verification and keeps the installed version.
- Frontend tests cover progress, failures and retries, unsaved-document protection and repeated clicks. Native tests download signed fixtures over local HTTP, reject corrupt content and replace only temporary application files.
- The Jev E2E uses the actual service, CLI, persisted settings, indexing and retrieval against a loopback protocol fixture. It verifies integration, not real Jev inference quality or remote deployment availability.
- Closing settings does not interrupt an update download; quitting the app requires downloading again. Updates replace program files without intentionally clearing knowledge bases, conversations or original files. Keep normal user-data backups and validate representative upgrade installations before publishing.
