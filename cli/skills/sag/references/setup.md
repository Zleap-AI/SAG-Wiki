# Knowledge base, configuration and installation

SAG CLI and the optional SAG Wiki desktop app share the same local service, knowledge base, settings and credentials. Search automatically uses registered folders within the requested directory, or the requested subtree of a registered root. There is no per-folder CLI authorization setting.

## First-use setup

1. Run `sag --version`. If missing, run `/var/apps/sag-desktop-fnos/target/bin/sag install cli` as the NAS login user, then open a new terminal. Keep the SAG Wiki application running. The packaged native CLI supports fnOS x86-64 and ARM64; no Node.js or Rust installation is required.
2. Run `sag install codex`, `sag install claude-code`, or `sag install opencode` for the requested agent. The bundled Skill and three references match the installed fnOS application. Preserve user-edited files and unrelated skills. Open a new agent session after installation.
3. Run `sag config --if-needed` to reuse the Jev settings shared with the web app. If credentials are missing, ask the user to enter them in the web app's model settings or in the terminal using `sag`. Hidden input and `--key-stdin` are supported; do not request credentials in chat.
4. Run `sag check --json` from the intended directory. fnOS must authorize that directory for SAG Wiki before it can be indexed. If authorization is missing, use the web app's native folder authorization flow. When coverage is `none`, inspect the formats and run `sag add --types md,txt,html,py,js,ts,tsx,json,yaml` with the required extensions. Preserve existing exclusions and folder format selections. For `partial`, inspect `sag list` and select the intended scope. Do not re-enable excluded content or CLI access without authorization.
5. Verify the CLI version, installed Skill files, existing Jev configuration, a ready nonempty index, and a semantic search answering a specific question about the intended files. Report a credential or permission blocker explicitly; installation alone is not successful search verification.

The web app's **Settings → CLI** can run actual `sag` commands as the logged-in user, showing stdout, stderr and exit code. It does not execute arbitrary shell commands. Use SSH for interactive prompts or pipes. A copied command run on another computer searches that computer only if it uses a separate local installation; this fnOS CLI connects to the NAS-local application socket.

## Directory and status commands

| Command | Meaning |
| --- | --- |
| `sag status` | Show service connection, build version, protocol version, process, executable, CLI access and JEV configuration. It does not inspect directory coverage or start a stopped service. |
| `sag list [--json]` | List all registered roots with status, indexed file count and errors. JSON is an array; no roots gives an empty array. |
| `sag check [DIRECTORY] [--json]` | Check the current or specified directory's coverage, readiness, CLI access, indexed file count and relevant roots. |
| `sag add [DIRECTORY] [--types EXT,...] [--json]` | Add or update the directory and start indexing. `--types` selects native file extensions, such as `md,html,py,ts,json`; a leading dot and uppercase are accepted. Omit it to retain existing settings. JSON includes `path`, `index_status` and `cli_authorized`. |
| `sag remove [DIRECTORY] [--json]` | Remove a registered root or its subtree from the knowledge base; local files remain. JSON returns `path`. |

Only `check`, `add` and `remove` accept a directory argument. `status` and `list` do not. Relative paths resolve from the current directory.

`check` JSON includes `path`, `coverage`, `index_ready`, `enabled`, `indexed_files` and `roots`, plus `extensions` for an included directory. `extensions` lists the native formats enabled at that directory; child folders may have their own overrides. Coverage is `included`, `partial` (only child roots are registered), `excluded` or `none`. `indexed_files` is the actual count within the checked directory. A registered root with zero indexed files has no searchable content. Root status values include `pending`, `indexing`, `ready`, `partial` (some files failed) and `error`; root indexing status and directory coverage are separate fields. Exit codes are in [parameters](parameters.md#output-and-exit-codes).

`--types` is an extension allowlist for that folder, saved in the managed index-options block of `.sagignore`. Changing it rescans the folder and removes unselected files from the index; original files and handwritten ignore rules remain. Explicit child-folder settings retain precedence. Ignore rules continue to apply. Unknown extensions and conversion formats such as PDF are rejected; this option selects native text/code formats and turns off optional conversion for that folder. With no saved folder settings, plain `sag add` inherits global defaults (Markdown and text enabled; code and configuration disabled).

When the task's project is unindexed, run `sag add` with the required project extensions in its directory yourself. Check `sag check`; if indexing is still in progress, wait for it to finish before searching. No sync flag is needed. Add only the intended project, preserving existing exclusions. Do not add unrelated projects or broaden to a parent directory merely to find more results.

Removing a root unregisters it. Removing a subtree keeps it excluded from future scans and file watching. Restore a removed subtree only when requested, with `sag add` for that same subtree. A moved or offline registered root can still be removed using its original absolute path.

## Model configuration and access

Running `sag` without a query in an interactive terminal guides first-time JEV setup; with existing configuration, it shows help. Noninteractive calls do not prompt. Semantic search needs JEV configuration; local matching, index management and service inspection do not need a model key.

| Command or option | Meaning |
| --- | --- |
| `sag config` | In a terminal, prompt for endpoint, model and a hidden key; Enter keeps existing values, but first-time setup requires a key. Noninteractive invocation reads the current configuration. Output is JSON without the key. |
| `sag config --if-needed` | With no replacement options, reuse an existing key and configuration; otherwise guide terminal setup. Noninteractive use without a key reports an error. |
| `sag config --url URL` | Set the JEV endpoint. |
| `sag config --model MODEL` | Set the JEV model. |
| `sag config --key-stdin` | Read the key from standard input. |
| `sag config --key KEY` | Set a key through a command argument; it may appear in shell history and process arguments. Mutually exclusive with `--key-stdin`. |
| `sag config --enable` | Re-enable CLI access when the user explicitly requests it. There is no `--disable` CLI option. |

Options may be combined. Prefer hidden terminal input or `--key-stdin`; never print credentials. Use the user's endpoint, model and credentials, and preserve existing configuration unless the user requests a change. For example, when those environment variables and a key file have already been supplied:

```bash
sag config --url "$JEV_URL" --model "$JEV_MODEL" --key-stdin < "$JEV_KEY_FILE"
```

If setup is required and credentials have not been supplied, ask the user to run `sag` in a terminal. Never invent credentials or silently change providers. If CLI access is off, report that setting; do not enable it without the user's request.

## Service commands

| Command | Meaning |
| --- | --- |
| `sag service status` | Print service metadata as JSON when running, or report that it is stopped. Does not start it. |
| `sag service stop` | Stop this fnOS user's shared service; already stopped is successful. |
| `sag service restart` | Stop it if running, then start it and print service metadata as JSON. |

Search and management commands that need the core start or reuse the service automatically. Stopping or restarting affects web and CLI requests for the same fnOS user; other users remain running; run these actions when requested or necessary for an authorized service update. The internal `sag service serve` command runs the service in the foreground for startup diagnostics; normal use does not require it. Service subcommands do not accept `--json` or a directory.

## Installation and removal

The application package owns the native CLI. Update it through fnOS Application Center. `sag install cli` installs a user-owned launcher that follows the packaged executable across upgrades; `sag uninstall cli` removes that launcher without uninstalling the application.

```bash
sag install cli
sag install codex
sag install claude-code
sag install opencode
sag uninstall codex
sag uninstall cli
```

`sag install TARGET` and `sag uninstall TARGET` require exactly one target: `cli`, `codex`, `claude-code` or `opencode`.

- `cli` installs the fnOS launcher into `~/.local/bin` and configures the terminal path. Open a new terminal if the current one has not picked up the change.
- `codex` installs the skill and its references into `~/.agents/skills/sag`.
- `claude-code` installs into `~/.claude/skills/sag`.
- `opencode` installs into `~/.config/opencode/skills/sag`.

Run installation or removal for the requested target only. Repeat installation updates SAG-managed files. The installer preserves foreign or edited files and reports ownership conflicts; do not bypass those checks. Uninstall removes managed installation files, while leaving project files and the knowledge base intact. Restart the relevant agent session to load an updated skill.

## Help and failures

Use `sag --help` / `sag help` for the command list, `sag <command> --help` / `sag help <command>` for a command, and `sag service <action> --help` for service actions. `sag -V` / `sag --version` shows the CLI version; `sag status` also identifies the running service build. Internal `sag --build-info` emits build metadata for packaging and compatibility checks.

fnOS determines the shared data directory from the operating-system UID. `SAG_DATA_DIR` and `SAG_DESKTOP_DATA_DIR` do not select a different NAS knowledge base. User data and model keys remain in the application's private storage; never broaden its permissions to make the CLI work.

If a command fails, report its reason. After a relevant correction, retry only the affected operation; otherwise use normal file tools for the authorized task. Do not silently switch model providers, enable access, or replace user-edited installation files.
