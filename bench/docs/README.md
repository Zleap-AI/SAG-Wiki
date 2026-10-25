# Documentation screenshots

The screenshot fixture mounts the current application and its real components.
All service responses are synthetic and in memory: no user files, credentials,
models, update services or MCP servers are accessed. Search timings, index runs
and graph edges illustrate UI states; they are not performance or backend test
results. Browser-extension screenshots are maintained separately.

The initial refresh uses GitHub main `680a342` (including the search redesign).
These are main-branch UI screenshots, not a claim that every illustrated change
is included in the latest released installer.

From the repository root, install the locked application dependencies and make
Playwright available after `npm ci` (or set `PLAYWRIGHT_MODULE` to an existing
Playwright module). Google Chrome is
required by the capture scripts.

```sh
npm ci
npm install --no-save --package-lock=false playwright
npx vite build -c bench/docs.config.ts
npx vite preview -c bench/docs.config.ts
# In another terminal:
node bench/capture-docs.cjs
```

The preview binds to loopback port 1437. Captures use a 1280 × 900 light window
and localized, fictitious project documents. Scripts wait for rendered Markdown,
citations and dialogs, and fail on browser page errors. The four README scenes
are copied from the corresponding documentation screenshots. Review both
languages visually after capture; rendering checks are not an app acceptance
test. The `many` and `diagram` fixture options demonstrate scrolling and Mermaid.

Refresh the matching bilingual MDX instructions whenever navigation or controls
change. Preserve extension screenshots, branding, community QR codes and
benchmark charts unless their owning features change.
