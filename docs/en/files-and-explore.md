# Files and Explore design

[简体中文](../zh-CN/文件与探索视图设计.md) · [Documentation index](README.md)

## Workspace and state

The knowledge-base workspace provides Files and Explore views. Directory browsing is managed separately from Q&A and search scope. Switching folders preserves each session's mode, scroll position, graph viewport, selected document and exploration history.

Main implementation entry points:

| Path | Responsibility |
| --- | --- |
| `src/components/LibraryWorkspace.svelte` | Workspace mode and file list |
| `src/components/ExploreView.svelte` | Relationship-graph page, document selection, local exploration and evidence |
| `src/components/ExploreGraph.svelte` | Node rendering, layout, zooming and dragging |
| `src/lib/explore.ts` | Session data structures, graph conversion and history state |
| `src/lib/graph-layout.ts` | Primary edges, viewport, layout transitions and folder colors |
| `src-tauri/src/explore.rs` | Local relationship calculations, snapshots, pagination and evidence interfaces |

## Files view

- Clicking a directory name opens it; clicking the tree arrow only expands or collapses it.
- Adding a folder submits background indexing and displays actual progress and errors.
- Markdown is editable. Other supported formats use read-only views.
- Removing a knowledge-base directory preserves the original files. Removed subdirectories remain excluded and can be restored from settings.
- External file changes refresh directory data without resetting current input or scroll position.

## Relationship data

Document relationships are calculated from locally derived terms and TF-IDF weights. Edges indicate content similarity. Scores do not measure answer correctness, document citations or causality. Generating the graph does not call Jev or the answer model.

The interface returns the current scope, index version, document information and relationships identified by a snapshot. Document hashes and index versions identify stale results. File changes refresh the relevant caches and graph data; invalidated evidence must be fetched again.

## Overview and local exploration

Opening Explore displays an overview of relationships in the current scope. The overview emphasizes primary relationships while preserving connectivity, reducing persistent display of many weak edges. Hovering over or selecting a document shows its direct relationships that meet the threshold.

The search box locates documents. A single click opens the side preview; double-clicking or choosing **View local relationships** enters that document's local relationships. **Back** and **Relationship overview** restore the corresponding state.

Relationship strength controls the display threshold. The UI shows actual document and edge counts, along with states such as indexing or partial failure.

## Rendering and interaction

The graph uses Sigma, Graphology and ForceAtlas2. Algorithm data is separate from frontend display state to reduce repeated computation while dragging or zooming.

- Nodes use stable colors based on their parent folders, with light- and dark-theme support.
- File names are not permanently shown by default; hovering displays the full name.
- Nodes and edges move together during layout. The viewport stays stable after convergence.
- Dragging pans the view or moves nodes; the mouse wheel zooms. An explicit fit-to-window action is provided.
- The system's reduced-motion preference uses a static layout.
- Graph components load on demand. Leaving the page cancels unfinished data requests and animations.

## Documents and evidence

Selecting a document displays its name, path, body preview and related documents in the sidebar. Opening it uses the desktop's shared document entry point.

Relationship evidence is fetched through a separate interface that returns source passages from both documents, their file versions and shared terms. Viewing evidence does not trigger model requests. An invalid index version produces an explicit refresh prompt.

## Implementation scope

The current graph colors documents by folder; colors do not imply automatic topic classification. Document relationships use local term statistics. Natural-language questions that require semantic judgments continue to use SAG2 retrieval. Dependency attribution and licenses are in `src-tauri/licenses/GRAPH-VIEW.txt`.
