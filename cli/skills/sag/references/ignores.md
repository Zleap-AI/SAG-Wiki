# Indexing exclusions

For task-relevant indexing exclusions, you may create or update `.sagignore` in the current project, for example to exclude generated output or caches. Read existing `.gitignore` and `.sagignore` rules first, preserve them, and add only the patterns needed for the task. Use `.sagignore` for SAG-specific exclusions; changing version-control rules in `.gitignore` requires a version-control reason. Report the paths or patterns you changed. Keep source files included unless the user requests their exclusion, and do not override existing exclusions merely to obtain search results.

SAG reads `.gitignore` and `.sagignore` within each knowledge-base root and its subdirectories, including projects without Git. Rules outside that root do not apply. Use Git ignore syntax: `output/` excludes a directory, `**/*.generated.md` matches generated files at any depth, and `!` introduces an inclusion exception.

When combining rules, apply this precedence to each reachable path:

- `.sagignore` takes precedence over `.gitignore`, even when the `.gitignore` is in a deeper directory. A `.sagignore` exception can restore a Git-ignored file, and a `.sagignore` exclusion can exclude a file kept by a Git exception.
- Within the same file type, rules in deeper directories take precedence over rules in parent directories. Within one file, the last matching rule wins. Patterns are relative to the directory containing the rule file.
- A file exception only works when every parent directory is included. If a parent directory is ignored, restore it first; rules inside an ignored directory cannot restore it.

For example, if the root `.gitignore` contains `output/`, adding only `!output/keep.md` to `.sagignore` will not restore the file. To include that file while keeping the other contents of `output/` ignored, use this root `.sagignore`:

```gitignore
!output/
output/*
!output/keep.md
```

For deeper paths, repeat the directory inclusion and child exclusion at every level. With the same root `.gitignore` containing `output/`, use this complete root `.sagignore` to restore only `output/level1/level2/level3/keep.md` from that directory tree:

```gitignore
!output/
output/*
!output/level1/
output/level1/*
!output/level1/level2/
output/level1/level2/*
!output/level1/level2/level3/
output/level1/level2/level3/*
!output/level1/level2/level3/keep.md
```

Each `!path/` restores the next directory; the following `path/*` keeps its contents ignored until the next explicit exception. Keep this order and every directory/exclusion pair: omitting a required ancestor inclusion blocks the target file, while omitting a child exclusion can include unwanted siblings. The final file exception restores only `keep.md`; other files in the same directory, higher levels and sibling directories stay ignored.

Rules affect indexing and search without deleting local files. Standalone searches refresh the index before searching. With the desktop running, changes apply automatically when file watching is enabled; otherwise reindex from File settings. Negation does not override manually removed subdirectories or the directory-name exclusion settings; use `sag add` to restore a manually removed subdirectory when requested.
