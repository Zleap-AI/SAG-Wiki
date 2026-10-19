# Architecture

[简体中文](../zh-CN/架构设计.md) · [Documentation index](README.md)

SAG Wiki is a local-file knowledge base. The desktop UI and CLI connect to the same standalone background service, while original files stay in the user's directories. Indexing, browsing and local matching work offline. Semantic retrieval and Q&A call the model services configured by the user.

## Modules

| Path | Responsibility |
| --- | --- |
| `src/` | Svelte UI, state, editor and document rendering |
| `src/components/ui/` | Local UI components used with Bits UI |
| `src-tauri/src/lib.rs` | Tauri command adapters, event forwarding and window lifecycle |
| `core/src/explore.rs` | Document relationships, graph data and relationship evidence |
| `core/src/store.rs` | SQLite, file metadata, index reads/writes and caches |
| `core/src/text.rs`, `document_text.rs` | Text decoding, chunking, tokenization and source positions |
| `core/src/indexer.rs`, `ignore_rules.rs` | Index queue, file changes and ignore rules |
| `core/src/retrieval.rs`, `sag2_v10.rs` | SAG2 recall, expansion and final ranking |
| `core/src/models.rs`, `agent.rs` | Model connections, Q&A and the tool-call loop |
| `core/src/sag_mcp.rs`, `mcp.rs` | Built-in tools and external MCP integration |
| `core/src/service.rs`, `server.rs` | Shared business logic, index watching and local service |
| `protocol/` | Independent requests, responses, events and build identity |
| `client/` | Service discovery, authentication, persistent connections, chunked transfer and cancellation |
| `core/src/cli_service.rs` | CLI operations using the shared service core |
| `cli/src/` | Command arguments, standalone operation, terminal output, installation and Skill |
| `scripts/` | Build, installation and release scripts |

The desktop uses Tauri 2 and Svelte. SQLite FTS5 provides local full-text search. Running release artifacts does not require Node.js, Python or Rust.

## Processes

```text
Desktop UI → Tauri → sag-client ─┐
                                ├→ sag service serve → sag-core
sag command → sag-client ───────┘      └→ SQLite / file watching / models
```

For each data directory, one service owns the database, index watcher, retrieval caches and model clients. Desktop and CLI automatically start it when needed. Closing or quitting the desktop leaves the service running. Ordinary CLI commands connect to it without initializing the core again in their own processes.

The local service listens only on `127.0.0.1`, using a random port and token. Connection metadata is stored in `sag-service.json` in the user data directory. A single frame is limited to 128 KiB and a complete message to 64 MiB. Large documents use chunked transfer. Tokens are not displayed in diagnostics.

`sag service status` shows the protocol version, build identity, development or release channel, service PID and executable location. Different builds are not silently mixed. An upgrade within a channel stops the old service first; development and release channels use separate data directories. See the [migration plan](service-unification-plan.md) for implementation progress and acceptance results.

## Storage

The default data directory is determined by the operating system's user data location and the application identifier `com.sag.desktop`. `SAG_DATA_DIR` selects an isolated data directory.

| Data | Storage |
| --- | --- |
| Original documents | Remain in the local folders added by the user |
| Directory and file information | SQLite stores paths, names, modification times, sizes and hashes |
| Chunk positions | SQLite stores headings, byte ranges and line/column positions |
| Full-text search | FTS5 uses `content=''` to store search terms and indexes, without a copy of the source body |
| Retrieval preparation data | Derived information such as vocabularies, with reusable statistics and query caches |
| Document relationships | Derived term summaries used for local similarity calculations |
| User settings | Stored in the application data directory |
| Model and MCP credentials | Private files for the current user; Unix permissions are 0600 |
| Q&A history, citations and drafts | Application session content, managed separately from the file index |

When retrieval results are read, the service accesses the original files at the stored positions and verifies their hashes. If a file changed, it requests reindexing rather than applying old positions to a new body. Conversation citations retain the content used when the answer was generated.

## File ingestion and indexing

1. Add a directory, normalize its path and resolve overlapping parent/child coverage.
2. Register it and submit background indexing. The desktop receives actual progress and error events.
3. Filter files by type, directory exclusions, `.gitignore` and `.sagignore`.
4. Skip unchanged files; read the source and verify hashes when needed.
5. Decode, chunk and tokenize text, generate derived information, then write batches to SQLite.
6. File watching merges subsequent changes into incremental tasks. Index-version changes invalidate relevant caches.

Markdown, plain text, RTF, supported code and text configuration files can be indexed. File-type settings determine the actual scope. Symbolic links cannot expand access.

Body chunks target approximately 512 characters by default, preserving complete sentences; code preserves complete lines. Very long sentences or lines may exceed the target. Chunks retain heading paths, and adjacent chunks overlap by approximately 80 characters of complete sentences or lines. Custom lengths are stored in retrieval settings. Base64 image data in HTML is excluded from the index and vocabulary.

Removing a directory from the knowledge base clears only its index and registration. Removing a subdirectory persists an exclusion; adding it again restores it. Original files are always preserved.

## Retrieval and Q&A

### Quick search

Local FTS5 BM25 merges hits by file and paginates them, displaying 20 files per page without calling a model. Database queries are separate from source-file verification. Only candidate content needed for the response is reconstructed.

### SAG2 semantic retrieval

BM25 recalls candidates, Jev judges evidence relevance, configured term expansion drives subsequent retrieval, and local diversity ranking produces the final results. Statistics, query and term-frequency caches are reused and invalidated by index version. Candidates exceeding a single model request's budget are processed in batches while keeping chunks intact.

Q&A uses the configured candidate and result counts. Desktop deep search has a separate limit for displayed files. The CLI's `-m` controls the final output count. All share the same underlying retrieval implementation.

### Q&A and tools

The answer model calls built-in `search`, `list`, `read`, `grep` or configured external MCP tools as needed. Each round shares a tool-call limit and context budget. Tool results carry stable source numbers, and answer citations refer to the corresponding original sources.

Request failures, context-limit violations, changed files and indexes that are not ready return explicit states without silently switching algorithms. See [Built-in MCP tools](mcp-tools.md) for the protocol.

## UI and document display

- File browsing and query scope are managed separately. Browsing a directory does not implicitly change Q&A scope.
- Markdown uses an editor loaded on demand and checks for conflicting external changes before saving.
- Search lists show short snippets around hits. Selecting a document renders Markdown or HTML on the right and highlights matches. Opening the full document retains its position; returning goes back to search.
- HTML renders in an isolated view, avoiding layout of long image encodings as body text.
- Relationship graphs render in the frontend; the local backend supplies data and evidence. See [Files and Explore design](files-and-explore.md).

## Concurrency and lifecycle

Index preparation can run in parallel; SQLite writes use transactions. Retrieval uses read-only connections and bounded caches. File hashing uses platform-supported acceleration. Configuration and protocol definitions specify actual concurrency, request limits and defaults.

The service supports 20 concurrent retrieval requests. Requests, timeouts and cancellations are independent. Writes to the same conversation are coordinated per conversation. Tool approvals go only to the connection that initiated the request. Disconnecting cancels that connection's tasks while other connections continue.

Closing the window hides the desktop, and quitting it also leaves the background service running. `sag status` and `sag service status` inspect status without starting the service. `sag service stop` stops it; `sag service restart` restarts it. After reconnecting, desktop reloads files, settings and history state.

## Dependencies and distribution

The core is linked only into the `sag` executable. Desktop depends only on the protocol and client. Desktop packages bundle native CLIs from the same build, while npm packages distribute only the CLI launcher and prebuilt native programs. See the [development guide](development.md) for builds and [npm publishing](npm-publishing.md) for distribution. Dependency licenses are stored in `src-tauri/licenses/`.
