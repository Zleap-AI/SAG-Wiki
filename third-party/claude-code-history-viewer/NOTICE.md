# Claude Code History Viewer

Source: https://github.com/jhlee0409/claude-code-history-viewer
Commit: 67979b401e936ed503c941a3f3294012f51d730e (v1.32.2)
Copyright (c) 2025 JaeHyeok Lee. MIT license; see LICENSE.

`core/src/chat_import.rs` adapts the Codex rollout filename / compressed-twin
selection rules and references the Claude/Codex event conversion and continuation
logic in `src-tauri/src/providers/codex.rs`, `commands/session/load.rs`, and
`commands/session/chain.rs`. SAG uses buffered reads, explicit import errors,
role and tool-block filtering, stable document identities, and its existing
Markdown storage and index instead of the viewer's UI, caches and export runtime.
