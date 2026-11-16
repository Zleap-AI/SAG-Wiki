# Preview benchmark

## Sidebar local-file benchmark

This entry mounts the actual `App.svelte`, then clicks the file in the sidebar.
It preserves the product's current opening behavior. It substitutes only the
native API: `read_document` fetches a configured UTF-8 Markdown file from a
loopback-only server. No real fixture, path, document name or body is committed.

```bash
npx vite build -c bench/document.config.ts
SAG_DOCUMENT_FIXTURE='/absolute/path/to/local-book.md' npx vite preview -c bench/document.config.ts
# Open http://127.0.0.1:1438/bench/document.html
# Development alternative: SAG_DOCUMENT_FIXTURE='...' npx vite -c bench/document.config.ts
```

Keep the file and its relative image assets unchanged between before/after runs.
The server exposes only that fixture and declared, existing inline Markdown or
quoted HTML raster images under its parent directory. It rejects external and
absolute references, traversal, symlinks, undeclared assets and hostile origins or
hosts. Image responses use opaque integer IDs and actual bytes, returned through
the same `load_image` data URI contract as the native app. Unsupported reference
image syntax remains unavailable; check `images.declared/completed/failed`.

In browser developer tools (or a native WKWebView JavaScript runner):

```js
// Wait for window.__SAG_DOCUMENT_BENCHMARK__.ready === true, then:
const result = await window.__SAG_DOCUMENT_BENCHMARK__.run();
JSON.stringify(result, null, 2);
// The latest report is also synchronous:
window.__SAG_DOCUMENT_BENCHMARK__.report();
window.__SAG_DOCUMENT_BENCHMARK__.snapshot(); // only document trace samples
window.__SAG_DOCUMENT_BENCHMARK__.reset(); // explicit, only while idle
```

`run()` returns a Promise for the report after the trace completes/cancels and
in-flight image calls settle (180-second document timeout; 30-second image
timeout). It returns through the folder route before each measured sidebar click
so repeated runs mount fresh document components. Concurrent runs are rejected.
`?autorun=1` starts once when ready; runners that call `run()` should omit it.
The global deliberately retains no document content or local file path.

The JSON includes fixture byte count/SHA-256, browser/viewport, stage durations,
first-content-paint latency, worst animation-frame gap, bounded trace samples,
long-task aggregates where supported, before/after DOM counts, image RPC counts
and transferred bytes, and optional browser heap samples. First paint means
component microtasks followed by two animation-frame callbacks; it does not wait
for every image to decode. Image hydration completion has a separate timestamp.
Unsupported heap/long-task APIs report `null`/`supported:false`, not zero-cost
claims. DOM counting runs only before and after measurement. Frame gaps start at
the measured sidebar click, including work before the first animation frame, and include
the final gap if navigation cancels before the next frame. `cancelled`, `timeout`
or `error` are failures to inspect, not successful opening measurements.

For a baseline, use an optimized production build, a foreground native macOS
WKWebView for the desktop acceptance path, a fixed viewport, and the exact same
fixture hash. Record at least five fresh-page samples and report median/p95.
Keep cold component loading separate from warm repeated `run()` calls; avoid
parallel builds/tests while timing. A normal browser is useful for diagnosis,
but its results should be labeled separately from native WKWebView measurements.
The loopback RPC includes disk read, fixture image-whitelist validation, HTTP and
JSON costs; measure native Rust `read_document` separately when attributing
backend versus rendering time.

### Trace interface for readers/editors

`src/lib/document-performance.ts` exports `configureDocumentPerformance`,
`beginDocumentTrace`, `cancelDocumentTrace`, `documentPerformanceSnapshot`,
`resetDocumentPerformance` and `afterDocumentPaint`. Collection is off by default;
the benchmark explicitly enables it. Disabled traces share a no-op handle and
perform no clock reads or frame scheduling. `beginDocumentTrace(renderer)` cancels
the previous active trace. A trace exposes `enabled`, `active`, `stage(name)`
(returns an idempotent stop function), `setRenderer(renderer)`, `firstPaint()`,
`end()` and `cancel()`. Set the resolved renderer after the RPC (for example,
a PDF source that resolves to converted Markdown), preserving trace ID/start time.
`afterDocumentPaint(trace, stillCurrent)` checks cancellation/disposal before
scheduling either frame and completes the trace after its first-content paint.

Stage names are closed: `read-rpc`, `target`, `model`, `component`, `editor-init`,
`reader-render`, `reader-dom`, `reader-index`, `reader-images`. Repeated stages
aggregate `{durationMs,count}` rather than retaining per-block events. Renderers
are `editor`, `reader`, `readonly`. Snapshots include active/completed/cancelled
records and are detached copies, capped at 30 by default (configurable 1–100).
No paths, names, terms, body text or arbitrary metadata are accepted. Invalid
runtime stage/renderer values cannot add document content to the closed schema.
`resetDocumentPerformance()` cancels pending handles and drops all samples;
disabling collection cancels the current trace.

### Native macOS WebView runner

The standalone runner uses a fresh, nonpersistent system `WKWebView`, fixed at
1280 × 850, and drives the same sidebar benchmark. It accepts a loopback HTTP
URL only, leaves installed app/browser data alone, and ignores physical mouse
clicks so a user's unrelated click cannot contaminate a run.

```bash
xcrun swiftc bench/native-document.swift -o /tmp/sag-document-benchmark -framework AppKit -framework WebKit
/tmp/sag-document-benchmark --url http://127.0.0.1:1438/bench/document.html --output /tmp/sag-document-result.json
```

Check the JSON's completed trace, browser errors, viewport, invocation counts,
and fixture hash before accepting a sample; process completion alone is not a
passing result. Do not append `?autorun=1`, because the runner invokes `run()`.
Repeat with a fresh process for cold module samples; `--runs 21` measures the
first open plus 20 warm reopens in one page, with each report in `browser.runs`.
This measures the real App/sidebar/renderer in native
WebKit with a simulated Tauri command bridge and local fixture fetch. It does
not measure Tauri IPC or a packaged native app.

The benchmark uses the desktop's script, style, image and worker CSP directives.
Its `connect-src 'self'` is specific to the loopback fixture bridge; desktop IPC
permissions stay in the application configuration.

`feedback.firstPaintMs` measures the first loading indicator or readable content
that survives a frame's paint opportunity; it is distinct from the document
trace's first readable paint. `cache` contains live numeric model/HTML usage,
loaded only after the first-paint/frame measurement to avoid prewarming the
reader. No book content or private path is included. Native WebKit may expose
neither JS heap nor long-task observations; null/unsupported fields must not be
interpreted as zero memory use or absence of blocking work.

`referenceMatches(terms)` performs an explicit whole-document reference render
and returns only the match count for a correctness comparison. Invoke it after
timing measurements, since the reference intentionally does more work than the
windowed reader. It is a benchmark helper, not part of the product reader.

After building the document benchmark, run the real-layout image/reflow check
with the same optional Playwright setup used by documentation screenshots:

```bash
node bench/reader-layout-check.cjs /tmp/sag-reader-layout.json
```

It creates its own temporary synthetic Markdown/PNG fixture and loopback preview
server, then verifies a visible text range after a delayed image and observes the
reader's actual browser caret sample before a width change. Fixture files,
browser and server are removed afterward. It fails if the image moves the range
more than 2 px, the sampled character moves more than 2 px after 3 or 10 further
animation frames, or a browser error occurs. The earlier reference word's resize
movement is diagnostic: other words on a line may wrap while the sampled visible
character stays fixed. No user documents are read by this check.

## Generated preview benchmark

Opens a generated, Chinese-first Markdown book the way the app does and reports
how long the cited passage takes to appear.

```bash
npm run dev
# then open, in Safari or Chrome:
# http://127.0.0.1:1420/bench/preview.html?mb=5&mode=citation
# http://127.0.0.1:1420/bench/preview.html?mb=5&mode=search
```

`visible` is the time until the current match is highlighted inside the
viewport, `complete` the time until the match counter stops changing, and
`worstFrame` the longest gap between animation frames. Safari runs the same
JavaScriptCore engine as the macOS app. `fixtures.ts` also feeds
`tests/markdown-chunks.test.ts`.

For numbers close to the packaged app, benchmark a production build:

```bash
npx vite build -c bench/vite.config.ts
npx vite preview -c bench/vite.config.ts --host 127.0.0.1
# http://127.0.0.1:1431/bench/preview.html?mb=5&mode=citation
```
