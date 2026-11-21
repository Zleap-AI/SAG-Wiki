# PDFium

The six archives in `8125/` are unmodified assets from the [upstream Chromium 8125 release](https://github.com/bblanchon/pdfium-binaries/releases/tag/chromium/8125). They cover x64 and ARM64 on macOS, Windows, and Linux. Each archive contains the upstream `LICENSE` and additional third-party license files under `licenses/`.

`scripts/prepare-pdfium.mjs` verifies the pinned SHA-256 for the target archive and extracts the runtime and licenses into the package. When updating PDFium, update the checked-in archives, pinned checksums, and Cargo PDFium API version together.
