# Large PDF preparation: diagnosis and validation

## Trigger and cause

A local 1,283-page PDF (20,002,289 bytes, 135,197 lopdf objects) left the UI at
154/191 files. The service's GetIndexStatus identified that PDF as the active
conversion. No cloud task journal existed for it and there were no outbound
network connections: preparation had not reached upload.

CDB non-invasive thread sampling (`!runaway; ~* k`) showed one worker with
44m52s of user CPU and other workers mostly waiting. Release symbols were
insufficient to assign precise Rust function names; this is **not** a claimed
symbolized function profile. An isolated release-mode lopdf 0.45.0 reproduction
then measured loading at 0.159s and deleting just one page at 7.292s.
`delete_pages` calls `delete_object` for each removed page, which traverses the
object graph. Keeping 200 pages deletes 1,083 pages from a full clone, and each
subsequent segment repeats this work. We did not wait for the old full split to
finish and do not extrapolate its duration as a measured benchmark.

## Measured comparison (same Windows machine and source)

| Path | Measured operation | Time |
| --- | --- | --- |
| Old lopdf | Load original PDF | 0.159s |
| Old lopdf | Delete **one** page | 7.292s |
| pikepdf/QPDF prototype | Seven parts plus reopen checks | 1.590s |
| Bundled QPDF CLI | Seven separate page-selection processes | 4.030s |
| Bundled QPDF CLI, repeat with deterministic IDs | Seven processes, verification excluded from timer | 2.429s |
| Application preparation regression | Prepare seven parts, validate each page count, reload source | 7.49s |

These operations differ; the table does not claim a numerical full-pipeline
speedup. No network or MinerU processing time is included. Output page counts
are 200, 200, 200, 200, 200, 200, 83. Original and QPDF output pages
1, 200, 201, 400, 1201 and 1283 rendered to identical pixels at half scale.
This checks representative boundaries, not every page or document-level feature.

## Reproduction and scope

The source is a user's local manual and is not distributed in the repository.
Its SHA256 is `e99228ac11395f865474f6faf4b4d140bab0554121c5f67931e801b1115e24a9`.
Run `python scripts/benchmark-qpdf.py INPUT.pdf PATH_TO_QPDF` (requires pypdf), or
set `SAG_TEST_LARGE_PDF` and run:

```sh
node scripts/prepare-qpdf.mjs
cargo test --manifest-path core/Cargo.toml --lib real_large_pdf_preparation_with_qpdf -- --ignored
```

The synthetic 201-page regression runs normally, including shared later-segment
task reuse. `--deterministic-id` prevents random PDF IDs from defeating journal
reuse. Existing completed conversion identities are unchanged. The helper uses
the existing process supervisor for cancellation and a 120-second split deadline.
Page selection is intended for OCR input, not preservation of bookmarks or
other document-level metadata. Original PDFs are never modified.

QPDF 12.4.2 archives are pinned by SHA256 for Windows/macOS/Linux; desktop and
CLI packages include the helper, libraries and license notices. Windows runtime
was tested locally. macOS archive layout was inspected but execution/signing
must be verified by macOS CI; no local macOS success is claimed.
