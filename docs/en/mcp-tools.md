# SAG built-in MCP tools

[简体中文](../zh-CN/SAG内置MCP工具.md) · [Documentation index](README.md)

SAG's built-in MCP provides six read-only tools. The client discovers and calls them through MCP within the same Rust process. It does not expose an HTTP port or start another program. `src/lib/sag-tools.json` is the single source of tool definitions, shared by the frontend catalog and Rust MCP implementation.

Model-facing names have the `sag2_` prefix to avoid collisions with other services. The UI displays `SAG · search`, `SAG · list`, `SAG · read`, `SAG · grep`, `SAG · getTime` and `SAG · calculate`.

## Parameters

All offsets are zero-based. Paths are absolute and may only narrow the user's selected file scope. Omitting a path uses the current scope. Explicitly referenced files are also permitted in the current round.

### list

Lists files and directories without reading their bodies.

| Parameter | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| path | string | No | Current scope | Restrict to a directory or a single file |
| pattern | string | No | No filter | Match relative paths, such as `**/*.rs` or `**/README*` |
| recursive | boolean | No | true | Include subdirectories |
| offset | integer | No | 0 | Number of entries to skip |
| length | integer | No | 2000 | Maximum entries to return; must be greater than 0 |

Returns `items` (path, type, size, modified_ms), `offset`, `has_more` and `next_offset`. Results follow a stable depth-first traversal by path. Hidden-file display settings and directory exclusions apply. Recursive traversal does not follow symbolic links.

### read

Reads the specified file's UTF-8 source, preserving line breaks. It uses the client's text-decoding rules; a UTF-8 BOM is not counted as body text.

| Parameter | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| path | string | Yes | — | Absolute file path |
| offset | integer | No | 0 | Starting Unicode character position |
| length | integer | No | 2000 | Maximum Unicode characters to read; must be greater than 0 |

Characters are Unicode code points, not UTF-8 bytes. Returns path, offset, length, total_characters, version, has_more, next_offset and a source passage with a source_id. An offset equal to the file length returns empty content; an offset beyond it returns an error.

### grep

Matches literal text in local files without tokenization, semantic expansion or Jev ranking.

| Parameter | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| query | string | Yes | — | Nonempty exact-match text; not interpreted as a regular expression |
| path | string | No | Current scope | Restrict to a file or directory |
| pattern | string | No | No filter | Filter files by relative path |
| case_sensitive | boolean | No | false | Match case |
| context_lines | integer | No | 2 | Lines of context before and after each hit |
| offset | integer | No | 0 | Number of matches to skip |
| length | integer | No | 50 | Maximum matches to return; must be greater than 0 |

Returns matches (path, line, offset, length, context_offset, source_id), original sources and pagination metadata. Multiple occurrences on one line each count as a separate match. File order is stable, and matches within a file are ordered by source position.

### search

Uses the existing SAG2 pipeline: BM25 recall, term expansion, Jev judgments and joint ranking.

| Parameter | Type | Required | Default | Meaning |
| --- | --- | --- | --- | --- |
| query | string | Yes | — | Natural-language question or missing information, 1–2000 characters |
| path | string | No | Current scope | Restrict to a file or directory |
| top_k | integer | No | Retrieval settings; default 10 | Maximum chunks to return, from 1–100, still constrained by candidate count |

Returns query, sources ranked by relevance and locations (path, offset, length, source_id). Locations use the same character units as read, so they can directly fetch context around a hit. Candidate counts, expansion rounds and related values still come from retrieval settings.

### getTime

Accepts no parameters. Reads the local machine's clock and returns `local_time`, `utc_time`, `timezone_offset` and `unix_ms`. Local and UTC times use RFC 3339 with a time-zone offset. Time questions require an actual call rather than reuse of an old result.

### calculate

Requires the string `expression`, for example `{"expression":"sqrt(283298439.39)"}`. The expressions below are supported, up to 2000 bytes and 64 nesting levels. Code, variable assignments and custom functions are not accepted.

| Operation | Syntax |
| --- | --- |
| Arithmetic, remainder, parentheses | `+ - * / % ( )` |
| Powers and square roots | `x^y`, `x**y`, `pow(x,y)`, `sqrt(x)` |
| Scientific notation and constants | `1.2e3`, `5E-2`, `pi`, `e`; constants also support `pi()` and `e()` |
| Absolute value, integer rounding, sign | `abs(x)`, `floor(x)`, `ceil(x)`, `trunc(x)` / `int(x)`, `sign(x)` |
| Rounding | `round(x)`, `round(x,n)`; n is 0–28, with midpoint ties rounded away from zero |
| Maximum and minimum | `max(x,y,...)`, `min(x,y,...)`, with at least one argument |
| Logarithms and exponentials | `ln(x)`, `log10(x)` / `log(x)`, `log(base,x)`, `exp(x)` |
| Trigonometry and angle conversion | `sin(x)`, `cos(x)`, `tan(x)` with radian arguments; `radians(x)`, `degrees(x)` |

Exponentiation is right-associative and has higher precedence than unary negation: `2^3^2 = 512`, `-2^2 = -4`. Function names are case-insensitive.

Calculations use fixed-precision decimal arithmetic. `0.1 + 0.2` returns `"0.3"`. Results are strings to avoid losing precision again through floating-point conversion. Up to 28 decimal places are supported. Scientific functions are approximate; actual accuracy depends on the function and input, and output digit count does not imply that many accurate digits. Operations may round to available precision. Nonterminating decimals do not imply unlimited precision. Division by zero, square roots of negative numbers, invalid logarithms, values outside the numeric range and invalid expressions return explicit errors.

The calculator follows the approach of restricting expression syntax before evaluation used by the [Strands calculator](https://github.com/strands-agents/tools/blob/main/src/strands_tools/calculator.py), and the common function interface of [fasteval](https://github.com/likebike/fasteval). Actual calculations use the `maths` feature of `rust_decimal`, without converting ordinary decimal calculations into binary floating point.

These two tools do not access the knowledge base, are not constrained by file-query scope and do not produce file citations. They follow the per-model enable/disable settings. For time or numeric calculations, the system prompt directs the model to call the appropriate tool first; file retrieval is unnecessary for pure calculations and time queries.

## Combining tools

| Available information | Tool use |
| --- | --- |
| Known file path | Call read directly |
| Only a filename or extension | Filter with list's pattern, then read |
| Function name, keyword or error text | Call grep, then read surrounding context when needed |
| Natural-language question or cross-file lookup | Call search, then read more source text when needed |
| Result has has_more=true | Continue with next_offset |
| Enough evidence | Stop calling tools and answer from the source text |

These rules are included in the MCP descriptions and tool definitions. Models are not required to use a fixed order or every tool.

## Scope, citations and budgets

- All six tools leave original files unchanged. Requests outside registered directories or the round's scope fail; symbolic links cannot expand access.
- read, grep and search share source_id values, file-version hashes and source positions within the round. Identical ranges from the same file version are deduplicated and reuse citation numbers.
- Each tool is enabled independently by default and can be disabled per model.
- SAG and external MCP tools share the round's call limit and answer-context budget. search can omit chunks to fit the budget and explicitly reports omitted_sources. If another tool's result is too large, the complete call record is retained and the model is explicitly asked to narrow its request, avoiding silent truncation or invalid citations.
- Tool arguments, results and failure reasons appear in conversation tool cards, collapsed by default and saved with history.
