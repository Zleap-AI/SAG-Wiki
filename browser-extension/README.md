# SAG Web Clipper

A standalone webpage-to-Markdown extension using Defuddle. Saving works without SAG Wiki or Obsidian. SAG Wiki provides package downloads and optional indexing of the save folder.

## Build

```sh
npm run build:clipper
# macOS with full Xcode:
npm run build:clipper:safari
```

Desktop builds include the Chrome/Edge ZIP on all platforms. On macOS, provide an already notarized Safari DMG in `src-tauri/clipper/sag-web-clipper-safari.dmg` or through `SAG_CLIPPER_SAFARI_DMG`. Without an installer, ordinary builds disable Safari export. Set `SAG_REQUIRE_SAFARI_CLIPPER=1` for builds that must contain it. Desktop builds verify and preserve a supplied installer; they do not replace it with an unsigned test build. Unpacked extensions go to `browser-extension/build/`.

Chrome/Edge packages use Load unpacked. Safari source builds use local ad-hoc signing and require Allow Unsigned Extensions. Test builds use the separate `com.sag.webclipper.development` identifier and `safari-development-derived` directory, so they cannot take over the installed release's registration. Their DMG stays in `browser-extension/build/`. To prepare a release app with the production identifier, run `node browser-extension/build.mjs --safari --release`. Xcode still builds under the development identifier; the production copy awaiting signing is written to `browser-extension/build/safari-release.noindex/SAG Web Clipper.app`. Do not launch unsigned production copies. Distributable Safari apps must then be signed with a Developer ID certificate, accepted by Apple notarization, and stapled before packaging. Create the DMG with `node browser-extension/package-safari-dmg.mjs "<app-path>" "<output.dmg>"`, then sign, notarize and staple the DMG as well. The saved Finder layout displays the app and an Applications shortcut; the packager uses only macOS tools and does not change Safari settings. Browser-store publication is separate.

Copy the built Safari app to the release directory, then sign it with `node browser-extension/sign-safari.mjs sign "<app-path>" "<Developer ID identity>"`. This preserves the effective entitlements of each Xcode target and removes debugger access. The setup app needs `com.apple.security.network.client` for WebKit to start; re-signing both targets with the file-saving entitlements alone removes this setting and causes a blank setup window. Verify the final app with `node browser-extension/sign-safari.mjs verify "<app-path>"`, `xcrun stapler validate`, and `spctl --assess --type execute`. Mount the final DMG, copy the app to Applications, and launch the installed copy to check that the language selector and extension status appear, and that Done closes an enabled setup app while a disabled extension offers Safari settings.

## Save behavior

- The context menu opens a 520 × 660 preview centered over the browser window containing the original webpage. There is no toolbar clipping action or related settings switch. Installation and updates do not customize Safari’s address bar or toolbar layout; usage requires only the webpage context menu. Browsers may still show their own extension-management entry; Safari users can remove it from the toolbar, and Chrome/Edge users can unpin it. The context menu does not use a browser action popup. The window shows the title, source URL, Markdown and destination. No file is written until Save is clicked; Cancel or closing the window discards the preview. Save uses the previewed content even if the source tab changes or closes.
- Chrome/Edge: `downloads` permission, default browser download directory, optional Save As dialog.
- Safari: `nativeMessaging` permission, with the native handler bundled in the extension's own app. Its sandbox allows writes to user-authorized folders and system Downloads. No SAG Wiki connection is involved.
- The default destination is Documents/sag-wiki/clippings. Safari writes directly to this fixed directory without a bookmark or folder picker, using a sandbox exception restricted to `/Documents/sag-wiki/`; it resolves the real user home rather than the app container. macOS may still show its own Documents access prompt. Chrome/Edge confirm the default folder on first Save. Custom destinations use browser directory handles or Safari security-scoped bookmarks. Settings changes remain drafts until Save settings is clicked; the indexing reminder appears only after applying a destination change. An optional Downloads mode uses the relative subfolder clippings.
- New files include source URL, title and capture time. Original files are not overwritten; images keep source URLs.
- SAG Wiki indexing is optional: add the actual save folder once in its folder manager. The extension does not open a local HTTP connection or use a connection code.

The Windows desktop package includes the Chrome/Edge ZIP. Default folders use the system Documents location through `dirs::document_dir`; selected save locations are browser directory handles, not string paths. Filename and package configuration checks run in `tests/web-clipper-windows.test.js`; these checks do not replace Windows runtime validation.

## Interface

The popup, settings, context menu and common save errors support Simplified Chinese and English. Language follows the browser or the explicit selection in settings. The Safari setup app has its own language selector. System dialogs follow the OS language. Both light and dark themes use black, white and gray. Settings text and controls are 18px with a 26px title. Extension PNGs at 16, 32, 48 and 128px are rendered from `assets/toolbar-icon.svg`; the Safari app includes a complete AppIcon asset set. All extension and packaged macOS application icons use transparent backgrounds, including the Safari context menu icon.

## Validation

```sh
npm run test -- tests/web-clipper.test.js tests/web-clipper-preview.test.js tests/web-clipper-localization.test.js tests/web-clipper-windows.test.js tests/web-clipper-destinations.test.js tests/web-clipper-settings.test.ts
swiftc browser-extension/safari/ClipFileStore.swift browser-extension/tests/ClipFileStoreTests.swift -o /tmp/sag-clipper-file-tests
/tmp/sag-clipper-file-tests
```

## License

[Defuddle](https://github.com/kepano/defuddle) is MIT licensed. Its license is included in each package, and bundled dependency notices are preserved. Obsidian branding, logos and marketing assets are not used.
