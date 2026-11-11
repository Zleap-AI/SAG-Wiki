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
fn archive_markdown_preserves_relative_images() {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    zip.start_file("full.md", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"# Result\n![figure](images/x.png)\n[web](https://example.com)")
        .unwrap();
    let archive = zip.finish().unwrap().into_inner();
    let parsed = markdown(archive).unwrap();
    assert!(parsed.contains("# Result"));
    assert!(parsed.contains("images/x.png"));
    assert!(parsed.contains("[web](https://example.com)"));
}

#[test]
fn archive_assets_survive_markdown_rename_and_isolate_segments() {
    let dir = tempfile::tempdir().unwrap();
    let make = |bytes: &[u8]| {
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        for (name, data) in [("full.md", b"# Result\n![caption](images/x.png)\n![title](./images/x.png \"title\")\n![](<images/x.png>)\n<img src='images/x.png'>".as_slice()), ("images/x.png", bytes)] {
            zip.start_file(name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.start_file("doc_content_list.json", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(br#"[{"type":"image","img_path":"images/x.png","image_caption":["Caption"],"page_idx":0}]"#).unwrap();
        zip.finish().unwrap().into_inner()
    };
    let first = archive_content_with_assets(make(b"first"), 0, 0, false, MIB, Some(dir.path())).unwrap();
    let second = archive_content_with_assets(make(b"second"), 0, 0, false, MIB, Some(dir.path())).unwrap();
    assert_ne!(first, second);
    for (text, expected) in [(&first, b"first".as_slice()), (&second, b"second".as_slice())] {
        let link = text.split("](").nth(1).unwrap().split(')').next().unwrap();
        assert_eq!(std::fs::read(dir.path().join(link)).unwrap(), expected);
        assert!(dir.path().join(link).parent().unwrap().parent().unwrap().join("full.md").exists());
        assert!(!text.contains("](./images/") && !text.contains("](images/"));
        assert_eq!(text.matches(".sag-mineru-assets/").count(), 4);
    }
    let staged = dir.path().join("pending.md");
    std::fs::write(&staged, &first).unwrap();
    std::fs::rename(staged, dir.path().join("report.pdf.md")).unwrap();
    assert_eq!(archive_content_with_assets(make(b"first"), 0, 0, false, MIB, Some(dir.path())).unwrap(), first);
    let split = archive_content_with_assets(make(b"first"), 201, 202, true, MIB, Some(dir.path())).unwrap();
    assert!(split.contains("source-page:201"));
    assert!(split.contains("Caption"));
    assert!(split.contains("![](.sag-mineru-assets/"));
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
