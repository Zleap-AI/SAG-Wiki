//! Durable per-part parsing, bounded cloud workers, and ordered publication.
use super::{Attempt, EngineFailures, Engines};
use crate::{
    docling::Docling,
    mineru::{self, Part},
};
use anyhow::{ensure, Context, Result};
use sag_protocol::{types::Settings, ActiveSegment, ConversionSegment, SegmentProgress};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
};

fn read_cached(path: &Path, limit: u64) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && !meta.file_type().is_symlink(),
                "PDF 分段缓存不是普通文件"
            );
            if meta.len() > limit {
                return Ok(None);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    }
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    Ok(String::from_utf8(bytes)
        .ok()
        .filter(|text| !text.trim().is_empty() && text.len() as u64 <= limit))
}

struct Document<'a> {
    parts: &'a [Part],
    paths: Vec<PathBuf>,
    work: tempfile::TempDir,
    limit: u64,
}

struct Job {
    input: PathBuf,
    parsed: tempfile::TempPath,
    // Held across the cloud to local handoff until the cache file is published.
    _lease: mineru::TaskLock,
}

impl<'a> Document<'a> {
    fn new(parts: &'a [Part], output: &Path, identity: &str, limit: u64) -> Result<Self> {
        let parent = output.parent().context("转换输出缺少父目录")?;
        let cache = parent.join(".sag-pdf-parts");
        fs::create_dir_all(&cache)?;
        ensure!(
            fs::symlink_metadata(&cache)?.is_dir(),
            "PDF 分段缓存目录无效"
        );
        let cache = cache.canonicalize()?;
        let work = tempfile::Builder::new()
            .prefix(".sag-pdf-work-")
            .tempdir_in(parent)?;
        let mut paths = Vec::with_capacity(parts.len());
        let mut previous = 0u32;
        for part in parts {
            ensure!(
                previous.checked_add(1) == Some(part.start) && part.end >= part.start,
                "PDF 分段范围无效"
            );
            previous = part.end;
            let mut hash = Sha256::new();
            hash.update(identity);
            hash.update(part.start.to_le_bytes());
            hash.update(part.end.to_le_bytes());
            hash.update(&part.bytes);
            paths.push(cache.join(format!("{:x}.md", hash.finalize())));
        }
        Ok(Self {
            parts,
            paths,
            work,
            limit,
        })
    }

    fn segment(&self, i: usize) -> ConversionSegment {
        let part = &self.parts[i];
        ConversionSegment {
            current: i + 1,
            total: self.parts.len(),
            start_page: part.start,
            end_page: part.end,
        }
    }

    fn begin<W>(
        &self,
        i: usize,
        cancel: &impl Fn() -> bool,
        on_wait: impl FnOnce() -> W,
    ) -> Result<PartInput> {
        let path = &self.paths[i];
        let lease = mineru::task_lock_with_wait(path, cancel, on_wait)?;
        if let Some(text) = read_cached(path, self.limit)? {
            return Ok(PartInput::Cached(merged_size(&self.parts[i], &text)));
        }
        let part = &self.parts[i];
        let input = self.work.path().join(&part.name);
        fs::write(&input, &part.bytes)?;
        let parsed = tempfile::NamedTempFile::new_in(path.parent().unwrap())?.into_temp_path();
        Ok(PartInput::Parse(Job {
            input,
            parsed,
            _lease: lease,
        }))
    }

    fn publish(&self, i: usize, job: Job, cancel: &impl Fn() -> bool) -> Result<u64> {
        let text = read_cached(&job.parsed, self.limit)?
            .context("PDF 分段未生成有效正文或超过大小限制")?;
        // A local parser may prepend a link to the temporary split input.
        let mut body_text = text.as_str();
        if text.starts_with(&format!("[{}](<", self.parts[i].name)) {
            if let Some((_, body)) = text
                .split_once("\n\n")
                .filter(|(header, _)| header.contains(".sag-pdf-work-"))
            {
                ensure!(!body.trim().is_empty(), "PDF 分段未生成有效正文");
                fs::write(&job.parsed, body)?;
                body_text = body;
            }
        }
        ensure!(!cancel(), "已停止转换，未发布输出");
        job.parsed
            .persist(&self.paths[i])
            .map_err(|error| error.error)?;
        Ok(merged_size(&self.parts[i], body_text))
    }

    fn merge(&self, output: &Path, cancel: &impl Fn() -> bool) -> Result<()> {
        let mut merged = tempfile::NamedTempFile::new_in(output.parent().unwrap())?;
        let mut size = 0u64;
        let limit = self.limit;
        for (part, path) in self.parts.iter().zip(&self.paths) {
            ensure!(!cancel(), "已停止转换，未发布输出");
            let text = read_cached(path, limit)?.context("PDF 分段缓存无效")?;
            let (marker, text) = merged_part(part, &text);
            size = size
                .checked_add(marker.len() as u64)
                .and_then(|n| n.checked_add(text.len() as u64))
                .context("PDF 合并正文超过大小限制")?;
            if size > limit {
                return Err(crate::process_supervisor::Failure::new(
                    crate::process_supervisor::FailureKind::Memory,
                    "PDF 合并正文超过配置的大小限制",
                )
                .into());
            }
            merged.write_all(marker.as_bytes())?;
            merged.write_all(text.as_bytes())?;
        }
        ensure!(!cancel(), "已停止转换，未发布输出");
        merged.persist(output).map_err(|e| e.error)?;
        Ok(())
    }
}

enum PartInput {
    Cached(u64),
    Parse(Job),
}

fn merged_part<'a>(part: &Part, text: &'a str) -> (String, std::borrow::Cow<'a, str>) {
    // MinerU stores assets beside each cached part; merged Markdown lives one directory up.
    static ASSET_DESTINATIONS: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r#"(?P<prefix>\]\(\s*<?|(?i:\bsrc\s*=\s*["'])|(?m:^\s*\[[^\]\n]+\]:\s*<?))\.sag-mineru-assets/"#).unwrap()
    });
    let text = ASSET_DESTINATIONS.replace_all(text, "${prefix}.sag-pdf-parts/.sag-mineru-assets/");
    let marker = format!("\n\n<!-- source-pages:{}-{} -->\n\n", part.start, part.end);
    (marker, text)
}

fn merged_size(part: &Part, text: &str) -> u64 {
    let (marker, body) = merged_part(part, text);
    marker.len() as u64 + body.len() as u64
}

enum State {
    Pending,
    Active(String, String),
    WaitingLocal,
    Ready,
    Failed,
    Cancelled,
}

enum Event {
    Available(usize),
    OutputLimit(usize),
    Attempt(usize, Attempt),
    State(usize, State),
    Error(usize, anyhow::Error),
}

fn emit_snapshot(
    doc: &Document,
    states: &[State],
    mut attempt: Attempt,
    report: &mut impl FnMut(Attempt),
) {
    let mut progress = SegmentProgress {
        total: states.len(),
        ..Default::default()
    };
    let mut representative = None;
    for (i, state) in states.iter().enumerate() {
        if representative.is_none() && !matches!(state, State::Ready) {
            representative = Some(doc.segment(i));
        }
        match state {
            State::Ready => progress.completed += 1,
            State::Failed => progress.failed += 1,
            State::Cancelled => progress.cancelled += 1,
            State::WaitingLocal => progress.waiting_local += 1,
            State::Active(engine, phase) => progress.active.push(ActiveSegment {
                segment_id: i + 1,
                start_page: doc.parts[i].start,
                end_page: doc.parts[i].end,
                engine: engine.clone(),
                phase: phase.clone(),
            }),
            State::Pending => {}
        }
    }
    attempt.segment_status = Some((progress, representative));
    report(attempt);
}

fn progress_only() -> Attempt {
    Attempt {
        segment_status: None,
        segment: None,
        phase: Some("segment".into()),
        engine: String::new(),
        elapsed_ms: 0,
        error: None,
    }
}

fn configured_cloud_width(settings: &Settings, parts: usize) -> usize {
    if settings.mineru.enabled {
        settings.mineru.concurrency.min(parts)
    } else {
        0
    }
}

pub(super) fn convert_document(
    parts: &[Part],
    output: &Path,
    identity: &str,
    docling: &Docling,
    settings: &Settings,
    cancel: &(impl Fn() -> bool + Sync),
    report: &mut impl FnMut(Attempt),
    enhanced: bool,
) -> Result<()> {
    let doc = Document::new(
        parts,
        output,
        identity,
        settings.runtime.conversion_output_mib as u64 * 1024 * 1024,
    )?;
    let width = configured_cloud_width(settings, parts.len());
    let paused = AtomicBool::new(false);
    schedule_with_session(
        &doc,
        width,
        cancel,
        report,
        |i, job, stop, events| {
            super::convert_using(
                &job.input,
                &job.parsed,
                docling,
                settings,
                || stop(),
                events,
                Some(parts[i].start),
                enhanced,
                None,
                Engines::Cloud(&paused),
            )
        },
        || std::cell::OnceCell::new(),
        |session, i, job, stop, events| {
            super::convert_using(
                &job.input,
                &job.parsed,
                docling,
                settings,
                || stop(),
                events,
                Some(parts[i].start),
                enhanced,
                Some(session),
                Engines::Local,
            )
        },
        |session| {
            if let Some(session) = session.get() {
                session.release_idle_model();
            }
        },
    )?;
    doc.merge(output, cancel)
}

type Stop<'a> = dyn Fn() -> bool + Sync + 'a;

fn combine_failures(cloud: anyhow::Error, local: anyhow::Error) -> anyhow::Error {
    let (Some(first), Some(last)) = (
        cloud.downcast_ref::<EngineFailures>(),
        local.downcast_ref::<EngineFailures>(),
    ) else {
        return local.context(format!("{cloud:#}"));
    };
    EngineFailures {
        messages: [first.messages.as_slice(), last.messages.as_slice()].concat(),
        resource: first.resource || last.resource,
        component_missing: first.component_missing && last.component_missing,
    }
    .into()
}

#[cfg(test)]
fn schedule<L>(
    doc: &Document,
    width: usize,
    cancel: &(impl Fn() -> bool + Sync),
    report: &mut impl FnMut(Attempt),
    cloud: impl Fn(usize, &Job, &Stop<'_>, &mut dyn FnMut(Attempt)) -> Result<()> + Sync,
    local_parser: impl FnOnce() -> L + Send,
) -> Result<()>
where
    L: FnMut(usize, &Job, &Stop<'_>, &mut dyn FnMut(Attempt)) -> Result<()>,
{
    schedule_with_session(
        doc,
        width,
        cancel,
        report,
        cloud,
        local_parser,
        |parse, i, job, stop, events| parse(i, job, stop, events),
        |_| {},
    )
}

enum LocalMessage {
    Part(usize, Option<(Job, anyhow::Error)>),
    CheckIdle,
}

struct WaitingForPart<'a>(&'a AtomicUsize);
impl Drop for WaitingForPart<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn schedule_with_session<S>(
    doc: &Document,
    width: usize,
    cancel: &(impl Fn() -> bool + Sync),
    report: &mut impl FnMut(Attempt),
    cloud: impl Fn(usize, &Job, &Stop<'_>, &mut dyn FnMut(Attempt)) -> Result<()> + Sync,
    create_session: impl FnOnce() -> S + Send,
    mut parse_local: impl FnMut(&mut S, usize, &Job, &Stop<'_>, &mut dyn FnMut(Attempt)) -> Result<()>
        + Send,
    mut release_idle: impl FnMut(&mut S) + Send,
) -> Result<()> {
    let abort = AtomicBool::new(false);
    let stop = || abort.load(Ordering::Acquire) || cancel();
    let mut states: Vec<_> = doc.parts.iter().map(|_| State::Pending).collect();
    let mut failure = None;
    let shared_waiters = AtomicUsize::new(0);
    let output_bytes = AtomicU64::new(0);
    let ready = |i, size, tx: &mpsc::Sender<Event>| {
        // Count exactly the bytes merge will write. Signal the terminal limit
        // before this worker asks for its next (potentially billable) part.
        if output_bytes
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |total| {
                total.checked_add(size).filter(|sum| *sum <= doc.limit)
            })
            .is_err()
            && !abort.swap(true, Ordering::AcqRel)
        {
            let _ = tx.send(Event::OutputLimit(i));
        } else {
            let _ = tx.send(Event::State(i, State::Ready));
        }
    };
    let fail = |i, error, tx: &mpsc::Sender<Event>| {
        if !cancel() && !abort.swap(true, Ordering::AcqRel) {
            let _ = tx.send(Event::Error(i, error));
        } else {
            let _ = tx.send(Event::State(i, State::Cancelled));
        }
    };
    std::thread::scope(|scope| {
        let (tx, events) = mpsc::channel();
        let (local_tx, local_queue) = mpsc::channel::<LocalMessage>();
        let local_events = tx.clone();
        let stop = &stop;
        let fail = &fail;
        let ready = &ready;
        let waiters = &shared_waiters;
        scope.spawn(move || {
            let mut session = create_session();
            loop {
                let message = match local_queue.try_recv() {
                    Ok(message) => message,
                    Err(_) => {
                        // A peer holding a shared part lease may be waiting for our
                        // model budget. Yield only between jobs and on the owning thread.
                        if waiters.load(Ordering::Acquire) != 0 {
                            release_idle(&mut session);
                        }
                        let Ok(message) = local_queue.recv() else {
                            break;
                        };
                        message
                    }
                };
                let LocalMessage::Part(i, fallback) = message else {
                    continue;
                };
                if stop() {
                    break;
                }
                let result = (|| {
                    let (job, cloud_error) = if let Some((job, error)) = fallback {
                        (job, Some(error))
                    } else {
                        match doc.begin(i, stop, || release_idle(&mut session))? {
                            PartInput::Parse(job) => (job, None),
                            PartInput::Cached(bytes) => return Ok(bytes),
                        }
                    };
                    let result = parse_local(&mut session, i, &job, stop, &mut |a| {
                        let _ = local_events.send(Event::Attempt(i, a));
                    });
                    if let Err(error) = result {
                        return Err(match cloud_error {
                            Some(cloud) => combine_failures(cloud, error),
                            None => error,
                        });
                    }
                    doc.publish(i, job, stop)
                })();
                match result {
                    Ok(bytes) => ready(i, bytes, &local_events),
                    Err(e) => {
                        fail(i, e, &local_events);
                        break;
                    }
                }
            }
        });
        let mut workers = Vec::new();
        for lane in 0..width {
            let (work_tx, work) = mpsc::channel::<usize>();
            workers.push(work_tx);
            let (events, local, cloud) = (tx.clone(), local_tx.clone(), &cloud);
            scope.spawn(move || {
                for i in work {
                    if stop() {
                        break;
                    }
                    let result = (|| {
                        let input = doc.begin(i, stop, || {
                            waiters.fetch_add(1, Ordering::AcqRel);
                            let _ = local.send(LocalMessage::CheckIdle);
                            WaitingForPart(waiters)
                        })?;
                        let bytes = match input {
                            PartInput::Cached(bytes) => bytes,
                            PartInput::Parse(job) => match cloud(i, &job, stop, &mut |a| {
                                let _ = events.send(Event::Attempt(i, a));
                            }) {
                                Ok(()) => doc.publish(i, job, stop)?,
                                Err(error) => {
                                    ensure!(!stop(), "已停止转换，未发布输出");
                                    let _ = events.send(Event::State(i, State::WaitingLocal));
                                    local
                                        .send(LocalMessage::Part(i, Some((job, error))))
                                        .map_err(|_| anyhow::anyhow!("本地解析已停止"))?;
                                    return Ok(());
                                }
                            },
                        };
                        ready(i, bytes, &events);
                        Ok(())
                    })();
                    if let Err(e) = result {
                        fail(i, e, &events);
                        break;
                    }
                    if events.send(Event::Available(lane)).is_err() {
                        break;
                    }
                }
            });
        }
        let mut next = 0..doc.parts.len();
        for worker in &workers {
            if let Some(i) = next.next() {
                let _ = worker.send(i);
            }
        }
        if width == 0 {
            for i in next.by_ref() {
                if stop() || local_tx.send(LocalMessage::Part(i, None)).is_err() {
                    break;
                }
            }
        }
        drop(local_tx);
        drop(tx);
        let mut workers: Vec<_> = workers.into_iter().map(Some).collect();
        for event in events {
            let (i, mut attempt) = match event {
                Event::Available(lane) => {
                    if let (false, Some(i)) = (stop(), next.next()) {
                        let _ = workers[lane].as_ref().unwrap().send(i);
                    } else {
                        workers[lane].take();
                    }
                    continue;
                }
                Event::Attempt(i, attempt) => {
                    if let Some(phase) = &attempt.phase {
                        let phase = if attempt.engine != "MinerU" && phase == "starting" {
                            "running_local"
                        } else {
                            phase
                        };
                        states[i] = State::Active(attempt.engine.clone(), phase.into());
                    } else {
                        states[i] = State::Pending;
                    }
                    (i, attempt)
                }
                Event::State(i, state) => {
                    states[i] = state;
                    (i, progress_only())
                }
                Event::OutputLimit(i) => {
                    states[i] = State::Ready;
                    failure = Some(
                        crate::process_supervisor::Failure::new(
                            crate::process_supervisor::FailureKind::Memory,
                            "PDF 合并正文超过配置的大小限制",
                        )
                        .into(),
                    );
                    (i, progress_only())
                }
                Event::Error(i, error) => {
                    states[i] = State::Failed;
                    let part = &doc.parts[i];
                    failure = Some(
                        error.context(format!("PDF 第 {}–{} 页转换失败", part.start, part.end)),
                    );
                    (i, progress_only())
                }
            };
            attempt.segment = Some(doc.segment(i));
            emit_snapshot(doc, &states, attempt, report);
            if stop() {
                for worker in &mut workers {
                    worker.take();
                }
            }
        }
    });
    if stop() {
        for state in &mut states {
            if !matches!(state, State::Ready | State::Failed) {
                *state = State::Cancelled;
            }
        }
        emit_snapshot(doc, &states, progress_only(), report);
    }
    if let Some(error) = failure {
        return Err(error);
    }
    ensure!(!cancel(), "已停止转换，未发布输出");
    ensure!(
        states.iter().all(|s| matches!(s, State::Ready)),
        "PDF 分段转换未全部完成"
    );
    Ok(())
}

#[cfg(test)]
fn convert_parts(
    parts: &[Part],
    output: &Path,
    identity: &str,
    limit: u64,
    cancel: &(impl Fn() -> bool + Sync),
    mut progress: impl FnMut(&Part, usize, usize),
    mut convert: impl FnMut(&Part, &Path, &Path) -> Result<()> + Send,
) -> Result<()> {
    let doc = Document::new(parts, output, identity, limit)?;
    schedule(
        &doc,
        0,
        cancel,
        &mut |attempt| {
            if let Some(segment) = attempt.segment {
                progress(&parts[segment.current - 1], segment.current, parts.len());
            }
        },
        |_, _, _, _| unreachable!(),
        || {
            move |i: usize, job: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| {
                convert(&parts[i], &job.input, &job.parsed)
            }
        },
    )?;
    doc.merge(output, cancel)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn parts() -> Vec<crate::mineru::Part> {
        vec![(1, 50), (51, 100), (101, 101)]
            .into_iter()
            .map(|(start, end)| crate::mineru::Part {
                bytes: vec![start as u8],
                start,
                end,
                name: format!("pages-{start}-{end}.pdf"),
            })
            .collect()
    }
    #[test]
    fn configured_cloud_width_follows_setting_and_available_parts() {
        let mut settings = Settings::default();
        assert_eq!(settings.mineru.concurrency, 2);
        assert_eq!(configured_cloud_width(&settings, 8), 0);
        settings.mineru.enabled = true;
        for width in [1, 2, 4, 16] {
            settings.mineru.concurrency = width;
            assert_eq!(configured_cloud_width(&settings, 20), width);
            assert_eq!(configured_cloud_width(&settings, 3), width.min(3));
            assert_eq!(configured_cloud_width(&settings, 0), 0);
        }
    }

    #[test]
    fn configured_width_four_runs_four_parts_together() {
        use std::sync::{Condvar, Mutex};
        use std::time::Duration;
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out.md");
        let parts: Vec<_> = (1..=4).map(|page| Part {
            bytes: vec![page as u8], start: page, end: page,
            name: format!("page-{page}.pdf"),
        }).collect();
        let doc = Document::new(&parts, &output, "four-parts", 4096).unwrap();
        let mut settings = Settings::default();
        settings.mineru.enabled = true;
        settings.mineru.concurrency = 4;
        let arrived = (Mutex::new(0usize), Condvar::new());
        schedule(&doc, configured_cloud_width(&settings, parts.len()), &|| false, &mut |_| {},
            |i, job, _, _| {
                let mut count = arrived.0.lock().unwrap();
                *count += 1;
                arrived.1.notify_all();
                let (count, timeout) = arrived.1.wait_timeout_while(count, Duration::from_secs(3), |n| *n < 4).unwrap();
                anyhow::ensure!(!timeout.timed_out() && *count == 4, "four cloud parts must overlap");
                drop(count);
                fs::write(&job.parsed, format!("part {}", i + 1))?;
                Ok(())
            }, || { |_, _: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| anyhow::bail!("unexpected local fallback") }
        ).unwrap();
        doc.merge(&output, &|| false).unwrap();
        let body = fs::read_to_string(&output).unwrap();
        let positions: Vec<_> = (1..=4).map(|i| body.find(&format!("part {i}")).unwrap()).collect();
        assert!(positions.windows(2).all(|p| p[0] < p[1]));
    }

    #[test]
    fn later_cloud_parts_can_finish_and_cache_while_the_first_is_active() {
        use std::sync::{mpsc::channel, Mutex};
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out.md");
        let parts = parts();
        let doc = Document::new(&parts, &output, "fixture", 4096).unwrap();
        let (started, first_started) = channel();
        let first_started = Mutex::new(first_started);
        let (release, wait) = channel();
        let wait = Mutex::new(wait);
        let mut ready = vec![];
        schedule(
            &doc,
            2,
            &|| false,
            &mut |attempt| {
                let (progress, earliest) = attempt.segment_status.as_ref().unwrap();
                if progress.completed > ready.len() {
                    ready.push(attempt.segment.as_ref().unwrap().current);
                    if progress.completed == 2 {
                        assert_eq!(earliest.as_ref().unwrap().current, 1);
                        assert_eq!(progress.active.len(), 1);
                        assert!(doc.paths[1].is_file() && doc.paths[2].is_file());
                        release.send(()).unwrap();
                    }
                }
            },
            |i, job, _, report| {
                report(Attempt {
                    engine: "MinerU".into(),
                    phase: Some("parsing".into()),
                    ..progress_only()
                });
                if i == 0 {
                    started.send(())?;
                    wait.lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))?;
                } else if i == 1 {
                    first_started
                        .lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))?;
                }
                fs::write(
                    &job.parsed,
                    format!(
                        "part {} ![fig](.sag-mineru-assets/example/image.png)",
                        i + 1
                    ),
                )?;
                Ok(())
            },
            || {
                |_, _: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| {
                    panic!("unexpected local fallback")
                }
            },
        )
        .unwrap();
        assert_eq!(ready, [2, 3, 1]);
        doc.merge(&output, &|| false).unwrap();
        let parallel = fs::read(output).unwrap();
        let serial_root = tempfile::tempdir().unwrap();
        let serial = serial_root.path().join("out.md");
        convert_parts(
            &parts,
            &serial,
            "fixture",
            4096,
            &|| false,
            |_, _, _| {},
            |part, _, path| {
                fs::write(
                    path,
                    format!(
                        "part {} ![fig](.sag-mineru-assets/example/image.png)",
                        (part.start - 1) / 50 + 1
                    ),
                )?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(parallel, fs::read(serial).unwrap());
    }

    #[test]
    fn one_local_fallback_can_run_beside_two_cloud_parts() {
        use std::sync::{Condvar, Mutex};
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out.md");
        let parts = parts();
        let doc = Document::new(&parts, &output, "fixture", 4096).unwrap();
        let gate = (Mutex::new(false), Condvar::new());
        let wait = || -> Result<()> {
            let (guard, _) = gate
                .1
                .wait_timeout_while(
                    gate.0.lock().unwrap(),
                    std::time::Duration::from_secs(5),
                    |released| !*released,
                )
                .unwrap();
            ensure!(*guard, "three segments never overlapped");
            Ok(())
        };
        let mut observed = false;
        schedule(
            &doc,
            2,
            &|| false,
            &mut |attempt| {
                let active = &attempt.segment_status.as_ref().unwrap().0.active;
                if active.len() == 3 {
                    assert_eq!(
                        active
                            .iter()
                            .map(|s| (s.segment_id, s.engine.as_str(), s.phase.as_str()))
                            .collect::<Vec<_>>(),
                        [
                            (1, "Docling", "running_local"),
                            (2, "MinerU", "parsing"),
                            (3, "MinerU", "parsing")
                        ]
                    );
                    observed = true;
                    *gate.0.lock().unwrap() = true;
                    gate.1.notify_all();
                }
            },
            |i, job, _, report| {
                if i == 0 {
                    anyhow::bail!("cloud failure");
                }
                report(Attempt {
                    engine: "MinerU".into(),
                    phase: Some("parsing".into()),
                    ..progress_only()
                });
                wait()?;
                fs::write(&job.parsed, "cloud result")?;
                Ok(())
            },
            || {
                |_, job: &Job, _: &Stop<'_>, report: &mut dyn FnMut(Attempt)| {
                    report(Attempt {
                        engine: "Docling".into(),
                        phase: Some("starting".into()),
                        ..progress_only()
                    });
                    wait()?;
                    fs::write(&job.parsed, "local result")?;
                    Ok(())
                }
            },
        )
        .unwrap();
        assert!(observed);
    }

    #[test]
    fn terminal_local_failure_cancels_cloud_siblings_and_keeps_the_original_error() {
        use std::sync::{mpsc::channel, Mutex};
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out.md");
        fs::write(&output, "previous complete text").unwrap();
        let parts = parts();
        let doc = Document::new(&parts, &output, "fixture", 4096).unwrap();
        let (entered, wait) = channel();
        let wait = Mutex::new(wait);
        let mut last = None;
        let result = schedule(
            &doc,
            2,
            &|| false,
            &mut |attempt| {
                last = attempt.segment_status;
            },
            |i, _, stop, _| {
                if i == 0 {
                    anyhow::bail!("cloud failure");
                }
                entered.send(())?;
                let start = std::time::Instant::now();
                while !stop() && start.elapsed() < std::time::Duration::from_secs(5) {
                    std::thread::yield_now();
                }
                ensure!(stop(), "sibling ignored terminal failure");
                anyhow::bail!("sibling cancelled")
            },
            || {
                |_, _: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| {
                    wait.lock()
                        .unwrap()
                        .recv_timeout(std::time::Duration::from_secs(5))?;
                    anyhow::bail!("fatal local failure")
                }
            },
        );
        let reason = format!("{:#}", result.unwrap_err());
        assert!(
            reason.contains("fatal local failure") && reason.contains("cloud failure"),
            "{reason}"
        );
        let progress = last.unwrap().0;
        assert_eq!(
            (progress.completed, progress.failed, progress.cancelled),
            (0, 1, 2)
        );
        assert!(progress.active.is_empty());
        assert_eq!(
            fs::read_to_string(output).unwrap(),
            "previous complete text"
        );
    }

    #[test]
    fn merged_output_budget_stops_new_cloud_submissions() {
        use std::sync::{atomic::AtomicUsize, Condvar, Mutex};
        for width in [1, 2] {
            let root = tempfile::tempdir().unwrap();
            let output = root.path().join("out.md");
            fs::write(&output, "previous complete text").unwrap();
            let parts: Vec<_> = (1..=5)
                .map(|p| Part {
                    bytes: vec![p],
                    start: p.into(),
                    end: p.into(),
                    name: format!("{p}.pdf"),
                })
                .collect();
            let doc = Document::new(&parts, &output, "fixture", 115).unwrap();
            let calls = AtomicUsize::new(0);
            let first_done = (Mutex::new(false), Condvar::new());
            // Each part fits. The range markers and expanded local asset paths
            // push the first two past 115 bytes.
            let result = schedule(
                &doc,
                width,
                &|| false,
                &mut |a| {
                    if a.segment_status.as_ref().unwrap().0.completed == 1 {
                        *first_done.0.lock().unwrap() = true;
                        first_done.1.notify_all();
                    }
                },
                |i, job, stop, _| {
                    calls.fetch_add(1, Ordering::SeqCst);
                    if i == 1 {
                        let (done, _) = first_done
                            .1
                            .wait_timeout_while(
                                first_done.0.lock().unwrap(),
                                std::time::Duration::from_secs(4),
                                |done| !*done,
                            )
                            .unwrap();
                        ensure!(*done, "first part did not finish");
                    }
                    if i == 2 && width == 2 {
                        let start = std::time::Instant::now();
                        while !stop() && start.elapsed() < std::time::Duration::from_millis(250) {
                            std::thread::yield_now();
                        }
                    }
                    fs::write(&job.parsed, "![x](.sag-mineru-assets/f.png)")?;
                    Ok(())
                },
                || {
                    |_, _: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| {
                        panic!("no fallback expected")
                    }
                },
            )
            .and_then(|_| doc.merge(&output, &|| false));
            assert!(super::super::resource_failure(&result.unwrap_err()));
            assert!(
                calls.load(Ordering::SeqCst) <= width + 1,
                "later paid parts started after cumulative output was too large"
            );
            assert_eq!(
                fs::read_to_string(&output).unwrap(),
                "previous complete text"
            );
            assert!(doc.paths[0].exists() && doc.paths[1].exists());
        }
    }

    #[test]
    fn two_documents_finish_when_a_shared_part_needs_the_only_model_slot() {
        use std::sync::{atomic::AtomicUsize, mpsc::channel, Condvar, Mutex};
        use std::time::Duration;
        struct Model<'a> {
            slot: &'a (Mutex<bool>, Condvar),
            holds: bool,
        }
        impl Model<'_> {
            fn acquire(&mut self) -> Result<()> {
                if !self.holds {
                    let (mut used, _) = self
                        .slot
                        .1
                        .wait_timeout_while(
                            self.slot.0.lock().unwrap(),
                            Duration::from_secs(4),
                            |used| *used,
                        )
                        .unwrap();
                    ensure!(
                        !*used,
                        "waiting for the other document's idle model timed out"
                    );
                    *used = true;
                    self.holds = true;
                }
                Ok(())
            }
            fn release(&mut self) {
                if self.holds {
                    self.holds = false;
                    *self.slot.0.lock().unwrap() = false;
                    self.slot.1.notify_all();
                }
            }
        }
        impl Drop for Model<'_> {
            fn drop(&mut self) {
                self.release();
            }
        }
        let root = tempfile::tempdir().unwrap();
        let a_parts = parts();
        let mut b_parts = parts();
        b_parts[0].bytes.push(42);
        let a = Document::new(&a_parts[..2], &root.path().join("a.md"), "fixture", 4096).unwrap();
        let b = Document::new(&b_parts[..2], &root.path().join("b.md"), "fixture", 4096).unwrap();
        let slot = (Mutex::new(false), Condvar::new());
        let (holding, held) = channel();
        let (leased, owned) = channel();
        let owned = Mutex::new(owned);
        let parsed_shared = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                let mut waited = false;
                schedule_with_session(
                    &a,
                    1,
                    &|| false,
                    &mut |attempt| {
                        if !waited && attempt.segment_status.as_ref().unwrap().0.waiting_local == 1
                        {
                            waited = true;
                            owned
                                .lock()
                                .unwrap()
                                .recv_timeout(Duration::from_secs(8))
                                .unwrap();
                        }
                    },
                    |_, _, _, _| anyhow::bail!("local required"),
                    || Model {
                        slot: &slot,
                        holds: false,
                    },
                    |model, i, job, _, _| {
                        model.acquire()?;
                        if i == 0 {
                            holding.send(())?;
                        } else {
                            parsed_shared.fetch_add(1, Ordering::SeqCst);
                        }
                        fs::write(&job.parsed, "A local")?;
                        Ok(())
                    },
                    Model::release,
                )
            });
            held.recv_timeout(Duration::from_secs(8)).unwrap();
            let second = schedule_with_session(
                &b,
                1,
                &|| false,
                &mut |_| {},
                |i, job, _, _| {
                    if i == 0 {
                        fs::write(&job.parsed, "B cloud")?;
                        Ok(())
                    } else {
                        leased.send(())?;
                        anyhow::bail!("local required")
                    }
                },
                || Model {
                    slot: &slot,
                    holds: false,
                },
                |model, _, job, _, _| {
                    model.acquire()?;
                    parsed_shared.fetch_add(1, Ordering::SeqCst);
                    fs::write(&job.parsed, "shared local")?;
                    Ok(())
                },
                Model::release,
            );

            first.join().unwrap().unwrap();
            second.unwrap();
        });
        assert_eq!(parsed_shared.load(Ordering::SeqCst), 1);
        assert_eq!(fs::read_to_string(&a.paths[1]).unwrap(), "shared local");
    }

    #[test]
    fn shared_part_lease_spans_local_fallback_and_waiters_can_cancel() {
        use std::sync::{mpsc::channel, Mutex};
        let root = tempfile::tempdir().unwrap();
        let output = root.path().join("out.md");
        let parts = parts();
        let first = Document::new(&parts[..1], &output, "fixture", 4096).unwrap();
        let second = Document::new(&parts[..1], &output, "fixture", 4096).unwrap();
        let (inside, entered) = channel();
        let (release, wait) = channel();
        let wait = Mutex::new(wait);
        std::thread::scope(|scope| {
            let first_run = scope.spawn(|| {
                schedule(
                    &first,
                    1,
                    &|| false,
                    &mut |_| {},
                    |_, _, _, _| anyhow::bail!("cloud failed"),
                    || {
                        |_, job: &Job, _: &Stop<'_>, _: &mut dyn FnMut(Attempt)| {
                            inside.send(())?;
                            wait.lock()
                                .unwrap()
                                .recv_timeout(std::time::Duration::from_secs(5))?;
                            fs::write(&job.parsed, "local result")?;
                            Ok(())
                        }
                    },
                )
            });
            entered
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            let checks = std::sync::atomic::AtomicUsize::new(0);
            let cancelled = || checks.fetch_add(1, Ordering::Relaxed) > 0;
            assert!(second.begin(0, &cancelled, || ()).is_err());
            let (waiting, waiting_rx) = channel();
            let second = &second;
            let retry = scope.spawn(move || {
                let waiter = || {
                    let _ = waiting.send(());
                    false
                };
                let job = second.begin(0, &waiter, || ()).unwrap();
                assert!(
                    matches!(job, PartInput::Cached(_)),
                    "second conversion must reuse the cached local result"
                );
            });
            waiting_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap();
            release.send(()).unwrap();
            first_run.join().unwrap().unwrap();
            retry.join().unwrap();
        });
    }

    #[test]
    fn parser_session_drops_before_temporary_inputs_on_success_and_failure() {
        use std::sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}};
        struct Session {
            input: Arc<Mutex<Option<std::path::PathBuf>>>,
            saw_live_input_at_drop: Arc<AtomicBool>,
        }
        impl Session {
            fn record(&self, path: &Path) { *self.input.lock().unwrap() = Some(path.to_owned()); }
        }
        impl Drop for Session {
            fn drop(&mut self) {
                self.saw_live_input_at_drop.store(self.input.lock().unwrap().as_ref().unwrap().exists(), Ordering::SeqCst);
            }
        }
        for success in [true, false] {
            let root = tempfile::tempdir().unwrap();
            let input = Arc::new(Mutex::new(None));
            let observed = Arc::new(AtomicBool::new(false));
            let session = Session { input: input.clone(), saw_live_input_at_drop: observed.clone() };
            let result = convert_parts(&parts()[..1], &root.path().join("out.md"), "v1", 1024, &|| false, |_,_,_| {}, move |_, source, output| {
                session.record(source);
                anyhow::ensure!(success, "fixture parser failure");
                fs::write(output, "complete segment")?;
                Ok(())
            });
            assert_eq!(result.is_ok(), success);
            assert!(observed.load(Ordering::SeqCst), "parser must release input handles before removing the work directory");
            assert!(!input.lock().unwrap().as_ref().unwrap().exists());
        }
    }

    #[test]
    fn cached_parts_keep_assets_reachable_and_allow_atomic_parser_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        let convert = |_: &Part, _: &Path, out: &Path| -> Result<()> {
            let assets = out.parent().unwrap().join(".sag-mineru-assets/hash/images");
            fs::create_dir_all(&assets)?;
            fs::write(assets.join("x.png"), b"image")?;
            let mut replacement = tempfile::NamedTempFile::new_in(out.parent().unwrap())?;
            replacement.write_all(b"![curve [red]](.sag-mineru-assets/hash/images/x.png)\n<img src=\".sag-mineru-assets/hash/images/x.png\">\n[ref]: .sag-mineru-assets/hash/images/x.png\n![remote](https://host/.sag-mineru-assets/x.png)\nplain .sag-mineru-assets/ text")?;
            replacement.persist(out).map_err(|e| e.error)?;
            Ok(())
        };
        convert_parts(&parts()[..1], &output, "v1", 1024, &|| false, |_, _, _| {}, convert).unwrap();
        let first = fs::read_to_string(&output).unwrap();
        let destination = ".sag-pdf-parts/.sag-mineru-assets/hash/images/x.png";
        assert!(first.contains(destination));
        assert_eq!(first.matches(destination).count(), 3);
        assert!(first.contains("https://host/.sag-mineru-assets/x.png"));
        assert!(first.contains("plain .sag-mineru-assets/ text"));
        assert_eq!(fs::read(dir.path().join(destination)).unwrap(), b"image");
        convert_parts(&parts()[..1], &output, "v1", 1024, &|| false, |_, _, _| {},
            |_, _, _| panic!("cached parts must not invoke parser")).unwrap();
        assert_eq!(fs::read_to_string(output).unwrap(), first);
    }

    #[test]
    fn retry_reuses_successful_parts_and_never_publishes_partial_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        fs::write(&output, "previous complete document").unwrap();
        let mut visited = vec![];
        let result = convert_parts(
            &parts(),
            &output,
            "version-1",
            1024,
            &|| false, |_, _, _| {},
            |part, _, out| {
                visited.push(part.start);
                if part.start == 51 {
                    anyhow::bail!("middle part failed");
                }
                fs::write(out, format!("body {}", part.start))?;
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(visited, [1, 51]);
        assert_eq!(
            fs::read_to_string(&output).unwrap(),
            "previous complete document"
        );
        visited.clear();
        convert_parts(
            &parts(),
            &output,
            "version-1",
            1024,
            &|| false, |_, _, _| {},
            |part, _, out| {
                visited.push(part.start);
                fs::write(out, format!("body {}", part.start))?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(visited, [51, 101]);
        let merged = fs::read_to_string(&output).unwrap();
        assert!(merged.find("body 1").unwrap() < merged.find("body 51").unwrap());
        assert!(merged.contains("source-pages:101-101"));
        let mut observed=vec![];
        convert_parts(
            &parts(),
            &output,
            "version-1",
            1024,
            &|| false, |part,current,total| observed.push((current,total,part.start,part.end)),
            |_, _, _| panic!("all parts cached"),
        )
        .unwrap();
        assert_eq!(observed,[(1,3,1,50),(2,3,51,100),(3,3,101,101)]);
        let mut count = 0;
        convert_parts(
            &parts(),
            &output,
            "version-2",
            1024,
            &|| false, |_, _, _| {},
            |_, _, out| {
                count += 1;
                fs::write(out, "new version")?;
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(count, 3);
    }
    #[test]
    fn cancellation_and_merged_limit_preserve_existing_output() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        fs::write(&output, "previous").unwrap();
        let stopped = AtomicBool::new(false);
        let result = convert_parts(
            &parts(),
            &output,
            "v1",
            1024,
            &|| stopped.load(Ordering::Acquire),
            |_, _, _| {},
            |_, _, out| {
                fs::write(out, "body")?;
                stopped.store(true, Ordering::Release);
                Ok(())
            },
        );
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), "previous");
        let result = convert_parts(&parts(), &output, "v1", 50, &|| false, |_, _, _| {}, |_, _, out| {
            fs::write(out, "body")?;
            Ok(())
        });
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&output).unwrap(), "previous");
    }
    #[test]
    fn invalid_cache_is_rebuilt_and_temporary_source_links_are_not_merged() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.md");
        let parts = parts();
        convert_parts(
            &parts[..1],
            &output,
            "v1",
            1024,
            &|| false, |_, _, _| {},
            |part, input, out| {
                fs::write(
                    out,
                    format!("[{}](<{}>)\n\nbody", part.name, input.display()),
                )?;
                Ok(())
            },
        )
        .unwrap();
        assert!(!fs::read_to_string(&output)
            .unwrap()
            .contains(".sag-pdf-work-"));
        let entry = fs::read_dir(dir.path().join(".sag-pdf-parts"))
            .unwrap()
            .filter_map(Result::ok)
            .find(|e| e.path().extension().is_some_and(|x| x == "md"))
            .unwrap();
        fs::write(entry.path(), [0xff]).unwrap();
        let mut count = 0;
        convert_parts(&parts[..1], &output, "v1", 1024, &|| false, |_, _, _| {}, |_, _, out| {
            count += 1;
            fs::write(out, "rebuilt")?;
            Ok(())
        })
        .unwrap();
        assert_eq!(count, 1);
        assert!(fs::read_to_string(&output).unwrap().contains("rebuilt"));
    }
}

/// Opt-in acceptance against a caller-supplied PDF and installed document
/// component. Uses an isolated knowledge root/database; never alters the source.
#[cfg(all(test, unix))]
#[test]
#[ignore = "requires SAG_PDF_ACCEPTANCE_SOURCE, SAG_PDF_ACCEPTANCE_DATA and SAG_PDF_ACCEPTANCE_DIR"]
fn real_pdf_conversion_indexing_and_cached_retry() {
    use crate::{conversion, docling::Docling, store::Store};
    use sag_protocol::types::Settings;
    let source = std::path::PathBuf::from(std::env::var("SAG_PDF_ACCEPTANCE_SOURCE").unwrap());
    let original_data = std::path::PathBuf::from(std::env::var("SAG_PDF_ACCEPTANCE_DATA").unwrap());
    let artifacts = std::path::PathBuf::from(std::env::var("SAG_PDF_ACCEPTANCE_DIR").unwrap());
    fs::create_dir_all(&artifacts).unwrap();
    let sandbox = std::env::var("SAG_PDF_ACCEPTANCE_REUSE")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            tempfile::Builder::new()
                .prefix("real-pdf-")
                .tempdir_in(&artifacts)
                .unwrap()
                .keep()
        });
    let root = sandbox.join("library");
    let data = sandbox.join("data");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&data).unwrap();
    let input = root.join("acceptance.pdf");
    if !input.exists() {
        fs::copy(&source, &input).unwrap();
    }
    let original_hash = crate::docling::fingerprint(&source).unwrap();
    assert_eq!(crate::docling::fingerprint(&input).unwrap(), original_hash);
    if !data.join("docling").exists() {
        std::os::unix::fs::symlink(original_data.join("docling"), data.join("docling")).unwrap();
    }
    let docling = Docling::new(&data);
    assert!(
        docling.has_profile(sag_protocol::DoclingProfile::Documents),
        "installed document component required"
    );
    let db = rusqlite::Connection::open_with_flags(
        original_data.join("sag.sqlite"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let raw: String = db
        .query_row("SELECT value FROM preferences", [], |r| r.get(0))
        .unwrap();
    let mut settings: Settings = serde_json::from_str(&raw).unwrap();
    let key = crate::mineru::key(&original_data, &settings.mineru).unwrap();
    crate::mineru::save_key(&data, &settings.mineru, &key).unwrap();
    // Keep service waiting bounded for this acceptance; local parser limits,
    // OCR, models and the actual configured cloud provider are unchanged.
    settings.mineru.request_timeout_secs = 10;
    settings.mineru.task_timeout_secs = 15;
    let store = Store::open(&data.join("sag.sqlite")).unwrap();
    store.save_settings(&settings).unwrap();
    let registered = store.add_root(&root).unwrap();
    let started = std::time::Instant::now();
    println!(
        "acceptance sandbox={} pages={} cloud_task_timeout_secs=15",
        sandbox.display(),
        lopdf::Document::load(&source).unwrap().get_pages().len()
    );
    store.scan(&registered, |_| {}).unwrap();
    let state = store
        .roots()
        .unwrap()
        .into_iter()
        .find(|r| r.path == registered)
        .unwrap();
    println!(
        "root_status={} elapsed_ms={} error={:?}",
        state.status,
        started.elapsed().as_millis(),
        state.error
    );
    for h in store.index_history().unwrap() {
        if let Some(log) = store.index_history_log(h.id).unwrap().raw {
            for line in log.lines() {
                let row: serde_json::Value = serde_json::from_str(line).unwrap();
                if row.get("engine").is_some() && row.get("phase").is_none() {
                    println!(
                        "engine={} elapsed_ms={} status={} error={}",
                        row["engine"], row["elapsed_ms"], row["status"], row["error"]
                    );
                }
            }
        }
    }
    assert_eq!(state.status, "ready", "{:?}", state.error);
    let output = root.join(".sag-converted/acceptance.pdf.md");
    let text = fs::read_to_string(&output).unwrap();
    let page_count = lopdf::Document::load(&source).unwrap().get_pages().len();
    let ranges = regex::Regex::new(r"<!-- source-pages:(\d+)-(\d+) -->").unwrap();
    let actual = ranges
        .captures_iter(&text)
        .map(|c| {
            (
                c[1].parse::<usize>().unwrap(),
                c[2].parse::<usize>().unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let expected = (1..=page_count)
        .step_by(50)
        .map(|start| (start, (start + 49).min(page_count)))
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    assert!(!text.contains(".sag-pdf-work-"));
    let terms = regex::Regex::new(r"[\p{Han}]{2,}|[A-Za-z]{4,}").unwrap();
    let body = text
        .lines()
        .filter(|l| !l.contains("<!--") && !l.starts_with("<a "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        terms.find_iter(&body).take(100).any(|term| !store
            .search_files(term.as_str(), &[], 5)
            .unwrap()
            .is_empty()),
        "merged content must be searchable"
    );
    let retry = std::time::Instant::now();
    conversion::convert(
        &input,
        &output,
        &docling,
        &settings,
        || false,
        |attempt| assert_eq!(attempt.phase.as_deref(), Some("segment"), "cached retry must not invoke a parser"),
    )
    .unwrap();
    assert_eq!(fs::read_to_string(&output).unwrap(), text);
    assert_eq!(crate::docling::fingerprint(&source).unwrap(), original_hash);
    println!(
        "acceptance passed: segments={} output_bytes={} cached_retry_ms={}",
        actual.len(),
        text.len(),
        retry.elapsed().as_millis()
    );
}

#[cfg(all(test, unix))]
#[path = "pdf_segments/full_book_equivalence.rs"]
mod full_book_equivalence;
