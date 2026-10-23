# Publish @zleap-ai/sag

[简体中文](../zh-CN/npm发布说明.md) · [Documentation index](README.md)

Installation commands after publication:

```sh
npm install -g @zleap-ai/sag
sag
```

`pnpm add -g @zleap-ai/sag` and `bun add -g @zleap-ai/sag` are also supported. The first run guides JEV configuration. Then use `sag add` and `sag "search query"`. The command name remains `sag`.

## Prepare the package

Run from the SAG-Desktop project directory:

```sh
node scripts/package-npm-cli.mjs
```

The script builds the standalone CLI for the current platform and generates:

- `releases/npm/sag/`: the publishable directory.
- `releases/npm/zleap-ai-sag-<version>.tgz`: the npm package; its filename changes with the CLI version.

The npm package contains only the launcher, prebuilt executables, documentation and package metadata. It excludes the desktop app, project source, knowledge base and user keys. There are no additional download or installation scripts, so installation does not depend on GitHub download URLs or require Rust. npm's global installation registers the `sag` command.

The two source `package.json` files remain `private: true` to prevent accidental publication of development directories. Publish the generated `.tgz`; the desktop project's `package.json` does not need to change.

## Sign in and publish

You need the npm account `zleap-ai`, or membership of the npm `zleap-ai` organization with publishing permission. The npm account is independent of the JEV API key. User model keys must not be included in the npm package.

```sh
npm login
npm whoami
npm_version=$(node -p "require('./releases/npm/sag/package.json').version")
npm_package="./releases/npm/zleap-ai-sag-${npm_version}.tgz"
npm publish "$npm_package" --dry-run --access public
```

After confirming the package name, version, file list and supported platforms, publish:

```sh
npm_version=$(node -p "require('./releases/npm/sag/package.json').version")
npm_package="./releases/npm/zleap-ai-sag-${npm_version}.tgz"
npm publish "$npm_package" --access public
```

Complete two-factor verification as prompted by npm. Before each new release, update the version in `cli/Cargo.toml` and rebuild the package. npm does not allow overwriting a published package with the same name and version.

The generated `package.json` and README define the supported platforms. Platforms absent from the package are not compiled during installation.

## Combine multiple platforms

After each platform build, the script saves its executable in `releases/cli-native/<version>/<platform>-<architecture>/`. Set `SAG_TARGET` to use a configured Rust cross-compilation target. The script can collect x64 and arm64 builds for macOS, Linux and Windows. Platforms not yet built are not listed in the generated package.

To publish one version for multiple systems, build the same commit and version in each environment, collect the directories under `releases/cli-native/<version>/` on the publishing machine, then rerun the packaging script. Check the final supported platforms and publish only once.

References: [Publishing scoped public packages](https://docs.npmjs.com/creating-and-publishing-scoped-public-packages/), [npm publishing verification requirements](https://docs.npmjs.com/requiring-2fa-for-package-publishing-and-settings-modification/).
