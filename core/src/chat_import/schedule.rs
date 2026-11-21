use super::{catalog, destination, import, root, source_key};
use crate::{indexer::IndexRequest, store::{self, Store}};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Local, NaiveTime};
use rusqlite::{OptionalExtension, params};
use sag_protocol::{ChatImportRun, ChatImportSchedule, ChatImportScheduleStatus, ChatSource};
use std::sync::mpsc;
use tokio_util::sync::CancellationToken;

fn load(store: &Store, source: ChatSource) -> Result<Option<(ChatImportScheduleStatus, i64)>> {
    let saved = store.connection().query_row(
        "SELECT enabled,time,source_root,include_tools,configured_at,last_run_at,last_run,interval_minutes FROM chat_import_schedules WHERE source=?",
        [source_key(source)],
        |row| Ok((ChatImportSchedule { enabled: row.get(0)?, time: row.get(1)?, source_root: row.get(2)?, include_tools: row.get(3)?, interval_minutes: row.get(7)? },
            row.get::<_, i64>(4)?, row.get::<_, Option<i64>>(5)?, row.get::<_, Option<String>>(6)?)),
    ).optional()?;
    saved.map(|(schedule, configured_at, last_run_at, last_run)| {
        let last_run = last_run.as_deref().map(serde_json::from_str).transpose()?;
        Ok((ChatImportScheduleStatus { schedule, last_run_at, last_run }, configured_at))
    }).transpose()
}

pub fn status(store: &Store, source: ChatSource) -> Result<ChatImportScheduleStatus> {
    Ok(load(store, source)?.map(|(status, _)| status).unwrap_or_default())
}

pub fn save(store: &Store, source: ChatSource, mut schedule: ChatImportSchedule) -> Result<()> {
    ensure!(schedule.time.len() == 5 && NaiveTime::parse_from_str(&schedule.time, "%H:%M").is_ok(), "请输入有效的每日导入时间");
    ensure!(matches!(schedule.interval_minutes, None | Some(30 | 60 | 360 | 720)), "请选择有效的同步频率");
    schedule.source_root = schedule.source_root.map(|path| path.trim().to_owned()).filter(|path| !path.is_empty());
    let path = root(source, schedule.source_root.as_deref())?;
    if schedule.enabled {
        ensure!(path.is_dir(), "聊天记录文件夹不存在");
    }
    store.connection().execute(
        "INSERT INTO chat_import_schedules(source,enabled,time,source_root,include_tools,configured_at,interval_minutes) VALUES(?1,?2,?3,?4,?5,?6,?7)
         ON CONFLICT(source) DO UPDATE SET enabled=excluded.enabled,time=excluded.time,source_root=excluded.source_root,
         include_tools=excluded.include_tools,interval_minutes=excluded.interval_minutes,
         configured_at=CASE WHEN enabled!=excluded.enabled OR time!=excluded.time OR interval_minutes IS NOT excluded.interval_minutes
         THEN excluded.configured_at ELSE configured_at END",
        params![source_key(source), schedule.enabled, schedule.time, schedule.source_root, schedule.include_tools, store::now(), schedule.interval_minutes],
    )?;
    Ok(())
}

pub fn run_due(
    store: &Store,
    scan: &mpsc::Sender<IndexRequest>,
    source: ChatSource,
    now: DateTime<Local>,
    stop: &CancellationToken,
) -> Result<Option<ChatImportRun>> {
    let Some((status, configured_at)) = load(store, source)? else { return Ok(None) };
    if !status.schedule.enabled || stop.is_cancelled() { return Ok(None) }
    if let Some(minutes) = status.schedule.interval_minutes {
        let anchor = configured_at.max(status.last_run_at.unwrap_or(configured_at));
        if now.timestamp_millis() < anchor + i64::from(minutes) * 60_000 { return Ok(None) }
    } else {
        let time = NaiveTime::parse_from_str(&status.schedule.time, "%H:%M")?;
        let due = now.date_naive().and_time(time);
        let configured = DateTime::from_timestamp_millis(configured_at).context("定时导入配置时间无效")?.with_timezone(&Local);
        let already_ran = status.last_run_at.map(|stamp| {
            DateTime::from_timestamp_millis(stamp).context("上次定时导入时间无效")
                .map(|last| last.with_timezone(&Local).date_naive() == now.date_naive())
        }).transpose()? == Some(true);
        if now.naive_local() < due || configured.naive_local() >= due + chrono::Duration::minutes(1) || already_ran { return Ok(None) }
    }

    let run = run_import(store, scan, source, &status.schedule, stop).unwrap_or_else(|error| {
        ChatImportRun { changed: 0, failed: 1, error: Some(error.to_string()) }
    });
    if !stop.is_cancelled() {
        store.connection().execute(
            "UPDATE chat_import_schedules SET last_run_at=?,last_run=? WHERE source=?",
            params![now.timestamp_millis(), serde_json::to_string(&run)?, source_key(source)],
        )?;
    }
    Ok(Some(run))
}

fn run_import(store: &Store, scan: &mpsc::Sender<IndexRequest>, source: ChatSource, schedule: &ChatImportSchedule, stop: &CancellationToken) -> Result<ChatImportRun> {
    let path = root(source, schedule.source_root.as_deref())?;
    ensure!(path.is_dir(), "聊天记录文件夹不存在");
    let catalog = catalog(source, Some(path.to_str().context("聊天记录文件夹路径无效")?))?;
    let mut run = ChatImportRun {
        failed: catalog.issues.len(),
        error: catalog.issues.first().map(|issue| issue.error.clone()),
        ..Default::default()
    };
    if catalog.sessions.is_empty() || stop.is_cancelled() { return Ok(run) }
    let documents = dirs::document_dir().context("无法获取文稿目录")?;
    let folder = destination(store, scan, &documents, &store.settings()?)?;
    for session in catalog.sessions {
        if stop.is_cancelled() { break; }
        match import(store, scan, source, &catalog.source_root, &session, &folder.to_string_lossy(), schedule.include_tools) {
            Ok(item) if item.status != "skipped" => run.changed += 1,
            Ok(_) => {},
            Err(error) => {
                run.failed += 1;
                run.error.get_or_insert_with(|| error.to_string());
            }
        }
    }
    Ok(run)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use rusqlite::Connection;

    #[test]
    fn existing_daily_settings_and_elapsed_intervals_survive_reopening() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("index.sqlite");
        let base = Local::now();
        let source = ChatSource::Claude;
        let (scan, _requests) = mpsc::channel();
        let stop = CancellationToken::new();
        let old = Connection::open(&db).unwrap();
        old.execute_batch("CREATE TABLE chat_import_schedules(source TEXT PRIMARY KEY,enabled INTEGER NOT NULL,time TEXT NOT NULL,source_root TEXT,include_tools INTEGER NOT NULL,configured_at INTEGER NOT NULL,last_run_at INTEGER,last_run TEXT);").unwrap();
        old.execute("INSERT INTO chat_import_schedules VALUES('claude',1,'18:20',?1,1,?2,NULL,NULL)",
            params![dir.path().to_str().unwrap(), base.timestamp_millis()]).unwrap();
        drop(old);

        let store = Store::open(&db).unwrap();
        let mut schedule = status(&store, source).unwrap().schedule;
        assert!(schedule.enabled && schedule.include_tools);
        assert_eq!(schedule.time, "18:20");
        assert_eq!(schedule.interval_minutes, None);
        schedule.interval_minutes = Some(30);
        save(&store, source, schedule.clone()).unwrap();
        store.connection().execute("UPDATE chat_import_schedules SET configured_at=?1", [base.timestamp_millis()]).unwrap();
        schedule.include_tools = false;
        save(&store, source, schedule.clone()).unwrap();
        assert!(run_due(&store, &scan, source, base + Duration::minutes(29), &stop).unwrap().is_none());
        assert!(run_due(&store, &scan, source, base + Duration::minutes(30), &stop).unwrap().is_some());
        drop(store);

        let store = Store::open(&db).unwrap();
        assert_eq!(status(&store, source).unwrap().schedule.interval_minutes, Some(30));
        assert!(run_due(&store, &scan, source, base + Duration::minutes(59), &stop).unwrap().is_none());
        let resumed = base + Duration::hours(6);
        assert!(run_due(&store, &scan, source, resumed, &stop).unwrap().is_some());
        assert!(run_due(&store, &scan, source, resumed + Duration::seconds(30), &stop).unwrap().is_none());
        schedule.enabled = false;
        save(&store, source, schedule).unwrap();
        assert!(run_due(&store, &scan, source, resumed + Duration::days(1), &stop).unwrap().is_none());
    }
}
