# SAG Web Clipper

A standalone webpage-to-Markdown extension using Defuddle. Saving works without SAG Wiki or Obsidian. SAG Wiki provides package downloads and optional indexing of the save folder.

## Build

```sh
npm run build:clipper
# macOS with full Xcode:
npm run build:clipper:safari
```

Desktop builds automatically include the Safari test package on macOS and the Chrome/Edge package on all platforms. Packages go to `src-tauri/clipper/`; unpacked extensions go to `browser-extension/build/`.

Chrome/Edge packages use Load unpacked. Safari source builds use local ad-hoc signing and require Allow Unsigned Extensions. Distributable Safari packages must be signed with a Developer ID certificate, accepted by Apple notarization, and stapled before packaging. Verify the final app with `codesign --verify --deep --strict`, `xcrun stapler validate`, and `spctl --assess --type execute`. Browser-store publication is separate.

## Save behavior

- Chrome/Edge: `downloads` permission, default browser download directory, optional Save As dialog.
- Safari: `nativeMessaging` permission, with the native handler bundled in the extension's own app. Its sandbox allows writes to user-authorized folders and system Downloads. No SAG Wiki connection is involved.
- The default destination is Documents/sag-wiki/clippings. Click Save settings to commit changes; a folder picker opens on first use to select Documents, then the extension creates the subfolders if needed. Default and custom destinations have separate directory handles (Chrome/Edge) or security-scoped bookmarks (Safari), preserving existing custom-folder permissions. Dropdown labels include the location. Folder and language choices are drafts until Save settings is clicked. The indexing reminder appears only after applying a destination change. An optional Downloads mode uses the relative subfolder clippings.
- New files include source URL, title and capture time. Original files are not overwritten; images keep source URLs.
- SAG Wiki indexing is optional: add the actual save folder once in its folder manager. The extension does not open a local HTTP connection or use a connection code.

The Windows desktop package includes the Chrome/Edge ZIP. Default folders use the system Documents location through `dirs::document_dir`; selected save locations are browser directory handles, not string paths. Filename and package configuration checks run in `tests/web-clipper-windows.test.js`; these checks do not replace Windows runtime validation.

## Interface

The popup, settings, context menu and common save errors support Simplified Chinese and English. Language follows the browser or the explicit selection in settings. The Safari setup app has its own language selector. System dialogs follow the OS language. Both light and dark themes use black, white and gray. Settings text and controls are 18px with a 26px title. Toolbar PNGs at 16, 32, 48 and 128px are rendered from `assets/toolbar-icon.svg`; the Safari app includes a complete AppIcon asset set.

## Validation

```sh
npm run test -- tests/web-clipper.test.js tests/web-clipper-localization.test.js tests/web-clipper-windows.test.js tests/web-clipper-destinations.test.js tests/web-clipper-settings.test.ts
swiftc browser-extension/safari/ClipFileStore.swift browser-extension/tests/ClipFileStoreTests.swift -o /tmp/sag-clipper-file-tests
/tmp/sag-clipper-file-tests
```

## License

[Defuddle](https://github.com/kepano/defuddle) is MIT licensed. Its license is included in each package, and bundled dependency notices are preserved. Obsidian branding, logos and marketing assets are not used.
