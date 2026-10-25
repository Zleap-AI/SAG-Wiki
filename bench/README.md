# Preview benchmark

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
