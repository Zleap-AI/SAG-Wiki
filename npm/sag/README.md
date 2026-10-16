# @zleap-ai/sag

Search local project files with SAG. The desktop app is optional; both use the same search core and knowledge base.

```sh
npm install -g @zleap-ai/sag
sag
sag add
sag "Where is permission checked?"
```

You can also install with `pnpm add -g @zleap-ai/sag` or `bun add -g @zleap-ai/sag`.

First use guides you through the JEV endpoint, model and API key. The key is entered without echoing. Existing settings are reused. To configure noninteractively, use `sag config --key 'YOUR_KEY'` or `sag config --key-stdin`. Command arguments may be saved in shell history.

The package includes precompiled native binaries. No Rust toolchain, desktop app, install script or extra binary download is required. Node.js 18 or later is required for the command launcher.

Supported platforms in this package: {{SUPPORTED_PLATFORMS}}.

Search is scoped to indexed folders under the current directory. `sag add` indexes the current project, `sag remove` removes its knowledge-base records without deleting local files. `.gitignore` and `.sagignore` are respected. Use `sag -c "question"` for original passages, or `--json` for structured output.
