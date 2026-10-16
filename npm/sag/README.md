# @zleap-ai/sag

Search local project files with SAG. The desktop app is optional; both use the same search core and knowledge base.

```sh
npm install -g @zleap-ai/sag
sag
sag add
sag "Where is permission checked?"
sag -E -c '^(ERROR|WARN) [0-9]+' ./logs
sag -F -c 'validatePermission(' ./src
```

You can also install with `pnpm add -g @zleap-ai/sag` or `bun add -g @zleap-ai/sag`.

First use of semantic search guides you through the JEV endpoint, model and API key. The key is entered without echoing. Existing settings are reused. To configure noninteractively, use `sag config --key 'YOUR_KEY'` or `sag config --key-stdin`. Command arguments may be saved in shell history.

The package includes precompiled native binaries. No Rust toolchain, desktop app, install script or extra binary download is required. Node.js 18 or later is required for the command launcher.

Supported platforms in this package: {{SUPPORTED_PLATFORMS}}.

Search is scoped to indexed folders under the current directory. `sag add` indexes the current project, `sag remove` removes its knowledge-base records without deleting local files. `.gitignore` and `.sagignore` are respected. Use `sag -c "question"` for original passages, or `--json` for structured output.

Semantic search remains the default. Use `-E` / `--extended-regexp` for Rust regex syntax or `-F` / `--fixed-strings` for literal substrings. These mutually exclusive modes match one line at a time, are case-sensitive by default, and need no API key or model. They share the knowledge-base scope and exclusions. `-m` limits matching lines; no matches exits successfully. Regex lookaround and backreferences are unsupported.

Local matching also supports `-i`, `-w`, `-x`, `-v`, context via `-A/-B/-C`, file names via `-l`, per-file `--count`, fragments via `-o`, and silent predicates via `-q` (exit 0/1/2 for match/no match/error). Use repeated `-e` patterns or `-f FILE` (`-` for stdin), plus ordered `--include` / `--exclude` filename globs. These controls require `-E` or `-F`. `-c` still means content. `--count` scans all lines unless an explicit global `-m` limit is supplied.
