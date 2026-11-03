# Document reading layers

Adapted from TextFold `ladder.py` and `tokens.py` at commit `84427249f932e05e781539c0f3960658ffcbe966` (Apache-2.0). The prompt template is derived from upstream `prompt_for`, with “paper” generalized to “document”. License and attribution: `src-tauri/licenses/TEXTFOLD.txt`.

Preserved core behavior:
- Chapter-based planning; balanced parts of at most 40 units; four concurrent requests.
- Title, abstract/opening, outline and section table context (up to three tables, 1,500 characters each).
- Separate takeaway/reason/topic/evidence/role fields; 40/60/8 Chinese budgets and 14/22/3 word budgets.
- Original sentence extraction, normalization of reply shapes and evidence indices, explicitly marked local levels for short or missing units.
- Chapter summaries after split requests; Chinese background suppression and repeated-summary removal; paragraph continuation repair.
- Successful request caching and no additional reasoning for layer generation.
- Request-scoped progress: completed paragraphs and section summaries, including cache hits; adaptive splits share their original work allocation. Only a fully assembled and persisted document reaches 100%.
- The frontend embeds the unmodified upstream reader for source-word matching, continuous zoom, FLIP transitions, topic cards, and local paragraph expansion. See `public/textfold/README.md`.

SAG adaptations:
- Existing Markdown editor supplies structure; existing table, figure and code markup stays intact.
- OpenAI-compatible JSON mode uses the existing model service; no output token cap is added.
- Cache keys deliberately exclude model identity, as requested. Complete document results are also cached.
- A provider context/output limit splits only the affected part; failed batches remain visible as errors with completed batches retained.
- PDF extraction, translation, question answering and argument-link generation remain outside this module.

Verification: `cargo test --manifest-path core/Cargo.toml document_fold --lib`; `npx vitest run tests/document-fold.test.ts`.
