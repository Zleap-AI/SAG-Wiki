# Development guide

[简体中文](../zh-CN/开发指南.md) · [Documentation index](README.md)

## Environment

Install Node.js, npm, Rust and the Tauri development dependencies for the target platform. Frontend dependencies are pinned in `package-lock.json`; Rust dependencies are pinned in each crate's `Cargo.lock`.

The repository has five Rust build entry points: `protocol/`, `client/`, `core/`, `cli/` and `src-tauri/`. Implement shared logic in `core/` so desktop and CLI do not maintain separate copies.

## Run the desktop development build

Run from the repository root:

```sh
npm ci
npm run tauri dev
```

The Tauri development command builds the debug CLI first, then starts Vite and the desktop backend. Frontend changes use hot reload; Rust changes require recompiling the backend. Routine UI development does not require repeatedly generating installers.

To start only the frontend development server:

```sh
npm run dev
```

The frontend server does not provide Tauri's local file and retrieval interfaces. Use the Tauri development entry point to verify complete desktop workflows.

## Build the CLI

```sh
npm run build:cli:dev
npm run build:cli
```

The first command produces a debug CLI; the second produces a release CLI. Development artifacts go in `src-tauri/bin/development/`, and release artifacts go in `src-tauri/bin/`. macOS artifacts go through a signing step.

To install the current build in the user's terminal, run explicitly:

```sh
./src-tauri/bin/sag install cli
sag --version
```

The installer checks ownership, checksums and build channel, validates a staged file, then replaces the executable atomically. The development desktop does not automatically update the release CLI, and the development CLI refuses to overwrite a registered release installation. After Rust source changes, rerun `npm run build:cli:dev` so desktop and service have the same build identity, then restart the desktop.

## Build the desktop app

```sh
npm run build
npm run tauri build
```

On Windows:

```sh
npm run package:windows
```

Use the system dependencies and signing configuration for the target platform. Distributable artifacts and the native CLI come from the same source version. For npm distribution, see [npm publishing](npm-publishing.md).

## Code checks

```sh
npm run check
npm test
cargo test --manifest-path protocol/Cargo.toml
cargo test --manifest-path client/Cargo.toml
cargo test --manifest-path core/Cargo.toml
cargo test --manifest-path cli/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml --lib
```

If desktop builds or checks require local CLI resources, run `npm run build:cli:dev` first. Use an isolated data directory for manual development checks to avoid affecting the everyday knowledge base:

```sh
SAG_DATA_DIR=/path/to/dev-data npm run tauri dev
SAG_DATA_DIR=/path/to/dev-data ./src-tauri/bin/development/sag service status
```

Set the same `SAG_DATA_DIR` when desktop and CLI should share development data. The service starts on demand after it exits; quitting the desktop does not stop it. Run `sag service stop` after testing when needed. Features that need a model service use credentials configured by the developer. Indexing and local matching do not require API keys.

## Documentation and repository boundaries

`docs/` contains the current architecture, development guides, interface documentation and clearly identified design plans. Update the relevant document when implementation changes, rather than adding conflicting historical descriptions.

Keep development artifacts, temporary output, logs, screenshots and local data out of the documentation directory. Credentials and user knowledge bases must not enter release packages. Keep automated tests and required fixtures in their respective test directories.
