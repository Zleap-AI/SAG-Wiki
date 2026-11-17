# @zleap-ai/sag

Search local project files with SAG. The desktop app is optional; both use the same search core and knowledge base.

```sh
npm install -g @zleap-ai/sag
sag
sag add --types md,txt,html,py,js,ts,tsx,json,yaml
sag "Where is permission checked?"
sag -E -c '^(ERROR|WARN) [0-9]+' ./logs
sag -F -c 'validatePermission(' ./src
```

The same unversioned install command updates an existing npm installation to the latest release. For Agent setup, follow https://github.com/Zleap-AI/SAG-Wiki/blob/main/cli/skills/sag/SKILL.md.

You can also install with `pnpm add -g @zleap-ai/sag` or `bun add -g @zleap-ai/sag`.

First use of semantic search guides you through the JEV endpoint, model and API key. The key is entered without echoing. Existing settings are reused. To configure noninteractively, use `sag config --key 'YOUR_KEY'` or `sag config --key-stdin`. Command arguments may be saved in shell history. For a local Jev service without authentication, use `sag config --url http://127.0.0.1:8000/v1 --model YOUR_MODEL --auth none`. This removes the saved Jev key and omits the authorization header. Base URLs and complete `/systemone` or `/decisions` endpoints are accepted. Use `--auth bearer --key-stdin` to return to API Key authentication.

The package includes precompiled native binaries. No Rust toolchain, desktop app, install script or extra binary download is required. Node.js 18 or later is required for the command launcher.

Supported platforms in this package: {{SUPPORTED_PLATFORMS}}.

Search is scoped to indexed folders under the current directory. `sag add` indexes the current project, `sag remove` removes its knowledge-base records without deleting local files. `.gitignore` and `.sagignore` are respected. Use `sag -c "question"` for original passages, or `--json` for structured output.

Semantic search remains the default. Use `-E` / `--extended-regexp` for Rust regex syntax or `-F` / `--fixed-strings` for literal substrings. These mutually exclusive modes match one line at a time, are case-sensitive by default, and need no API key or model. They share the knowledge-base scope and exclusions. `-m` limits matching lines; no matches exits successfully. Regex lookaround and backreferences are unsupported.

Local matching also supports `-i`, `-w`, `-x`, `-v`, context via `-A/-B/-C`, file names via `-l`, per-file `--count`, fragments via `-o`, and silent predicates via `-q` (exit 0/1/2 for match/no match/error). Use repeated `-e` patterns or `-f FILE` (`-` for stdin), plus ordered `--include` / `--exclude` filename globs. These controls require `-E` or `-F`. `-c` still means content. `--count` scans all lines unless an explicit global `-m` limit is supplied.

## File sharing

Share Markdown files and referenced local images without starting the desktop app or configuring a model:

```sh
sag share ./notes
sag download SAG-ABCDEFGH --output ./received
sag share-history --json
sag cancel-share SAG-ABCDEFGH
```

Shares expire after one hour. Share history lists shares created on this device; cancellation removes the server file and local record while leaving previously downloaded copies unchanged.

## HTTP API

Run `sag api key create --name "Application A"`, then `sag api start` to serve local and LAN applications. The API starts separately from CLI access and does not require the desktop app. It keeps running after the terminal closes. `sag api stop` closes HTTP access while retaining the shared backend and index.

Use `sag api status --json` to inspect state without starting the backend. Manage keys with `sag api key list` and `sag api key revoke KEY_ID`. With no valid key, the running API returns 401 Unauthorized.

For project access, use `sag api key create --name "Project A" --path "/data/project-a"`. Repeat `--path` for multiple folders; omitting it grants all folders. Change the scope with `sag api key set-scope KEY_ID --path "/data/project-a"` and export the matching client configuration with `sag api mcp config --key-id KEY_ID`. The exported configuration contains the full key; keep it private.

The HTTP API supports fast and deep search, source-passage results, hierarchical folder browsing and multiple absolute directory paths. Deep search uses the decision model configured by `sag config`; fast search needs no model. Optional startup registration is managed with `sag api autostart enable`, `disable` and `status`, and is off by default.

API overview: http://sag-wiki.zleap.com/en/docs/api-guide
API reference: http://sag-wiki.zleap.com/en/docs/api
