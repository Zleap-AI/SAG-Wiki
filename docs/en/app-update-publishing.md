# In-app update publishing

[简体中文](../zh-CN/应用内更新发布.md) · [Documentation index](README.md)

The client downloads and verifies updates in the app, then displays **Restart and update**. Closing settings does not interrupt a download. Quitting the entire app requires downloading again. Updates replace program files while preserving the knowledge base, conversations and original local files.

## Signing keys

The local private key is `releases/updater-signing/sag-wiki.key`. This directory is Git-ignored, and the private key is readable and writable only by the current user. Back it up securely and separately. Do not upload it to GitHub or distribute it with installers. Future releases must use the same signing key. The client public key is stored in `src-tauri/tauri.conf.json`.

Update signing is independent of Apple notarization and does not require an Apple account.

## Build update packages

Provide `TAURI_SIGNING_PRIVATE_KEY` as a private-key path or its contents, and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` when building. The password for the locally generated key is empty. The version must be higher than the published version. `0.1.6` below is an example; replace it consistently for the actual release.

macOS:

```sh
export TAURI_SIGNING_PRIVATE_KEY="$PWD/releases/updater-signing/sag-wiki.key"
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=''
npm run tauri -- build --config src-tauri/tauri.updater.conf.json --config '{"version":"0.1.6"}'
```

On the Windows build machine, securely configure the same private key and combine the Windows and updater configurations:

```text
npm run package:windows -- --config src-tauri/tauri.updater.conf.json --config '{"version":"0.1.6"}'
```

Use the generated `.app.tar.gz` on macOS and the NSIS `.exe` on Windows. The `.dmg` and `.exe` installers remain available for first-time installations.

## Generate release files

After both platform builds finish, run:

```sh
npm run package:updates -- --version 0.1.6 \
  --macos '/path/to/SAG Wiki.app.tar.gz' \
  --windows '/path/to/SAG Wiki-setup.exe' \
  --notes '/path/to/release-notes.md'
```

The script defaults to the local private key above and also supports `TAURI_SIGNING_PRIVATE_KEY` and the password environment variable. It signs both platform packages, confirms that the signing key matches the client public key, and generates update packages, `.sig` files and `latest.json` in `releases/0.1.6/updates/`.

Upload these files together with the installers to the GitHub `v0.1.6` official Release, then mark that release as latest. The download URLs in `latest.json` point to the same Release. If the manifest is missing, a signature is invalid or a download fails, the client displays an error and keeps the existing version. It does not open a browser.

A website update source uses the same `latest.json` format. The two platform keys are `darwin-aarch64` and `windows-x86_64`; each entry contains `url` and `signature`.

Older clients must first install a package containing this implementation before they can use in-app updates. Replacing installers on GitHub alone cannot add this capability to an older client.

## Validation scope

- Frontend tests cover progress, reopening settings after closing them, download and installation failures, retries, protection of unsaved documents and repeated clicks.
- Native tests download a signed test package through a local HTTP server, reject corrupt content and replace only a temporary macOS app. They confirm that test conversations and original files remain intact.
- Windows uses Tauri's official updater to start the installer, exit and reopen the app. The complete installation workflow still requires validation on a Windows machine before release.
