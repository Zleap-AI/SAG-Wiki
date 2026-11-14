use sag_core::anydoc;
use std::{fs, io::Write};
fn document(path: &std::path::Path, ppt: bool, asset_bytes: usize) {
    use zip::{write::SimpleFileOptions, CompressionMethod, ZipWriter};
    let mut z = ZipWriter::new(fs::File::create(path).unwrap());
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let (main, kind, body) = if ppt {
        (
            "ppt/presentation.xml",
            "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml",
            r#"<p:presentation xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:sldIdLst><p:sldId id="256" r:id="s1"/></p:sldIdLst></p:presentation>"#,
        )
    } else {
        (
            "word/document.xml",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml",
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><w:body><w:p><w:r><w:t>officetexttoken</w:t></w:r></w:p><w:p><w:r><w:drawing><a:blip r:embed="img1"/></w:drawing></w:r><w:r><w:drawing><a:blip r:embed="img2"/></w:drawing></w:r></w:p></w:body></w:document>"#,
        )
    };
    let prefix = if ppt { "ppt" } else { "word" };
    let mut add = |name: &str, body: &str| {
        z.start_file(name, opts).unwrap();
        z.write_all(body.as_bytes()).unwrap();
    };
    add(
        "[Content_Types].xml",
        &format!(
            r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Override PartName="/ppt/slides/slide1.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slide+xml"/><Override PartName="/{main}" ContentType="{kind}"/><Override PartName="/{prefix}/charts/picture.xml" ContentType="image/svg+xml"/></Types>"#
        ),
    );
    add(
        "_rels/.rels",
        &format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="main" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="{main}"/></Relationships>"#
        ),
    );
    add(main, body);
    if ppt {
        add(
            "ppt/_rels/presentation.xml.rels",
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="s1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide" Target="slides/slide1.xml"/></Relationships>"#,
        );
        add(
            "ppt/slides/slide1.xml",
            r#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>officetexttoken</a:t></a:r></a:p></p:txBody></p:sp><p:pic><p:blipFill><a:blip r:embed="img1"/></p:blipFill></p:pic><p:pic><p:blipFill><a:blip r:embed="img2"/></p:blipFill></p:pic></p:spTree></p:cSld></p:sld>"#,
        );
    }
    let rel = if ppt {
        "ppt/slides/_rels/slide1.xml.rels"
    } else {
        "word/_rels/document.xml.rels"
    };
    add(
        rel,
        &format!(
            r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="img1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{up}media/a.png"/><Relationship Id="img2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/image" Target="{up}media/b.png"/></Relationships>"#,
            up = if ppt { "../" } else { "" }
        ),
    );
    for name in ["a.png", "b.png"] {
        z.start_file(format!("{prefix}/media/{name}"), opts)
            .unwrap();
        let chunk = [0u8; 65536];
        for _ in 0..asset_bytes / 65536 {
            z.write_all(&chunk).unwrap();
        }
    }
    // Tripwires: neither binary objects nor an image disguised as XML may be parsed.
    z.start_file(format!("{prefix}/embeddings/object.bin"), opts)
        .unwrap();
    z.write_all(b"embedded object: not XML").unwrap();
    z.start_file(format!("{prefix}/charts/picture.xml"), opts)
        .unwrap();
    z.write_all(b"<invalid SVG payload that must never be read")
        .unwrap();
    z.finish().unwrap();
}
#[test]
fn office_text_only_skips_assets_before_the_asset_limit() {
    for ppt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir
            .path()
            .join(if ppt { "report.pptx" } else { "report.docx" });
        let output = dir.path().join("out.md");
        document(&input, ppt, 70 * 1024 * 1024);
        let normal = sag_protocol::runtime::RuntimeSettings::default();
        let error = anydoc::convert_with_runtime(&input, &output, &normal, || false).unwrap_err();
        assert!(
            format!("{error:#}").contains("max_asset_total_bytes"),
            "{error:#}"
        );
        let text = serde_json::from_value(serde_json::json!({"office_text_only":true})).unwrap();
        anydoc::convert_with_runtime(&input, &output, &text, || false).unwrap();
        assert!(fs::read_to_string(&output)
            .unwrap()
            .contains("officetexttoken"));
    }
}

#[test]
#[cfg(unix)]
fn text_only_failures_cannot_fall_back_and_mode_changes_retry() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("files");
    fs::create_dir(&root).unwrap();
    let source = root.join("bad.docx");
    fs::write(&source, b"not an OOXML package").unwrap();
    let home = dir.path().join("docling");
    fs::create_dir_all(home.join("bin")).unwrap();
    let script = home.join("bin/docling");
    fs::write(
        &script,
        r#"#!/bin/sh
set -eu
shift
out=''
while [ "$#" -gt 0 ]; do
 if [ "$1" = '--output-file' ]; then shift; out="$1"; fi
 shift
done
touch "$(dirname "$0")/unsafe-called"
printf 'fallbacktoken' > "$out"
"#,
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(home.join("ready"), "2.140.0").unwrap();
    let store = sag_core::store::Store::open(&dir.path().join("index.sqlite")).unwrap();
    let key = store.add_root(&root).unwrap();
    let mut settings = store.settings().unwrap();
    assert!(!settings.runtime.office_text_only);
    settings.runtime.office_text_only = true;
    store.save_settings(&settings).unwrap();
    store.scan(&key, |_| {}).unwrap();
    assert!(!home.join("bin/unsafe-called").exists());
    assert!(store.files().unwrap().is_empty());
    settings.runtime.office_text_only = false;
    store.save_settings(&settings).unwrap();
    store.scan(&key, |_| {}).unwrap();
    assert!(home.join("bin/unsafe-called").exists());
    assert_eq!(
        store.search_files("fallbacktoken", &[], 10).unwrap().len(),
        1
    );
}

#[test]
fn text_mode_is_opt_in_and_does_not_change_legacy_formats() {
    let mut r = sag_protocol::runtime::RuntimeSettings::default();
    assert!(!anydoc::office_text_only(
        std::path::Path::new("x.docx"),
        &r
    ));
    r.office_text_only = true;
    for ext in ["docx", "PPTX"] {
        assert!(anydoc::office_text_only(
            std::path::Path::new(&format!("x.{ext}")),
            &r
        ));
    }
    for ext in ["doc", "ppt", "docm", "pptm", "pdf", "xlsx"] {
        assert!(!anydoc::office_text_only(
            std::path::Path::new(&format!("x.{ext}")),
            &r
        ));
    }
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("file.docx");
    document(&input, false, 0);
    let output = dir.path().join("old.md");
    fs::write(&output, "old complete output").unwrap();
    assert!(anydoc::convert_with_runtime(&input, &output, &r, || true).is_err());
    r.max_decoded_text_mib = 0;
    assert!(anydoc::convert_with_runtime(&input, &output, &r, || false).is_err());
    assert_eq!(fs::read_to_string(&output).unwrap(), "old complete output");
}

#[test]
fn ordinary_fixture_text_is_preserved_and_settings_survive_reopen() {
    for name in ["docx-text.docx", "pptx-pres.pptx"] {
        let dir = tempfile::tempdir().unwrap();
        let input = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/anydoc")
            .join(name);
        let plain = dir.path().join("plain.md");
        let restricted = dir.path().join("text.md");
        let mut runtime = sag_protocol::runtime::RuntimeSettings::default();
        anydoc::convert_with_runtime(&input, &plain, &runtime, || false).unwrap();
        runtime.office_text_only = true;
        anydoc::convert_with_runtime(&input, &restricted, &runtime, || false).unwrap();
        // Images render differently, but every ordinary textual line must survive.
        let original = fs::read_to_string(plain).unwrap();
        let text = fs::read_to_string(restricted).unwrap();
        for line in original
            .lines()
            .filter(|l| !l.trim().is_empty() && !l.contains("!["))
        {
            assert!(text.contains(line), "missing {line:?} in {name}");
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("index.sqlite");
    {
        let store = sag_core::store::Store::open(&db).unwrap();
        let mut settings = store.settings().unwrap();
        settings.runtime.office_text_only = true;
        store.save_settings(&settings).unwrap();
    }
    assert!(
        sag_core::store::Store::open(&db)
            .unwrap()
            .settings()
            .unwrap()
            .runtime
            .office_text_only
    );
}

#[test]
fn corrupt_resource_compression_is_never_opened() {
    use std::io::{Read, Seek, SeekFrom};
    for ppt in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join(if ppt { "file.pptx" } else { "file.docx" });
        document(&input, ppt, 65536);
        let prefix = if ppt { "ppt" } else { "word" };
        for member in [
            format!("{prefix}/media/a.png"),
            format!("{prefix}/embeddings/object.bin"),
            format!("{prefix}/charts/picture.xml"),
        ] {
            let offset = {
                let mut archive = zip::ZipArchive::new(fs::File::open(&input).unwrap()).unwrap();
                let entry = archive.by_name(&member).unwrap();
                entry.data_start()
            };
            let mut file = fs::OpenOptions::new().write(true).open(&input).unwrap();
            file.seek(SeekFrom::Start(offset)).unwrap();
            file.write_all(&[0xff; 4]).unwrap();
            // Demonstrate that reading the member really fails, rather than testing an inert fixture.
            let mut archive = zip::ZipArchive::new(fs::File::open(&input).unwrap()).unwrap();
            assert!(archive
                .by_name(&member)
                .unwrap()
                .read_to_end(&mut Vec::new())
                .is_err());
        }
        let mut runtime = sag_protocol::runtime::RuntimeSettings::default();
        runtime.office_text_only = true;
        let output = dir.path().join("text.md");
        anydoc::convert_with_runtime(&input, &output, &runtime, || false).unwrap();
        assert!(fs::read_to_string(output)
            .unwrap()
            .contains("officetexttoken"));
    }
}

#[test]
fn text_only_stays_local_in_both_scan_lanes_even_when_cloud_is_enabled() {
    for progressive in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("files");
        fs::create_dir(&root).unwrap();
        for ppt in [false, true] {
            document(
                &root.join(if ppt { "file.pptx" } else { "file.docx" }),
                ppt,
                0,
            );
        }
        let store = sag_core::store::Store::open(&dir.path().join("index.sqlite")).unwrap();
        let mut settings = store.settings().unwrap();
        settings.runtime.office_text_only = true;
        settings.mineru.enabled = true;
        // No cloud credentials or downloaded parser exists in this isolated store.
        store.save_settings(&settings).unwrap();
        let key = store.add_root(&root).unwrap();
        if progressive {
            store.scan_basic(&key, |_| {}).unwrap();
        } else {
            store.scan(&key, |_| {}).unwrap();
        }
        assert_eq!(
            store
                .search_files("officetexttoken", &[], 10)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(store.roots().unwrap()[0].status, "ready");
    }
}
