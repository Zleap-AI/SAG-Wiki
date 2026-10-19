# SAG standalone service and shared desktop core implementation plan

[简体中文](../zh-CN/SAG独立后台服务与桌面统一内核实施计划.md) · [Documentation index](README.md)

Date: 2026-10-18

Status: Core migration implemented and local acceptance checks passed; release packages and Windows installation await release validation.

## 1. Goals

Connect desktop and CLI to one persistent SAG service. Started by the `sag` executable, it independently manages the knowledge base, indexing, file watching, model configuration and retrieval. Desktop becomes an optional UI whose closure does not affect CLI operation.

Run only one service per user data directory. It owns SQLite, file watchers, retrieval caches and network clients to prevent duplicate initialization, version mismatches and duplicate watchers.

Preserve existing usage:

- A CLI-only installation can perform first-time configuration, add folders and search.
- Installing desktop automatically prepares the required CLI and service.
- `sag "complete question"` remains the default semantic-search entry point; existing options and Skill entry points remain valid.
- Added folders are indexed automatically. File changes follow watching settings, preserving `.gitignore`, `.sagignore` and subdirectory exclusions.
- The index stores only file information, headings, positions and search terms. Source passages are read from local files by position.

## 2. Existing implementation and migration scope

See [Architecture](architecture.md) for process relationships and module boundaries.

| Area | Existing state | Target state |
| --- | --- | --- |
| Core code | Desktop and CLI share `sag-core` but compile it separately | The core runs only in the persistent service process |
| Desktop startup | Directly opens the database and starts watching and the CLI service | Connects to the service, starting it if necessary |
| CLI execution | Tries desktop first, then loads the core in its own process on failure | Connects to the persistent service, starting it if necessary |
| Lifecycle | Desktop exit ends the service it hosts | The service continues after desktop exits |
| External interface | Mainly status, configuration, add, remove, search and matching | All desktop knowledge-base operations and events |
| Large-file transfer | CLI request limit 32 KB, response limit 512 KB | Bounded ordinary messages and chunked large-file transfer |
| Build and installation | Development updates CLI from the debug directory; release uses resources | Separate development and release artifacts, checking versions by build identity |

Reuse the existing single-instance lock and read-only query connections. The instance lock establishes process ownership; it must not become an exclusive lock shared by every query.

## 3. Target structure

```text
Desktop UI ── local client ──┐
                            ├── persistent SAG service (sag service mode)
sag command ─ local client ─┘      ├── SQLite and retrieval caches
                                   ├── file watching and indexing tasks
                                   ├── BM25 / SAG2 / model requests
                                   └── settings, conversations, drafts and tools
```

Default to the existing local TCP transport: listen only on `127.0.0.1`, with a random port and a private connection record for the current user. Keep random-token authentication without adding a web framework or externally accessible port.

### 3.1 Module responsibilities

| Module | Responsibility |
| --- | --- |
| `sag-core` | Retrieval, storage, indexing, models and tools; remain independent of Tauri |
| Independent protocol module | Requests, responses, events, versions and error types, without database or model implementation dependencies |
| Local client module | Service discovery, version handshake, requests, events, cancellation and disconnection handling |
| `sag` | Terminal and service startup entry points; ordinary commands use only the client |
| Desktop backend | Adapts Tauri to the service and handles windows, tray, file selection and other OS operations |
| Desktop frontend | Content, input, settings UI, HTML rendering and interaction state |

Ultimately remove `sag-core` from desktop dependencies, leaving only the protocol and client modules. Desktop must no longer create a `Store`, watcher or retrieval executor directly.

### 3.2 Service lifecycle

1. Check for an existing service at desktop startup and before CLI search, directory addition or configuration.
2. If absent, start `sag` in internal service mode. Ordinary search syntax stays unchanged.
3. When clients start simultaneously, a per-data-directory process lock selects one service. Other clients wait for readiness before connecting.
4. Atomically write connection metadata only after startup completes. Handle stale records using connection results and process identity when a process is no longer valid.
5. Desktop exit leaves the service running. Provide an explicit stop action that ends watching, cancels requests, completes necessary transactions and releases the lock.
6. Do not add operating-system startup configuration in the first version.

`sag status` inspects existing state without starting a service. Operations requiring it handle startup.

## 4. Required invariants

### Data and index

- Reuse the current user data directory, SQLite database and settings. Process migration alone must not rebuild the whole knowledge base.
- Do not add original-body columns to `documents` or `chunks`; FTS continues using `content=''`.
- Preserve original-file hashes, byte ranges, line/column positions and version checks when reading.
- Preserve unchanged-file skipping, retrieval-statistics and BM25 caches, and HTML image-encoding exclusion.
- Preserve saved chunk lengths and other retrieval parameters. Migration does not change retrieval strategy.
- Retain existing conversation, citation and draft text storage separately from the file index.

### Concurrency and cancellation

- Support 20 simultaneously submitted retrieval requests without adding a global exclusive query lock.
- Hold write-transaction locks only when writing is necessary. Coordinate writes to the same conversation per conversation.
- Give each request an independent ID, timeout, progress and cancellation state. One client's cancellation must not cancel another client's requests.
- Clean up a disconnected client's requests while connected clients continue.
- Preserve request-scope, path and symbolic-link checks. Migration must not broaden directory access.

### Transport and large files

- Define typed requests, responses and events. The protocol must not expose arbitrary SQL or function execution.
- Send progress, streamed model output, index changes and tool approvals as events with request IDs.
- Transfer large file bodies in chunks, limiting frame size and request resource usage separately. A 14–18 MB HTML file cannot be placed directly in the existing 512 KB response.
- Search previews continue showing retrieved snippets directly. Fetch the full document only when opened. HTML renders in an isolated desktop view.
- Transmit keys only during explicit configuration operations, excluding command arguments, connection records and logs. Configuration reads return only whether credentials are configured.

## 5. Implementation stages

Deliver and verify each stage separately. Intermediate versions remain runnable. Use isolated data directories when comparing old and new services.

### Stage 0: Unify builds and version identity

Complete this stage first to address missed CLI updates independently.

- Add a verifiable core build identity and protocol version. An application version alone cannot distinguish multiple builds of the same version.
- Desktop and CLI diagnostics show the actual service version, process and executable location without tokens.
- Desktop installers and CLI distribution use native artifacts from the same build.
- Development builds use separate artifacts and explicit deployment steps, preventing an older debug CLI from replacing an installed release when development starts.
- Use staging, validation and replacement for updates, respecting installation records and user modifications.

Acceptance: One build makes the desktop/CLI/service relationship traceable. Old development artifacts cannot silently overwrite newer release artifacts.

### Stage 1: Extract UI-independent service capabilities

- Inventory every command and event in `src-tauri/src/lib.rs`.
- Move query orchestration, conversation writes, model configuration and tool-approval coordination into the service layer.
- Replace `AppHandle`-dependent event delivery with a service-event interface forwarded by Tauri.
- Move Explore data computation into the service, keeping graph rendering and interaction in the frontend.
- Extract independent protocol types so sharing types does not require desktop to link the full core.

Acceptance: The service layer executes existing operations in tests without a Tauri window, preserving result and error semantics.

### Stage 2: Implement the persistent service and connect CLI first

- Extend the listener, authentication and instance lock in `core/src/cli_service.rs`.
- Let the service initialize the database, index queue, file watching, caches and network clients centrally.
- Add CLI discovery, startup, readiness waiting and version-handshake flows.
- Stop CLI queries from opening the database themselves. Remove the connection-failure path that loads a second core.
- Preserve first-run configuration, installation and Skill commands. Background logs must not mix with terminal results.

Acceptance: Without desktop installed, first-time configuration, `sag add`, search, matching and removal all work. Repeated CLI invocations connect to one service and reuse caches across commands.

### Stage 3: Migrate desktop file, index and settings operations

- Migrate directory listing, file reads and saves, drafts, directory addition/removal, reindexing and settings reads/writes.
- Migrate image reads and chunked large-HTML transfer.
- Connect desktop to index progress, directory changes and error events. Reconnection first fetches full state, then receives subsequent events.
- Keep windows, tray, Finder / file manager and OS file pickers in desktop.

Acceptance: Desktop and CLI see the same knowledge base and index state. Directory additions, modifications and removals agree on both sides. Large HTML and images display correctly.

### Stage 4: Migrate Q&A, tools and Explore operations

- Migrate quick search, deep search, streamed Q&A and conversation operations.
- Migrate internal tools, external MCP calls and approval flows.
- Send approval requests only to the initiating client. If it disconnects while human approval is still needed, end the request and explain why.
- Migrate Explore overview, relationships, evidence and cancellation interfaces.
- Remove desktop's direct ownership of `Store`, index threads, retrieval caches and model clients.

Acceptance: CLI and desktop work concurrently. Stopping, failing or denying approval for one request leaves others unaffected. CLI works after desktop exits.

### Stage 5: Unify upgrades and releases

- New clients handshake by protocol version. Incompatibility explicitly requires an upgrade without falling back to a second local core.
- Stop accepting new work before upgrading. Complete or explicitly cancel in-flight tasks, stop the old service, then start the new one.
- Failed updates preserve existing installation files. Rollback still runs only one service.
- Desktop uninstallation preserves the independent CLI, service and user knowledge base by default. Complete removal requires a separate explicit action.
- Verify native-artifact identities in desktop and CLI packages.

Acceptance: Upgrades, restarts, CLI-only usage, desktop-first usage and combined installations pass. Delivered desktop builds no longer link the retrieval core.

## 6. Validation and performance criteria

### Functionality and data

| Scenario | Acceptance criterion |
| --- | --- |
| Two clients start for the first time simultaneously | One valid service and watcher per data directory |
| CLI runs after desktop exits | Service remains usable without preparing the entire knowledge base again |
| 20 concurrent queries | No global exclusive query lock; results correspond to their requests |
| Cancellation, timeout, disconnection | Only the corresponding request is affected; the client stops waiting promptly |
| File changes and ignore rules | Both clients agree, caches invalidate promptly, original files remain intact |
| Source text and citations | Content, hashes, byte positions and line/column positions match original files |
| Database storage | File bodies do not enter ordinary tables; FTS does not store source-body copies |
| Large HTML | Search displays short snippets; full documents render images, styles and internal navigation correctly |
| Models and MCP | Streaming, failure messages, approvals and credential reads/writes preserve existing behavior |
| Version mismatch | Explicit diagnostics; no silent use of an old core |

### Performance

Compare before and after migration on the same local machine with the same knowledge-base copy, parameters and queries. Report remote-model time separately. Do not combine timings from different machines or endpoints.

- Establish pre-migration baselines for cold start, cached queries, repeated CLI calls, 20-way concurrency and queries during indexing.
- Time client startup, service connection, queuing, index preparation, BM25, original-file reads and validation, model requests and UI rendering separately.
- Record total time from the user action or command to visible results, including queuing.
- With a fixed test set, retrieval content, ordering and positions stay identical. Local-retrieval median and P95 may increase by no more than 10% over the baseline.
- Repeated quick queries on the same machine and knowledge base must stay below 1 second. Record the actual baseline before migration.
- Repeated commands reuse caches rather than traversing and decoding the entire source collection each time.
- Record service and client memory usage. They must not retain two full retrieval-cache copies.

These are implementation acceptance criteria, not claims of achieved performance.

## 7. Main risks and responses

| Issue | Response |
| --- | --- |
| Migration omissions across the broad desktop interface | Inventory commands and events first; verify each stage against that list |
| Transport adds latency and copying | Reuse persistent connections, bound ordinary messages, chunk large files and time each part |
| Two clients start a service simultaneously | Single-instance lock, readiness handshake and atomic connection-record publication |
| Service upgrade interrupts tasks | Explicit in-flight task handling and client-visible completion reasons |
| Caches fail to refresh after file changes | Retain index-version invalidation and test cross-client changes |
| Development and release overwrite each other | Isolate development artifacts and update release installations by build identity |
| Long-lived background process | Provide status and stop actions; promptly free request resources after disconnection |

## 8. Recommended order and completion criteria

Finish stage 0 first, delivering version-synchronization protection separately. Validate the independent service in isolated data directories during stages 1–2, migrate desktop during stages 3–4, then validate upgrades and releases in stage 5.

Mark the overall migration complete only when all conditions hold:

1. Every targeted desktop and CLI operation passes through the same service.
2. Desktop no longer opens the database or runs the retrieval core directly.
3. Source-text, index, access-control, cancellation and concurrency checks all pass.
4. Same-machine, same-knowledge-base performance comparisons meet the criteria, with actual end-to-end timings recorded.
5. One build can be installed through desktop or CLI alone, and upgrades leave no active service with a mismatched version.

## 9. Implementation and local acceptance (2026-10-18)

The code migration for stages 0–5 is complete. Desktop depends only on protocol and client modules. The SAG service owns data, watchers, indexing, retrieval and model clients. Original files, database format and retrieval parameters retain their existing settings. Development artifacts are isolated in `src-tauri/bin/development/`, and managed release installations cannot be overwritten by development builds.

| Scenario | Local result |
| --- | --- |
| Standalone CLI | First-run configuration, add, semantic search, local matching, removal and terminal installation tests passed |
| Simultaneous startup | Two CLIs added directories simultaneously; subsequent queries reused one PID |
| 20-way concurrency | All 20 model requests arrived before the first response was released; 19 completed and 1 was independently cancelled |
| Timeout and approval | Timeouts did not affect other connections; tool approvals went only to the initiating connection |
| Concurrent chunking on one connection | A chunked request being received was unaffected by completion of another request |
| Large HTML | An 18 MiB file matched byte for byte after chunked transfer; desktop HTML and Markdown rendering and highlighting worked |
| Service lifecycle | CLI worked after desktop exit; restart and recovery after abnormal termination worked; desktop reconnected and showed CLI-added directories |
| Real knowledge base | Three existing roots, history and settings were retained; desktop quick search covered 432 files, with an initial measured time of 673 ms |
| Performance comparison | Same local machine and knowledge-base copy, 5 fixed queries, 10 warm runs each; content, ordering, positions and totals matched |
| Warm-query median | Direct call: 44.96 ms; standalone-service end-to-end: 47.03 ms; increase: 4.62% |
| Warm-query P95 | Direct call: 366.09 ms; standalone-service end-to-end: 372.90 ms; increase: 1.86% |

BM25 scores differed by at most 3.56×10⁻¹⁵ in floating-point trailing digits after a JSON round trip, without affecting order. All warm queries were below 1 second. This comparison covers only local quick search, excluding remote-model latency. It does not establish that grep scanning is faster.

Local checks passed 467 tests, with 3 manual or external-network tests skipped: frontend 211, core 204, CLI 47, client 2 and protocol 3. Type checking passed. CLI and desktop also compiled in an independent staging directory. Model and external MCP migration tests used local mock models and MCP subprocesses. This migration sent no additional real documents to external models. Release signing, public one-command installation and Windows-machine upgrades require release-time validation. No installer was generated in this migration.

Search previews retain the user's later explicit requirement: the list shows snippets, and the right side renders Markdown / HTML directly. They must not revert to showing only unrendered text.
