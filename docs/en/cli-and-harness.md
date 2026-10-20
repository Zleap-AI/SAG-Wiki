# sag CLI

[简体中文](../zh-CN/CLI与Harness使用说明.md) · [Documentation index](README.md)

## Standalone installation and configuration

sag runs independently, without installing or starting SAG Wiki desktop. Retrieval commands automatically start or reuse the local service.

With Node.js 18 or later installed, run on macOS Apple Silicon or Windows x64:

```sh
npm install -g @zleap-ai/sag
sag --version
sag
```

The published `@zleap-ai/sag` contains native executables for these two platforms. Rust, source compilation and desktop installation are unnecessary. npm registers the `sag` command. Reuse an existing working `sag` installation.

The first interactive run of `sag` guides configuration of the JEV retrieval service. The endpoint and model prompts show their current values; press Enter to keep them. First-time setup requires a key, entered with hidden input. The default endpoint is `https://api.302ai.cn/typesafeai/v1`, and the model is `jev-latest`. CLI search needs only this retrieval configuration. The answer model is configured separately in desktop.

If no terminal was available during installation, the first terminal invocation of `sag` or `sag "search query"` opens the same setup flow. When started from a search, saving configuration resumes the original query. Existing keys automatically reuse the current configuration. Ctrl+C cancels; nothing is saved until setup finishes. Run `sag config` to configure it again at any time.

With an existing local key file, configure it through standard input, optionally specifying the endpoint and model:

```sh
sag config --url "$JEV_URL" --model "$JEV_MODEL" --key-stdin < "$JEV_KEY_FILE"
```

Usually, run `sag` and enter the key at the hidden prompt. Do not paste keys into chat or place them in command arguments.

Keys are stored in private configuration for the current user and omitted from configuration output and installation logs. Noninteractive environments do not wait for input. An agent can read the explicit error and ask the user to run `sag` in a terminal. `sag config --if-needed` prompts only when configuration is missing and preserves existing settings. In a normal piped environment, `sag config` only outputs redacted configuration and whether a key exists.

Give your agent the [SAG Skill GitHub link](https://github.com/Zleap-AI/SAG-Wiki/blob/main/cli/skills/sag/SKILL.md) and ask it to follow setup for the current project. It retrieves companion files from the same directory and completes CLI installation, Skill installation, configuration checks, project indexing and retrieval verification. The user enters the first key in a terminal. Building from source is for development; see the [development guide](development.md).

## Sharing with desktop

CLI and the optional SAG Wiki desktop use the same indexing, retrieval and configuration implementation in `core/`, along with the same user data directory, `sag.sqlite`, keys and ignore rules.

- Desktop and CLI share a standalone background service, configuration and knowledge base. Directory changes appear in both.
- The service keeps running after desktop exits. `sag add` returns after indexing finishes; background watching updates file and ignore-rule changes.
- `sag service status` shows the service version and process. `sag service stop` stops it; `sag service restart` restarts it. Status inspection does not automatically start it.
- Both modes retain access controls and directory-scope restrictions. Failures do not switch search algorithms. If CLI access was disabled, explicitly run `sag config --enable` to restore it.
- After installing the SAG Skill for the relevant coding tool, open a new session to use it. The agent can run `sag add` for the current project, then `sag "search query"`.

## Commands

```sh
sag status
sag list
sag list --json
sag check
sag check /path/to/project --json
sag config
sag add
sag add /path/to/project --json
sag remove
sag remove /path/to/project --json
sag "Where are permissions checked after login?"
sag -c "How does configuration affect request retries?" ./src -m 5
sag "Where is the connection pool initialized?" --path ./server --json
sag -c "How are tasks cancelled?" --timeout 900 --json
sag -E -c '^(ERROR|WARN) [0-9]+' ./logs
sag -F -c 'validatePermission(' ./src --json
sag install claude-code
sag install codex
sag install opencode
sag uninstall codex
sag uninstall cli
```

`sag list` lists all knowledge-base directories, index status, file counts and indexing errors. `--json` returns a directory array. If none have been added, it returns an empty list without adding the current directory automatically.

`sag check` inspects the current or a specified directory. It distinguishes inclusion within an indexed directory, partial coverage of child directories, no index coverage and exclusion. It also shows index readiness, CLI access and the actual indexed-file count in the current directory. JSON `coverage` values are `included`, `partial`, `none` and `excluded`. The exit code is 0 when the directory is included, its index is ready, CLI access is enabled and indexed files exist; other check results return 1; argument or execution errors return 2. Ignore rules, directory exclusions and file types still determine searchable files. These two commands require no model configuration and do not register directories.

`sag status` shows only background-service connection status, service version, protocol version, process, executable, CLI access and JEV configuration status. It does not depend on the current directory's index state or start a stopped service. Use `sag check` for directory coverage, readiness and file counts.

`sag add` adds the current or a specified directory. It reuses the shared add/index workflow without creating duplicate roots. In desktop service mode, indexing runs in the background and the command returns the current state; standalone mode returns after indexing finishes. Use `sag check` to inspect readiness; no sync parameter is needed. `--json` returns the path, index state and current CLI availability. Added directories also appear in the desktop knowledge base.

`sag remove` removes the current directory, a registered root or a subdirectory from the knowledge base. Roots are unregistered. Subdirectories have their indexes cleared and remain excluded from later scans and file watching. All local files remain unchanged. Running `sag add` for the same subdirectory restores and reindexes it. If a directory moved or its disk is offline, remove its record using the original registered absolute path. `--json` returns the removed directory path. The directory disappears immediately from desktop's knowledge base. The UI folder menu provides the same removal action; **File settings → Removed subdirectories** allows restoration.

Search requires no `search` subcommand: run `sag "search query"` directly. `add`, `remove`, `list`, `check`, `status`, `config`, `service`, `install` and `uninstall` are reserved management commands. Use `--` when searching those literal words or queries beginning with `-`, for example `sag -c -- "status"`.

Search defaults to the current working directory. A positional path and `--path` cannot be combined. The default result limit is 10 chunks, with 1–100 allowed. `-c` changes output only, without changing recall or ranking. Paths resolve relative to the command's working directory. JSON includes absolute paths, line numbers, actual ranges and truncation flags. Only `-c` returns `content`. Separate source ranges are output separately, so discontinuous chunks are not represented as continuous lines.

### Regular-expression and fixed-string matching

Without a mode option, search remains semantic. `-E` (`--extended-regexp`) uses regular expressions; `-F` (`--fixed-strings`) matches literal strings. They cannot be combined. Both run entirely locally, require no model or API key and do not trigger first-time model setup.

Both modes are case-sensitive and line-based by default. Ordinary output returns one result per matching line even if that line has multiple matches. Regular expressions use Rust regex syntax, supporting groups, alternatives, character classes and line anchors, without lookaround or backreferences. Protect shell-special characters in regular expressions with single quotes.

Matching remains restricted to registered, indexed knowledge-base directories. It respects access controls, file types, `.gitignore`, `.sagignore` and manually removed subdirectories, and reads current file contents. Results follow file traversal order. `-m` limits matching lines, `-c` displays them, and `--json` retains the same path and line-number fields. No matches produce empty results with exit code 0.

The following options require `-E` or `-F` and do not change default semantic search:

| Option | Purpose |
| --- | --- |
| `-i` / `--ignore-case` | Ignore case |
| `-w` / `--word-regexp` | Match whole words; letters, digits and underscores are word characters |
| `-x` / `--line-regexp` | Match whole lines; takes precedence over `-w` when combined |
| `-v` / `--invert-match` | Select nonmatching lines |
| `-A N` / `--after-context N` | Show N following lines |
| `-B N` / `--before-context N` | Show N preceding lines |
| `-C N` / `--context N` | Show N lines on both sides |
| `-l` / `--files-with-matches` | Output each matching filename once |
| `--count` | Count matching lines per file, including files with zero matches |
| `-o` / `--only-matching` | Show only nonempty matched substrings; multiple hits on a line are separate outputs |
| `-q` / `--quiet` | Suppress output; found = 0, not found = 1, error = 2 |
| `-e PATTERN` / `--regexp PATTERN` | Add a pattern; repeatable |
| `-f FILE` / `--file FILE` | Read patterns line by line from a UTF-8 file; `-` means standard input |
| `--include GLOB` / `--exclude GLOB` | Include or exclude filenames by glob; repeatable |

Context options automatically display content, allowing 0–1000 lines per side. Overlapping or adjacent ranges merge; JSON `matched_lines` identifies which lines matched. `-A` and `-B` override their respective sides of `-C`. File-list, count, substring and quiet output modes are mutually exclusive and cannot use context. `-o` cannot combine with `-v`; `-q` cannot combine with `--json`. `-c` always means display content; use only `--count` for counting.

`-m` limits matching lines across the entire search; ordinary output defaults to 10 lines. With `-l`, it limits matching files. `-o` returns every nonempty matching substring from selected lines. `--count` counts all matches by default. With an explicit `-m`, it stops at the global matching-line limit and does not count subsequent files. JSON `results` contains lines or substrings, and `files` contains filenames or counts, alongside `matched` and `truncated`.

Multiple `-e` and `-f` patterns use OR semantics. When these options are used, the positional argument denotes only the search directory. An empty pattern file supplies no patterns. `-e ''` matches any line, matches nothing with `-w`, and matches only empty lines with `-x`. Each pattern is limited to 2000 characters; combined patterns are limited to 16 KiB.

File filters are applied in command order, with the last matching rule taking precedence. If the first filter is `--include`, other unmatched filenames are excluded; otherwise inclusion is the default. Globs match filenames only and cannot override knowledge-base ignore rules or manual exclusions.

```sh
sag -F -i -w -c 'token' ./src
sag -E -e 'ERROR' -e 'WARN' ./logs -C 3
sag -F -f patterns.txt ./src --include='*.ts' --exclude='*.test.ts' -l
sag -F 'TODO' ./src --count
sag -F 'ready' ./src -q
```

The default request timeout is 600 seconds, including index checks in standalone mode. `--timeout` accepts 1–86400 seconds; large multi-batch searches may need a higher value. Ctrl+C terminates the current request with exit code 130. Search argument errors return 2, execution errors return 1, and successful or empty results return 0. `check` uses the check-specific codes above. stdout contains results; stderr contains errors, installation information and truncation notices.

Requests are limited to 32 KiB and responses to 512 KiB. At the response limit, complete chunks are retained and truncation is explicitly marked. The retrieval candidate budget is not reduced to shorten output. Changed files require a new query; the CLI does not return new line numbers for stale chunks.

## Project ignore rules

Indexing, file watching and built-in list/grep honor `.gitignore` and `.sagignore` in the project root and its subdirectories. They work without a Git repository and do not read rules outside the knowledge-base root. `.sagignore` takes precedence over `.gitignore`; deeper directories take precedence among rules of the same type.

For example, create `.sagignore` in the project root:

```gitignore
# Exclude generated files and caches
output/
**/*.generated.md
# Exclude drafts, but retain a specific file
notes/draft-*.md
!notes/draft-final.md
```

Git ignore syntax is supported, including `*`, `**`, trailing-`/` directory rules and `!` exceptions. If a parent directory is entirely ignored, first unignore it before including its files again. Existing directory-name exclusions and manually removed subdirectories still apply; `!` cannot override them.

Standalone mode rechecks the index before the next search. With desktop auto-watching enabled, creating, modifying, deleting or renaming a rule file rechecks the project, removing newly ignored entries from the index and indexing newly included files. When watching is disabled, use reindexing in File settings. Rules do not delete or hide local files; the SAG file tree still browses them. Only manually removed subdirectories are hidden from the tree.

## Installation locations and upgrades

| Component | User-level location |
| --- | --- |
| npm CLI | npm global installation directory; inspect with `npm prefix -g` |
| CLI managed by desktop or installed with `sag install cli` | `~/.local/bin` (`sag.exe` on Windows) |
| Claude Code | `~/.claude/skills/sag/SKILL.md` |
| Codex | `~/.agents/skills/sag/SKILL.md` |
| OpenCode | `~/.config/opencode/skills/sag/SKILL.md` |

Update an npm-installed CLI with `npm install -g @zleap-ai/sag`, and remove it with `npm uninstall -g @zleap-ai/sag`. Running `sag install cli` again to create a second command is unnecessary.

Repeating `sag install` updates SAG-owned files. Installation records contain checksums, version and the actual CLI installation location. Later PATH-order changes do not change that location, and uninstallation follows the records. If foreign files, user modifications or altered SAG-managed terminal configuration blocks are detected, the installer stops and reports them. Uninstallation removes only verified SAG files and preserves other directory contents.

The command name is `sag`. On startup, the client updates managed CLIs and SAG Skills not modified by the user. An explicitly uninstalled CLI is not restored automatically. The `.sagignore` filename and existing knowledge base remain unchanged.

Windows uses a separate console `sag.exe`, bundled with the application installer. Installing the CLI does not normally require administrator privileges. The Windows packaging entry point is `npm run package:windows`. Available platforms depend on the native executables actually included in a release package.

## Error handling

- Standalone mode: without a desktop service, the local core is used automatically; opening the app is unnecessary.
- Core busy: another process is using the index; retry later.
- Access disabled: enable and save CLI access in CLI settings.
- No indexed content: run `sag add` for the current project. The program does not search knowledge bases outside the specified directory instead.
- Index not ready: wait for indexing and inspect file-index errors.
- JEV missing or request failed: inspect configuration with `sag config`, or check model settings in the optional desktop app.
- File changed: wait for the watcher to update, then query again.
- Installation conflict: retain existing files and establish their origin before resolving it manually.

For development and testing, `SAG_DATA_DIR` selects an isolated data directory. CLI and desktop must use the same value to share it.

## Automatic terminal configuration

- macOS/Linux support zsh, bash, fish and POSIX sh, respecting `ZDOTDIR` and `XDG_CONFIG_HOME`. Repeated installation does not duplicate configuration; uninstalling preserves existing environment settings.
- Windows updates the current user's PATH and broadcasts the environment change, without administrator privileges. Existing terminals keep their old environment; open a new terminal after installation.
- Implementation references: [zsh startup files](https://zsh.sourceforge.io/Doc/Release/Files.html), [Windows environment-change notification](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange).

## Harness support

Skill installation is currently provided for Claude Code, Codex and OpenCode.

- [Claude Code Skill](https://code.claude.com/docs/en/skills)
- [Codex Skill](https://learn.chatgpt.com/docs/build-skills)
- [OpenCode Skill](https://opencode.ai/docs/skills/)

For development, builds and code checks, see the [development guide](development.md).
