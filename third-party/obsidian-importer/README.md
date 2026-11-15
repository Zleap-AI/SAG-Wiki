# Obsidian Importer conversion engine

Source: https://github.com/obsidianmd/obsidian-importer
Commit: fa9444a188459f227b07ec121ad03d26bd6b301a (2026-11-12)
License: MIT; see LICENSE. Individual source notices are retained.

This is the unmodified transitive source subset used by Notion HTML ZIP,
Evernote ENEX, Google Keep Takeout and Apple Notes conversion.
SAG adapters and build configuration live in /importer and
/scripts/build-importer.mjs. The browser shim is upstream's own web host.
Apple Notes uses a read-only native database snapshot and static protobuf
code generated at build time, with no dynamic code evaluation at runtime.
Generated bundles are ignored and rebuilt by npm run build:importer.

Regression fixtures in /tests/fixtures/note-import derive from the same
MIT-licensed upstream tests. apple.sql contains synthetic notes only.
