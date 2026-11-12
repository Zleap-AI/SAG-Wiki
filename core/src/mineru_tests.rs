use super::*;
use sag_protocol::mineru::{MineruConfig, MineruProvider};
fn default_config()->MineruConfig {MineruConfig{provider:MineruProvider::Ai302,..Default::default()}}

#[test]
fn settings_default_disabled_and_provider_keys_are_separate() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = default_config();
    assert!(!config.enabled);
    assert_eq!(config.provider, MineruProvider::Ai302);
    save_key(dir.path(), &config, "302-secret").unwrap();
    config.provider = MineruProvider::Official;
    assert!(!has_key(dir.path(), &config).unwrap());
    save_key(dir.path(), &config, "official-secret").unwrap();
    assert_eq!(key(dir.path(), &config).unwrap(), "official-secret");
    clear_key(dir.path(), &config).unwrap();
    config.provider = MineruProvider::Ai302;
    assert_eq!(key(dir.path(), &config).unwrap(), "302-secret");
}

#[test]
fn unsupported_formats_stay_local_and_invalid_settings_fail() {
    assert!(supports(Path::new("scan.pdf")));
    assert!(supports(Path::new("slides.PPTX")));
    assert!(!supports(Path::new("code.rs")));
    assert!(!supports(Path::new("notes.md")));
    let mut config = default_config();
    config.task_timeout_secs = 0;
    assert!(config.validate().is_err());
}

#[test]
fn archive_markdown_rewrites_relative_images_without_extracting_paths() {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("full.md", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"# Result\n![figure](images/x.png)\n[web](https://example.com)")
        .unwrap();
    let archive = zip.finish().unwrap().into_inner();
    let parsed = markdown(archive).unwrap();
    assert!(parsed.contains("# Result"));
    assert!(!parsed.contains("images/x.png"));
    assert!(parsed.contains("[web](https://example.com)"));
}

#[test]
fn cancellation_does_not_overwrite_existing_output() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, b"pdf").unwrap();
    std::fs::write(&output, "previous").unwrap();
    assert!(convert(&source, &output, &default_config(), "key", &|| true).is_err());
    assert_eq!(std::fs::read_to_string(output).unwrap(), "previous");
}

fn archive(full: &str, rows: Option<Value>) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("full.md", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(full.as_bytes()).unwrap();
    if let Some(rows) = rows {
        zip.start_file(
            "doc_content_list.json",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(serde_json::to_string(&rows).unwrap().as_bytes())
            .unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn split_page_mapping_keeps_code_charts_and_drops_running_matter() {
    let rows = json!([
        {"type":"header","text":"Running header","page_idx":0},
        {"type":"footer","text":"Running footer","page_idx":0},
        {"type":"page_number","text":"- 2 -","page_idx":0},
        {"type":"text","text":"Chapter","page_idx":0},
        {"type":"code","code_caption":["Listing 1"],"code_body":"fn main() {\n    let s = \"```\";\n}","guess_lang":"rust","code_footnote":["Run it twice"],"page_idx":1},
        {"type":"chart","chart_caption":["Sales"],"chart_footnote":["Source"],"page_idx":2},
        {"type":"aside_text","text":"Margin note","page_idx":2}
    ]);
    let output = archive_content(archive("full", Some(rows)), 201, 203, true).unwrap();
    for expected in [
        "source-page:201",
        "source-page:202",
        "source-page:203",
        "Chapter",
        "Listing 1",
        "````rust\nfn main() {\n    let s = \"```\";\n}\n````",
        "Run it twice",
        "Sales",
        "Source",
        "Margin note",
    ] {
        assert!(output.contains(expected), "missing {expected} in {output}");
    }
    for dropped in ["Running header", "Running footer", "- 2 -"] {
        assert!(!output.contains(dropped), "kept {dropped} in {output}");
    }
    // A page holding only running matter keeps no anchor, so citations never point at it.
    let only_footer = archive_content(
        archive(
            "full",
            Some(json!([{"type":"footer","text":"Foot","page_idx":0},{"type":"text","text":"Body","page_idx":1}])),
        ),
        201,
        202,
        true,
    )
    .unwrap();
    assert!(!only_footer.contains("source-page:201"), "{only_footer}");
    assert!(only_footer.contains("source-page:202"), "{only_footer}");
}

#[test]
fn split_page_mapping_reads_nested_text_and_keeps_chart_and_code_details() {
    let rows = json!([
        {"type":"chart","chart_caption":[{"text":"Nested sales"}],"chart_footnote":["Source"],"page_idx":0},
        {"type":"chart","page_idx":1},
        {"type":"chart","chart_footnote":["Loose note"],"page_idx":2},
        {"type":"text","text":"Body","page_idx":3},
        {"type":"code","code_body":"print(1)","guess_lang":"objective c","page_idx":4},
        {"type":"code","code_body":"int x;","code_language":"c++ (17)","page_idx":5}
    ]);
    let output = archive_content(archive("full", Some(rows)), 201, 206, true).unwrap();
    for expected in [
        "[图片：Nested sales]",
        "Source",
        "Loose note",
        "```objective-c\nprint(1)\n```",
        "```c++-17\nint x;\n```",
    ] {
        assert!(output.contains(expected), "missing {expected} in {output}");
    }
    // A chart with no caption and no footnote keeps no anchor, so it cannot be cited.
    assert!(!output.contains("source-page:202"), "{output}");
    assert!(output.contains("source-page:203"), "{output}");
    // The caption nests its text in an object; reporting it as unreadable would be wrong.
    for page in 201..=206 {
        assert!(output.contains(&format!("source-page:{page}")) || page == 202, "missing page {page}");
    }
}

#[test]
fn empty_segment_blocks_report_their_page() {
    let error = archive_content(
        archive("full", Some(json!([{"type":"table","table_caption":["Only a caption"],"page_idx":0}]))),
        201,
        202,
        true,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("第 201 页"), "{error:#}");
    let error = archive_content(
        archive("full", Some(json!([{"type":"equation","page_idx":1}]))),
        201,
        202,
        true,
    )
    .unwrap_err();
    assert!(format!("{error:#}").contains("第 202 页"), "{error:#}");
}

#[test]
fn unknown_segment_types_report_type_and_page() {
    let error = archive_content(
        archive("full", Some(json!([{"type":"signature_block","page_idx":0}]))),
        201,
        203,
        true,
    )
    .unwrap_err();
    let message = format!("{error:#}");
    assert!(message.contains("signature_block"), "{message}");
    assert!(message.contains("201"), "{message}");
    // A long type is shortened so remote content cannot flood the diagnostic.
    let error = archive_content(
        archive("full", Some(json!([{"type": "x".repeat(80), "page_idx": 0}]))),
        201,
        203,
        true,
    )
    .unwrap_err();
    assert!(format!("{error:#}").len() < 200);
}

#[test]
fn split_page_mapping_preserves_lists_tables_formula_and_captions() {
    let rows = json!([
        {"type":"text","text":"Chapter","text_level":1,"page_idx":0},
        {"type":"list","list_items":["first","second"],"page_idx":0},
        {"type":"equation","text":"E=mc^2","page_idx":0},
        {"type":"table","table_caption":["Table caption"],"table_body":"<table>cells</table>","table_footnote":["Table footnote"],"page_idx":1},
        {"type":"image","image_caption":["Figure caption"],"image_footnote":["Figure footnote"],"page_idx":1}
    ]);
    let output = archive_content(archive("full", Some(rows)), 201, 202, true).unwrap();
    for expected in [
        "source-page:201",
        "source-page:202",
        "Chapter",
        "first",
        "second",
        "E=mc^2",
        "Table caption",
        "Table footnote",
        "Figure caption",
        "Figure footnote",
        "cells",
    ] {
        assert!(output.contains(expected), "missing {expected}");
    }
    assert!(archive_content(archive("full", Some(json!([]))), 201, 202, true).is_err());
    assert!(archive_content(
        archive(
            "full",
            Some(json!([{"type":"text","text":"bad","page_idx":4294967296u64}]))
        ),
        201,
        202,
        true
    )
    .is_err());
    assert_eq!(
        archive_content(archive("original full text", Some(json!([]))), 1, 2, false).unwrap(),
        "original full text"
    );
}

fn pdf(pages: u32) -> Vec<u8> {
    use lopdf::{dictionary, Document, Object};
    let mut doc = Document::with_version("1.5");
    let pages_id = doc.new_object_id();
    let mut kids = Vec::<Object>::new();
    for _ in 0..pages {
        let id=doc.add_object(dictionary!{"Type"=>"Page","Parent"=>pages_id,"MediaBox"=>vec![0.into(),0.into(),100.into(),100.into()]});
        kids.push(id.into());
    }
    doc.objects.insert(
        pages_id,
        dictionary! {"Type"=>"Pages","Kids"=>kids,"Count"=>pages}.into(),
    );
    let catalog = doc.add_object(dictionary! {"Type"=>"Catalog","Pages"=>pages_id});
    doc.trailer.set("Root", catalog);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes
}

#[test]
fn large_pdf_splits_at_provider_page_limit_and_keeps_original_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("large.pdf");
    std::fs::write(&source, pdf(201)).unwrap();
    let parts = prepare(&source, 50 * MIB, &|| false).unwrap();
    assert_eq!(
        parts
            .iter()
            .map(|part| (part.start, part.end))
            .collect::<Vec<_>>(),
        vec![(1, 200), (201, 201)]
    );
    assert_eq!(
        lopdf::Document::load_mem(&parts[0].bytes)
            .unwrap()
            .get_pages()
            .len(),
        200
    );
    assert_eq!(
        lopdf::Document::load_mem(&parts[1].bytes)
            .unwrap()
            .get_pages()
            .len(),
        1
    );
}

struct Step {
    method: &'static str,
    path: &'static str,
    status: u16,
    body: Vec<u8>,
}
impl Step {
    fn json(method: &'static str, path: &'static str, value: Value) -> Self {
        Self {
            method,
            path,
            status: 200,
            body: serde_json::to_vec(&value).unwrap(),
        }
    }
}
struct Fixture {
    base: String,
    requests: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<usize>>,
}
impl Fixture {
    fn new(make: impl FnOnce(&str) -> Vec<Step>) -> Self {
        use std::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let steps = make(&base);
        let requests = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = requests.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stopped = stop.clone();
        let thread = std::thread::spawn(move || {
            let mut steps = steps.into_iter();
            let mut count = 0;
            while !stopped.load(std::sync::atomic::Ordering::Relaxed) {
                let (mut socket, _) = match listener.accept() {
                    Ok(socket) => socket,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut data = Vec::new();
                let mut byte = [0];
                while !data.ends_with(b"\r\n\r\n") {
                    socket.read_exact(&mut byte).unwrap();
                    data.push(byte[0]);
                    assert!(data.len() < 65536);
                }
                let header = String::from_utf8(data).unwrap();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|length| length.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                let mut content = vec![0; length];
                socket.read_exact(&mut content).unwrap();
                let step = steps.next().expect("unexpected HTTP request");
                assert!(
                    header.starts_with(&format!("{} {} ", step.method, step.path)),
                    "{header}"
                );
                write!(socket,"HTTP/1.1 {} OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n",step.status,step.body.len()).unwrap();
                socket.write_all(&step.body).unwrap();
                seen.lock().unwrap().push(format!("{header}{}",String::from_utf8_lossy(&content)));
                count += 1;
            }
            count
        });
        Self {
            base,
            requests,
            stop,
            thread: Some(thread),
        }
    }
    fn finish(mut self, expected: usize) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        assert_eq!(self.thread.take().unwrap().join().unwrap(), expected);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[tokio::test]
async fn connection_probe_uses_only_the_parser_endpoint_without_upload_or_dashboard() {
    let server=Fixture::new(|_|vec![Step{method:"GET",path:"/302/v2/mineru/task",status:422,body:vec![]}]);
    let message=test_connection_at(&MineruConfig::default(),"secret",&server.base).await.unwrap();assert!(message.contains("尚未验证"));server.finish(1);
}
#[tokio::test]
async fn connection_probe_auth_failure_is_redacted() {
    let server=Fixture::new(|_|vec![Step{method:"GET",path:"/302/v2/mineru/task",status:401,body:b"secret".to_vec()}]);
    let error=test_connection_at(&MineruConfig::default(),"secret",&server.base).await.unwrap_err().to_string();assert!(error.contains("HTTP 401"));assert!(!error.contains("secret"));server.finish(1);
}
#[tokio::test]
async fn connection_probe_forbidden_is_not_reported_as_invalid_key() {
    let server=Fixture::new(|_|vec![Step{method:"GET",path:"/302/v2/mineru/task",status:403,body:b"sensitive".to_vec()}]);
    let error=test_connection_at(&MineruConfig::default(),"secret",&server.base).await.unwrap_err().to_string();assert!(error.contains("HTTP 403"));assert!(!error.contains("sensitive"));server.finish(1);
}
#[tokio::test]
async fn enterprise_direct_markdown_is_checkpointed_without_resubmitting() {
    let server=Fixture::new(|base|vec![Step::json("POST","/302/upload-file",json!({"code":200,"data":format!("{base}/pdf")})),Step::json("POST","/302/v2/mineru/task",json!(serde_json::to_string(&json!({"data":{"markdown":"# direct result"}})).unwrap()))]);
    let dir=tempfile::tempdir().unwrap();let path=dir.path().join("task.json");let part=Part{bytes:pdf(1),start:1,end:1,name:"test.pdf".into()};let config=MineruConfig::default();
    for _ in 0..2 {let result=parse_part(&client(5).unwrap(),&server.base,&part,&path,&config,"key",false,&|_|{}).await.unwrap();assert_eq!(result,"# direct result");}
    server.finish(2);
}
#[tokio::test]
async fn enterprise_poll_accepts_encoded_payload_and_downloaded_markdown() {
    let server=Fixture::new(|base|vec![Step::json("POST","/302/upload-file",json!({"code":200,"data":format!("{base}/pdf")})),Step::json("POST","/302/v2/mineru/task",json!({"data":{"taskId":"enterprise-task"}})),Step::json("GET","/302/v2/mineru/task?task_id=enterprise-task",json!(serde_json::to_string(&json!({"data":{"status":"finished","markdown_url":format!("{base}/result.md")}})).unwrap())),Step{method:"GET",path:"/result.md",status:200,body:b"# downloaded result".to_vec()}]);
    let dir=tempfile::tempdir().unwrap();let part=Part{bytes:pdf(1),start:1,end:1,name:"test.pdf".into()};let result=parse_part(&client(5).unwrap(),&server.base,&part,&dir.path().join("task.json"),&MineruConfig::default(),"key",false,&|_|{}).await.unwrap();assert_eq!(result,"# downloaded result");server.finish(4);
}

#[tokio::test]
async fn opensource_upload_create_poll_download_uses_version_and_method() {
    let zip=archive("# open source result",None);
    let server=Fixture::new(|base|vec![Step::json("POST","/302/upload-file",json!({"code":200,"data":format!("{base}/document.pdf")})),Step::json("POST","/302/v2/mineru/task",json!({"code":200,"data":{"task_id":"open-task"}})),Step::json("GET","/302/v2/mineru/task?task_id=open-task",json!({"code":200,"data":{"status":"completed","zip_url":format!("{base}/result.zip")}})),Step{method:"GET",path:"/result.zip",status:200,body:zip}]);
    let dir=tempfile::tempdir().unwrap();let part=Part{name:"test.pdf".into(),bytes:pdf(1),start:1,end:1};
    let config=MineruConfig{version:"2.0".into(),parse_method:"txt".into(),..Default::default()};
    let result=parse_part(&client(5).unwrap(),&server.base,&part,&dir.path().join("task.json"),&config,"key",false,&|_|{}).await.unwrap();assert!(result.contains("open source result"));
    let requests=server.requests.lock().unwrap();assert!(requests[1].contains("\"version\":\"2.0\""));assert!(requests[1].contains("\"parse_method\":\"txt\""));assert!(!requests[1].contains("model_version"));drop(requests);server.finish(4);
}

#[test]
fn official_upload_resumes_task_and_never_sends_key_to_storage() {
    let zip = archive("# parsed", None);
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"task-1","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/task-1",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: zip.clone(),
            },
            Step::json(
                "GET",
                "/extract-results/batch/task-1",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: zip.clone(),
            },
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(1)).unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    convert_at(&source, &output, &config, "secret", &|| false, &server.base).unwrap();
    convert_at(&source, &output, &config, "secret", &|| false, &server.base).unwrap();
    assert!(std::fs::read_to_string(output)
        .unwrap()
        .contains("# parsed"));
    let seen = server.requests.lock().unwrap();
    for request in seen.iter() {
        if request.starts_with("PUT /storage") || request.starts_with("GET /result") {
            assert!(!request.to_lowercase().contains("authorization:"));
        } else {
            assert!(request.contains("Bearer secret"));
        }
    }
    drop(seen);
    server.finish(6);
}

#[tokio::test]
async fn timed_out_poll_can_resume_without_a_new_submission() {
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"task-2","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/task-2",
                json!({"code":0,"data":{"extract_result":[{"state":"running"}]}}),
            ),
            Step::json(
                "GET",
                "/extract-results/batch/task-2",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: archive("resumed", None),
            },
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("task.json");
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    let part = Part {
        bytes: pdf(1),
        start: 1,
        end: 1,
        name: "scan.pdf".into(),
    };
    let http = client(5).unwrap();
    {
        let operation = parse_part(
            &http,
            &server.base,
            &part,
            &path,
            &config,
            "key",
            false,
            &|_| {},
        );
        tokio::pin!(operation);
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                tokio::select! {
                    result=&mut operation=>panic!("poll should remain running: {result:?}"),
                    _=tokio::time::sleep(Duration::from_millis(5))=>{
                        if server.requests.lock().unwrap().len()>=3 {break;}
                    }
                }
            }
        })
        .await
        .unwrap();
    }
    assert_eq!(
        read_task(&path).unwrap().unwrap().id.as_deref(),
        Some("task-2")
    );
    assert_eq!(
        parse_part(
            &http,
            &server.base,
            &part,
            &path,
            &config,
            "key",
            false,
            &|_| {}
        )
        .await
        .unwrap(),
        "resumed"
    );
    server.finish(5);
}

#[test]
fn failed_second_pdf_part_never_publishes_partial_markdown() {
    let zip = archive(
        "first",
        Some(json!([{"type":"text","text":"first","page_idx":0}])),
    );
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"first","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/first",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: zip,
            },
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"second","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/second",
                json!({"code":0,"data":{"extract_result":[{"state":"failed"}]}}),
            ),
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(201)).unwrap();
    std::fs::write(&output, "previous").unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    assert!(convert_at(&source, &output, &config, "key", &|| false, &server.base).is_err());
    assert_eq!(std::fs::read_to_string(output).unwrap(), "previous");
    server.finish(7);
}

#[test]
fn ai302_upload_and_task_protocol_roundtrip() {
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/302/upload-file",
                json!({"code":200,"data":format!("{base}/hosted.pdf")}),
            ),
            Step::json(
                "POST",
                "/302/v2/mineru/task",
                json!({"code":0,"data":{"task_id":"task302"}}),
            ),
            Step::json(
                "GET",
                "/302/v2/mineru/task?task_id=task302",
                json!({"code":0,"data":{"state":"done","full_zip_url":format!("{base}/result")}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: archive("302 result", None),
            },
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(1)).unwrap();
    convert_at(
        &source,
        &output,
        &default_config(),
        "key",
        &|| false,
        &server.base,
    )
    .unwrap();
    assert!(std::fs::read_to_string(output)
        .unwrap()
        .contains("302 result"));
    server.finish(4);
}

#[test]
fn unconfirmed_submission_is_not_blindly_submitted_twice() {
    let server = Fixture::new(|_| {
        vec![Step {
            method: "POST",
            path: "/file-urls/batch",
            status: 500,
            body: vec![],
        }]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(1)).unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    assert!(convert_at(
        &source,
        &output,
        &config,
        "private-key",
        &|| false,
        &server.base
    )
    .is_err());
    let error = convert_at(
        &source,
        &output,
        &config,
        "private-key",
        &|| false,
        &server.base,
    )
    .unwrap_err();
    assert!(error.to_string().contains("避免重复提交"));
    let cache = std::fs::read_dir(dir.path().join(".sag-mineru-tasks"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert!(!std::fs::read_to_string(cache)
        .unwrap()
        .contains("private-key"));
    server.finish(1);
}

#[test]
fn cancellation_during_polling_does_not_publish() {
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"cancelled","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/cancelled",
                json!({"code":0,"data":{"extract_result":[{"state":"running"}]}}),
            ),
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(1)).unwrap();
    std::fs::write(&output, "previous").unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    let cancelled = || server.requests.lock().unwrap().len() >= 3;
    let started = std::time::Instant::now();
    let error = convert_at(&source, &output, &config, "key", &cancelled, &server.base).unwrap_err();
    assert!(error.to_string().contains("已停止"));
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(std::fs::read_to_string(output).unwrap(), "previous");
    server.finish(3);
}

#[test]
fn concurrent_same_source_reuses_one_submission() {
    let zip = archive("shared result", None);
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"shared","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/shared",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: zip.clone(),
            },
            Step::json(
                "GET",
                "/extract-results/batch/shared",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: zip.clone(),
            },
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(1)).unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        concurrency: 2,
        ..Default::default()
    };
    std::thread::scope(|scope| {
        let first =
            scope.spawn(|| convert_at(&source, &output, &config, "key", &|| false, &server.base));
        let second =
            scope.spawn(|| convert_at(&source, &output, &config, "key", &|| false, &server.base));
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
    });
    server.finish(6);
}

#[test]
fn explicit_rate_limit_does_not_leave_an_uncertain_submission_marker() {
    let server = Fixture::new(|_| {
        vec![
            Step {
                method: "POST",
                path: "/file-urls/batch",
                status: 429,
                body: vec![],
            },
            Step {
                method: "POST",
                path: "/file-urls/batch",
                status: 429,
                body: vec![],
            },
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("scan.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(1)).unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    for _ in 0..2 {
        let error =
            convert_at(&source, &output, &config, "key", &|| false, &server.base).unwrap_err();
        assert!(error.to_string().contains("429"));
    }
    server.finish(2);
}

#[test]
fn wire_roundtrip_preserves_saved_key_status_without_serializing_the_key() {
    let config = MineruConfig {
        has_key: true,
        ..Default::default()
    };
    let encoded = serde_json::to_string(&config).unwrap();
    assert!(
        serde_json::from_str::<MineruConfig>(&encoded)
            .unwrap()
            .has_key
    );
    assert!(!encoded.contains("api_key"));
}

#[test]
fn cloud_uses_shared_configured_source_page_and_output_limits() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("large.docx");
    std::fs::write(&source, vec![0; 1024 * 1024 + 1]).unwrap();
    let mut runtime = RuntimeSettings::default();
    runtime.conversion_input_mib = 1;
    assert!(
        prepare_with_runtime(&source, 200 * MIB, &runtime, &|| false)
            .unwrap_err()
            .to_string()
            .contains("配置")
    );
    let source = dir.path().join("three.pdf");
    std::fs::write(&source, pdf(3)).unwrap();
    runtime.max_document_pages = 2;
    assert!(
        prepare_with_runtime(&source, 200 * MIB, &runtime, &|| false)
            .unwrap_err()
            .to_string()
            .contains("页数")
    );
    assert!(archive_content_with_limit(archive("too long", None), 0, 0, false, 3).is_err());
}

#[test]
fn placeholder_or_markup_only_results_fail_but_formula_and_table_text_survive() {
    for empty in [
        "![](images/page.png)",
        "###\n\n---",
        "<!-- source-page:1 -->\n<a id=\"page-1\"></a>",
        "<p>&nbsp;</p>",
    ] {
        assert!(
            archive_content(archive(empty, None), 1, 1, false).is_err(),
            "accepted empty body: {empty}"
        );
    }
    assert!(archive_content(
        archive(
            "ignored",
            Some(json!([{"type":"image","page_idx":0,"image_caption":[]}]))
        ),
        1,
        1,
        true
    )
    .is_err());
    for valid in [
        "$$ E=mc^2 $$",
        "<table><tr><td>42</td></tr></table>",
        "![示意图](images/figure.png)",
        "# 中文正文",
    ] {
        assert!(
            archive_content(archive(valid, None), 1, 1, false).is_ok(),
            "rejected meaningful body: {valid}"
        );
    }
}

#[test]
fn review_split_documents_serialize_shared_later_segment_before_submission() {
    use std::sync::{atomic::{AtomicBool,Ordering},mpsc};
    let dir=tempfile::tempdir().unwrap();let original=pdf(201);let mut sources=vec![];
    for (name,cover) in [("a.pdf","cover-a"),("b.pdf","cover-b")] {
        let mut document=lopdf::Document::load_mem(&original).unwrap();let first=document.get_pages()[&1];
        document.get_object_mut(first).unwrap().as_dict_mut().unwrap().set("ReviewCover",lopdf::Object::string_literal(cover));
        let mut bytes=vec![];document.save_to(&mut bytes).unwrap();let source=dir.path().join(name);std::fs::write(&source,bytes).unwrap();sources.push(source);
    }
    let a=prepare(&sources[0],50*MIB,&||false).unwrap();let b=prepare(&sources[1],50*MIB,&||false).unwrap();
    assert_ne!(a[0].bytes,b[0].bytes);assert_eq!(a[1].bytes,b[1].bytes,"fixtures must share only a later segment");
    let config=MineruConfig{provider:MineruProvider::Official,concurrency:16,..Default::default()};
    let output_a=dir.path().join("a.md");let output_b=dir.path().join("b.md");
    let stop_a=AtomicBool::new(false);let stop_b=AtomicBool::new(false);
    let (holding_tx,holding_rx)=mpsc::channel();let (ready_tx,ready_rx)=mpsc::channel();let (upload_tx,upload_rx)=mpsc::channel();
    std::thread::scope(|scope| {
        let first=scope.spawn(||convert_at_with_progress(&sources[0],&output_a,&config,"test-key",&||stop_a.load(Ordering::SeqCst),"http://127.0.0.1:1",&RuntimeSettings::default(),&|phase|{
            if phase=="uploading" {holding_tx.send(()).unwrap();while !stop_a.load(Ordering::SeqCst){std::thread::sleep(Duration::from_millis(1));}}
        }));
        holding_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let second=scope.spawn(||convert_at_with_progress(&sources[1],&output_b,&config,"test-key",&||stop_b.load(Ordering::SeqCst),"http://127.0.0.1:1",&RuntimeSettings::default(),&|phase|{
            if phase=="preparing"{ready_tx.send(()).unwrap();}if phase=="uploading"{let _=upload_tx.send(());}
        }));
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();let overlapped=upload_rx.recv_timeout(Duration::from_millis(500)).is_ok();
        stop_a.store(true,Ordering::SeqCst);stop_b.store(true,Ordering::SeqCst);
        assert!(first.join().unwrap().is_err());assert!(second.join().unwrap().is_err());
        assert!(!overlapped,"a shared later journal must be locked before either document can submit");
    });
}

#[test]
fn enterprise_response_variants_and_error_codes_are_classified() {
    assert_eq!(find_302_task_id(&json!({"data":"task-123"})).as_deref(),Some("task-123"));
    assert_eq!(find_302_task_id(&json!({"status":"success","data":{"id":123}})).as_deref(),Some("123"));
    assert_eq!(find_302_task_id(&json!("A0005")),None);
    assert_eq!(normalize_opensource(json!("A0005")).unwrap()["state"],"failed");
    assert_eq!(normalize_opensource(json!({"data":{"markdown":"# result"}})).unwrap()["markdown"],"# result");
}

#[tokio::test]
async fn resumed_expired_task_stops_pending_polling_but_can_recover_completed_result() {
    for state in ["pending", "running", "converting", "waiting-file", "done"] {
        let server = Fixture::new(|base| {
            let mut steps = vec![Step::json(
                "GET",
                "/extract-results/batch/old-task",
                json!({
                    "code":0,"data":{"extract_result":[{"state":state,"full_zip_url":format!("{base}/result")} ]}
                }),
            )];
            if state == "done" {
                steps.push(Step {
                    method: "GET",
                    path: "/result",
                    status: 200,
                    body: archive("# recovered", None),
                });
            }
            steps
        });
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("task.json");
        write_task(
            &path,
            &Task {
                id: Some("old-task".into()),
                upload: None,
                uploaded: true,
                submitted_at: now() - 60,
                result: None,
            },
        )
        .unwrap();
        let config = MineruConfig {
            provider: MineruProvider::Official,
            task_timeout_secs: 10,
            ..Default::default()
        };
        let part = Part {
            bytes: pdf(1),
            start: 1,
            end: 1,
            name: "scan.pdf".into(),
        };
        let result = tokio::time::timeout(
            Duration::from_millis(500),
            parse_part(
                &client(5).unwrap(),
                &server.base,
                &part,
                &path,
                &config,
                "key",
                false,
                &|_| {},
            ),
        )
        .await;
        assert!(
            result.is_ok(),
            "{state}: resumed task restarted its waiting budget"
        );
        let result = result.unwrap();
        if state == "done" {
            assert!(result.unwrap().contains("recovered"));
        } else {
            assert!(result.unwrap_err().to_string().contains("超时"));
        }
        assert!(
            read_task(&path).unwrap().is_some(),
            "keep journal to avoid duplicate submission"
        );
        server.finish(if state == "done" { 2 } else { 1 });
    }
}

#[test]
fn shared_pdf_splitter_uses_fifty_pages_and_preserves_original_ranges() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("long.pdf");
    std::fs::write(&source, pdf(201)).unwrap();
    let parts =
        prepare_pdf_segments(&source, 200 * MIB, &RuntimeSettings::default(), &|| false).unwrap();
    assert_eq!(
        parts.iter().map(|p| (p.start, p.end)).collect::<Vec<_>>(),
        [(1, 50), (51, 100), (101, 150), (151, 200), (201, 201)]
    );
    for part in parts {
        assert_eq!(
            lopdf::Document::load_mem(&part.bytes)
                .unwrap()
                .get_pages()
                .len(),
            (part.end - part.start + 1) as usize
        );
    }
}

#[test]
fn segment_cloud_results_keep_original_page_offsets() {
    let server = Fixture::new(|base| {
        vec![
            Step::json(
                "POST",
                "/file-urls/batch",
                json!({"code":0,"data":{"batch_id":"segment","file_urls":[format!("{base}/storage")]}}),
            ),
            Step {
                method: "PUT",
                path: "/storage",
                status: 200,
                body: vec![],
            },
            Step::json(
                "GET",
                "/extract-results/batch/segment",
                json!({"code":0,"data":{"extract_result":[{"state":"done","full_zip_url":format!("{base}/result")} ]}}),
            ),
            Step {
                method: "GET",
                path: "/result",
                status: 200,
                body: archive(
                    "full",
                    Some(json!([
                        {"type":"text","text":"first segment page","page_idx":0},
                        {"type":"text","text":"second segment page","page_idx":1}
                    ])),
                ),
            },
        ]
    });
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("segment.pdf");
    let output = dir.path().join("out.md");
    std::fs::write(&source, pdf(2)).unwrap();
    let config = MineruConfig {
        provider: MineruProvider::Official,
        ..Default::default()
    };
    convert_range_at_with_progress(
        &source,
        &output,
        &config,
        "key",
        &|| false,
        &server.base,
        &RuntimeSettings::default(),
        &|_| {},
        Some(51),
    )
    .unwrap();
    let text = std::fs::read_to_string(output).unwrap();
    assert!(text.contains("source-page:51"));
    assert!(text.contains("source-page:52"));
    assert!(text.contains("id=\"page-51\""));
    assert!(!text.contains("source-page:1"));
    server.finish(4);
}

#[test]
fn shared_pdf_page_limit_remains_a_resource_failure() {
    let dir=tempfile::tempdir().unwrap();let source=dir.path().join("long.pdf");
    std::fs::write(&source,pdf(51)).unwrap();
    let runtime=RuntimeSettings{max_document_pages:50,..Default::default()};
    let error=prepare_pdf_segments(&source,200*MIB,&runtime,&||false).unwrap_err();
    assert!(crate::conversion::resource_failure(&error));
}
