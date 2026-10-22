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
  --reviewer Zleap-Admin
```

This administrator operation configures environment protection and uploads the secret values from files without printing them. A repository Maintain role is insufficient to configure protected environments. If GitHub returns 403, an administrator must complete this step; a successful local build does not establish that the signing environment is ready.

In **Settings → Environments → release**, the administrator should also uncheck **Allow administrators to bypass configured protection rules**. GitHub's public REST API does not expose this switch, so inspect it in the UI. The automated preflight below checks reviewers, prevention of self-approval, and tag restrictions.

The release job checks environment protection before signing. You can also inspect it with:

```sh
node scripts/release.mjs verify-environment
```

## Build and release flow

1. Update desktop and CLI versions, lock files and Changelog in a PR. Merge the PR into `main` only after review and unsigned CI checks pass.
2. PR jobs build macOS arm64 and Windows x64 installers and the bundled CLI from the same commit, run tests and the local Jev protocol E2E. These jobs receive no signing secrets.
3. Create `vX.Y.Z` at the validated `main` commit. The workflow rejects mismatched versions and tags outside `main`.
4. After both platforms pass, the protected `release` job requires its reviewer. It signs the exact built update packages with Environment secrets, binds signatures to the release version, verifies signatures, checks platform metadata has the same commit/version, and generates checksums and update manifests.
5. The workflow creates a **draft** GitHub Release with installers, signed update packages, manifests and build provenance. It does not automatically promote a partially built release.

Actions are pinned to full commit SHAs. Checkout does not persist credentials. Build jobs have read-only repository permissions; signing, publishing and provenance permissions are limited to the jobs that need them. Dependencies use committed npm and Cargo lock files. Do not overwrite published artifacts to repair a failed build; fix the source and release a higher version.

For example, from the reviewed, clean `main` checkout:

```sh
git fetch origin main --tags
node scripts/release.mjs validate
release_commit=$(git rev-parse origin/main)
git tag -a v0.1.9 "$release_commit" -m 'SAG Wiki 0.1.9'
git push origin refs/tags/v0.1.9
```

The macOS `.dmg` is for manual installation; its `.app.tar.gz` is the updater payload. Windows uses the NSIS `.exe` for both purposes. The central signing step generates `.sig` files with the release version included in the signed comment. `requireSignedVersion` remains enabled.

## Review and promote the draft

Before publishing, confirm both platform jobs, the Windows NSIS installation smoke check and bundled CLI E2E passed, and both platform entries and signatures exist. Download draft assets and verify their SHA256 checksums and build provenance. For a downloaded artifact:

```sh
gh attestation verify /path/to/downloaded-installer -R Zleap-AI/SAG-Wiki
```

Checksums establish byte identity; an attestation identifies build source and workflow. Neither replaces signature validation or an installation check. Hosted-runner smoke checks do not establish acceptance on every supported customer machine.

After an authorized release reviewer inspects the assets and release notes:

```sh
gh release view v0.1.9 --repo Zleap-AI/SAG-Wiki
gh release edit v0.1.9 --repo Zleap-AI/SAG-Wiki --draft=false --latest
```

Check public download URLs and update manifests after promotion. Update README download links and sizes only when new assets are publicly available.

## 0.1.9 key migration

Version 0.1.9 introduces a new updater key because the prior private key was unavailable. **Users on 0.1.8 or older clients that trust the previous key must manually download and install 0.1.9 once.** The old public key cannot validate packages signed by the new key. After this installation, future versions signed with the new key can use the in-app updater.

| Manifest | Intended clients | Behavior |
| --- | --- | --- |
| `latest-v2.json` | 0.1.9 and later with the new public key | Lists the current release and new signatures. |
| `latest.json` | Clients with the old public key | Preserves the original 0.1.8 manifest, URLs and signatures verbatim. A 0.1.8 client reports no newer version; older compatible clients may still install 0.1.8. |

Each latest Release retains the legacy `latest.json` so old clients do not encounter a missing manifest or an untrusted update. Do not relabel the old manifest as 0.1.9, replace old signatures, or disable verification. Release notes must state that old users need one manual upgrade; the retained legacy manifest does not notify them about 0.1.9.

Existing GitHub repository settings continue to work: the new binary selects `latest-v2.json` for the configured repository. A custom **Website** source keeps its explicit URL. Its operator must serve the new manifest at a separate URL and update that setting for migrated clients, while keeping the old channel intact where required.

## Failure handling and validation limits

- Missing signing secrets, insufficient environment protection, mixed source commits, a missing platform package, a corrupt signature or a signed-version mismatch must stop release creation. Do not publish an unsigned fallback as an in-app update.
- A valid newer manifest can display an update notification before package signature verification. The client refuses an untrusted package during download/verification and keeps the installed version.
- Frontend tests cover progress, failures and retries, unsaved-document protection and repeated clicks. Native tests download signed fixtures over local HTTP, reject corrupt content and replace only temporary application files.
- The Jev E2E uses the actual service, CLI, persisted settings, indexing and retrieval against a loopback protocol fixture. It verifies integration, not real Jev inference quality or remote deployment availability.
- Closing settings does not interrupt an update download; quitting the app requires downloading again. Updates replace program files without intentionally clearing knowledge bases, conversations or original files. Keep normal user-data backups and validate representative upgrade installations before publishing.
