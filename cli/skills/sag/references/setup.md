# Knowledge base and setup

sag runs independently; SAG-Desktop is optional. Both use the same core, knowledge base, settings and credentials. If the desktop service is running, sag reuses it; otherwise it runs the core in the CLI process. Search automatically uses indexed knowledge-base folders within the requested directory, or the requested subtree of an indexed project; there is no per-folder CLI authorization setting. If semantic search needs JEV configuration, ask the user to open a terminal and run `sag`; first-use setup guides them through the endpoint, model and hidden API-key input. Noninteractive calls do not prompt. Never invent credentials or change their provider automatically.

When the current project has not been indexed, run `sag add` in its directory yourself. This adds it to the knowledge base and starts indexing. Check `sag status`; if indexing is still in progress, wait for it to finish before searching. No sync flag is needed. Do not add unrelated projects or broaden the search to a parent directory merely to find more results. If the user has turned CLI off, report that setting instead of enabling it yourself. When they explicitly request re-enabling it, use `sag config --enable`.

When the user asks to remove a project from the knowledge base, run `sag remove` in that project or pass a registered root directory or a subdirectory. This removes only knowledge-base records and indexes; local files remain unchanged. A removed subdirectory stays excluded from automatic indexing. When the user asks to restore it, run `sag add` for that same subdirectory.

If a command fails for another reason, report its reason and continue with the harness's normal tools; do not repeat it without a relevant change.
