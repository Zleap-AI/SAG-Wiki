mod account_auth;
mod apify;
mod api_source;
mod remote;
#[cfg(test)]
mod tests;

use crate::{indexer, models, service::Service, store::Store};
use anyhow::{ensure, Context, Result};
use chrono::{Local, NaiveTime, TimeZone, Utc};
use rusqlite::{params, OptionalExtension};
use sag_protocol::online_import::{Action, Connection, Platform, Schedule, Source};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{OwnedRwLockWriteGuard, RwLock};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
pub(crate) struct Jobs {
    access: Arc<RwLock<()>>,
    active: Mutex<HashMap<String, CancellationToken>>,
    account_login: Mutex<Option<account_auth::Login>>,
    account_refresh: tokio::sync::Mutex<()>,
    account_limits: Arc<remote::AccountLimits>,
    apify_import: tokio::sync::Mutex<()>,
}

impl Jobs {
    pub fn exclusive(&self) -> Result<OwnedRwLockWriteGuard<()>> {
        self.access
            .clone()
            .try_write_owned()
            .context("请先停止正在同步的数据源")
    }

    pub async fn wait(&self) {
        let _guard = self.access.write().await;
    }
}

#[derive(Default, Deserialize, Serialize)]
struct Report {
    status: String,
    imported: usize,
    updated: usize,
    skipped: usize,
    conflicts: usize,
    failed: usize,
    issues: Vec<Issue>,
}

#[derive(Deserialize, Serialize)]
struct Issue {
    resource: String,
    item: Option<String>,
    title: String,
    reason: String,
}

impl Report {
    fn issue(
        &mut self,
        resource: &str,
        item: Option<&remote::Item>,
        reason: String,
        failure: bool,
    ) {
        if failure {
            self.failed += 1;
        } else {
            self.conflicts += 1;
        }
        self.issues.push(Issue {
            resource: resource.into(),
            item: item.map(|item| item.id.clone()),
            title: item.map_or_else(|| resource.to_string(), |item| item.title.clone()),
            reason,
        });
    }
}

#[derive(Serialize, Deserialize)]
struct FileRecord {
    path: String,
    hash: String,
}

fn now() -> i64 {
    Utc::now().timestamp()
}

fn next_run(source: &Source, after: i64) -> Result<Option<i64>> {
    if source.paused || (source.connection.platform != Platform::Apify && source.resources.is_empty()) {
        return Ok(None);
    }
    match source.schedule {
        Schedule::Manual => Ok(None),
        Schedule::HalfHourly => Ok(Some(after + 1800)),
        Schedule::Hourly => Ok(Some(after + 3600)),
        Schedule::EverySixHours => Ok(Some(after + 6 * 3600)),
        Schedule::EveryTwelveHours => Ok(Some(after + 12 * 3600)),
        Schedule::Daily => {
            let time = NaiveTime::parse_from_str(&source.daily_time, "%H:%M")
                .context("请选择每日同步时间")?;
            let mut target = Local
                .timestamp_opt(after, 0)
                .single()
                .context("同步时间无效")?
                .date_naive()
                .and_time(time);
            loop {
                if let Some(instant) = Local.from_local_datetime(&target).earliest() {
                    if instant.timestamp() > after {
                        return Ok(Some(instant.timestamp()));
                    }
                    target = target
                        .checked_add_days(chrono::Days::new(1))
                        .context("同步时间无效")?;
                } else {
                    target = target
                        .checked_add_signed(chrono::TimeDelta::minutes(1))
                        .context("同步时间无效")?;
                }
            }
        }
    }
}

fn normalize_resources(source: &mut Source) {
    if source.connection.platform == Platform::Github {
        source.resources = remote::github_resources(&source.resources);
    }
}

fn source(store: &Store, id: &str) -> Result<(Source, Option<i64>)> {
    let (config, next) = store
        .connection()
        .query_row(
            "SELECT config,next_at FROM online_sources WHERE id=?1",
            [id],
            |row| Ok((row.get::<_, String>(0)?, row.get(1)?)),
        )
        .optional()?
        .context("连接不存在")?;
    let mut source = serde_json::from_str(&config)?;
    normalize_resources(&mut source);
    Ok((source, next))
}

fn history(store: &Store, id: &str) -> Result<Vec<Value>> {
    let db = store.connection();
    let mut query = db.prepare(
        "SELECT started,finished,result FROM online_runs WHERE source=?1 ORDER BY id DESC LIMIT 20",
    )?;
    let rows = query.query_map([id], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    rows.map(|row| -> Result<Value> {let (started,finished,result)=row?;Ok(json!({"started":started,"finished":finished,"result":serde_json::from_str::<Report>(&result)?}))}).collect()
}

fn platform_slug(platform: Platform) -> &'static str {
    match platform {
        Platform::Feishu => "feishu",
        Platform::Lark => "lark",
        Platform::Notion => "notion",
        Platform::Yuque => "yuque",
        Platform::Github => "github",
        Platform::Gitlab => "gitlab",
        Platform::Dingtalk => "dingtalk",
        Platform::Ima => "ima",
        Platform::Rss => "rss",
        Platform::Apify => "apify",
        Platform::Api => "api",
    }
}

fn name_part(name: &str) -> String {
    let name: String = name
        .chars()
        .filter(|c| !c.is_control())
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .take(70)
        .collect();
    let name = name.trim_matches(|c: char| c == '.' || c.is_whitespace());
    let stem = name
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    match stem.as_str() {
        "" => "untitled".into(),
        "CON" | "PRN" | "AUX" | "NUL" | "COM1" | "COM2" | "COM3" | "COM4" | "COM5" | "COM6"
        | "COM7" | "COM8" | "COM9" | "LPT1" | "LPT2" | "LPT3" | "LPT4" | "LPT5" | "LPT6"
        | "LPT7" | "LPT8" | "LPT9" => format!("_{name}"),
        _ => name.into(),
    }
}

fn normal_connection(mut connection: Connection) -> Result<Connection> {
    ensure!(connection.platform != Platform::Apify, "请使用 Apify 导入入口");
    ensure!(!connection.user_access || connection.platform.supports_account_login(), "此数据源不支持账号授权");
    connection.app_id = connection.app_id.trim().into();
    connection.address = connection.address.trim().into();
    match connection.platform {
        Platform::Github => connection.address = remote::github_address(&connection.address)?,
        Platform::Gitlab => connection.address = remote::gitlab_address(&connection.address)?,
        Platform::Dingtalk => ensure!(
            !connection.app_id.is_empty() && !connection.address.is_empty(),
            "请填写 Client ID 和操作人 Union ID"
        ),
        Platform::Ima => ensure!(!connection.app_id.is_empty(), "请填写 Client ID"),
        _ => {}
    }
    if matches!(connection.platform, Platform::Feishu | Platform::Lark) && connection.drive {
        if let Ok(url) = reqwest::Url::parse(&connection.address) {
            let host = url.host_str().unwrap_or_default();
            let suffix = if connection.platform == Platform::Feishu {
                "feishu.cn"
            } else {
                "larksuite.com"
            };
            ensure!(
                url.scheme() == "https"
                    && (host == suffix || host.ends_with(&format!(".{suffix}"))),
                "请选择对应平台的云盘链接"
            );
            let segments = url
                .path_segments()
                .context("云盘链接无效")?
                .collect::<Vec<_>>();
            let token = segments
                .windows(2)
                .find(|parts| parts[0] == "folder")
                .context("请输入云盘文件夹链接或标识")?[1];
            connection.address = token.into();
        }
    }
    Ok(connection)
}

fn documents_dir() -> Option<PathBuf> {
    dirs::document_dir().or_else(|| dirs::home_dir().map(|home| home.join("Documents")))
}

pub(crate) async fn handle(service: &Arc<Service>, action: Action) -> Result<Value> {
    let documents = documents_dir();
    match action {
        Action::Apify { token, connection, request } => {
            apify::handle(service, &token, connection.as_deref(), request).await
        }
        Action::AccountLogin { platform, app_id, app_secret, drive } => {
            account_auth::start(service, platform.unwrap_or(Platform::Feishu), app_id, app_secret, drive).await
        }
        Action::AccountLoginStatus { session } => account_auth::status(&service.online_import, &session),
        Action::CancelAccountLogin { session } => {
            account_auth::cancel_login(&service.online_import, &session);
            Ok(Value::Null)
        }
        Action::List { platform } => {
            let rows = {
                let db = service.store.connection();
                let mut query =
                    db.prepare("SELECT config,next_at FROM online_sources ORDER BY rowid")?;
                let rows = query.query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            let sources = rows
                .into_iter()
                .map(|(config, next)| Ok((serde_json::from_str::<Source>(&config)?, next)))
                .collect::<Result<Vec<_>>>()?;
            let mut result = vec![];
            for (mut source, next_at) in sources
                .into_iter()
                .filter(|(source, _)| source.connection.platform == platform
                    && (platform != Platform::Apify || !source.connection.address.is_empty()))
            {
                normalize_resources(&mut source);
                let last = history(&service.store, &source.id)?.into_iter().next();
                let running = service
                    .online_import
                    .active
                    .lock()
                    .unwrap()
                    .contains_key(&source.id);
                let pending = platform == Platform::Api && api_source::has_pending(&service.store, &source.id)?;
                result.push(json!({"source":source,"next_at":next_at,"last":last,"running":running,"pending":pending}));

            }
            Ok(json!({"sources":result,"documents":documents}))
        }
        Action::ApiSample { id, request, secret } => {
            let existing = id.as_deref().map(|id| source(&service.store, id)).transpose()?;
            let previous = existing.as_ref().and_then(|(source, _)| source.api.as_ref());
            let token = api_source::secret(service, previous, id.as_deref(), &request, secret)?;
            api_source::sample(&request, token.as_deref()).await
        }
        Action::ApiPreview { config, response } => Ok(json!(api_source::preview(service, &config, &response).await?)),
        Action::Browse {
            id,
            connection,
            secret,
            auth_session,
            parent,
            cursor,
        } => {
            let _guard = service.online_import.access.clone().try_read_owned().context("正在清除本地数据")?;
            let connection = normal_connection(connection)?;
            ensure!(
                !connection.platform.supports_account_login() || connection.user_access,
                "请先完成账号授权"
            );
            let token = if let Some(session) = auth_session {
                let grant = account_auth::pending(&service.online_import, &session, &connection)?;
                account_auth::token(service, None, &connection, Some(grant)).await?
            } else if connection.user_access {
                let reconnect = || account_auth::Reconnect(connection.platform);
                let id = id.as_deref().with_context(reconnect)?;
                let previous = source(&service.store, id)?.0.connection;
                ensure!(
                    previous.user_access && previous.platform == connection.platform
                        && previous.drive == connection.drive && previous.app_id == connection.app_id,
                    reconnect()
                );
                account_auth::token(service, Some(id), &connection, None).await?
            } else {
                read_secret(service, id.as_deref(), &connection, secret)?
            };
            let remote = remote::Remote::open(connection, token, &service.online_import.account_limits).await?;
            Ok(serde_json::to_value(
                remote.browse(parent.as_deref(), cursor.as_deref()).await?,
            )?)
        }
        Action::Save { mut source, secret, auth_session } => {
            let _guard = service
                .online_import
                .access
                .clone()
                .try_read_owned()
                .context("正在清除本地数据")?;
            ensure!(!service.stop.is_cancelled(), "后台正在退出");
            source.connection = normal_connection(source.connection)?;
            let _refresh = service.online_import.account_refresh.lock().await;
            let current = if source.id.is_empty() {
                None
            } else {
                Some(self::source(&service.store, &source.id)?.0)
            };
            let token = if source.connection.platform == Platform::Api {
                let config = source.api.as_ref().context("API 请求配置缺失")?;
                let token = api_source::secret(
                    service,
                    current.as_ref().and_then(|source| source.api.as_ref()),
                    current.as_ref().map(|source| source.id.as_str()),
                    &config.request,
                    secret,
                )?;
                api_source::validate(config, token.as_deref())?;
                if let Some(cleanup) = &config.cleanup {
                    ensure!(service.store.settings()?.answer.models.contains(&cleanup.model), "请为 API 清洗选择已配置的问答模型");
                }
                source.resources = vec!["api".into()];
                token
            } else if source.connection.user_access {
                if let Some(session) = auth_session.as_deref() {
                    Some(account_auth::pending(&service.online_import, session, &source.connection)?)
                } else {
                    let reconnect = || account_auth::Reconnect(source.connection.platform);
                    let previous = current.as_ref().with_context(reconnect)?;
                    ensure!(
                        previous.connection.user_access
                            && previous.connection.platform == source.connection.platform
                            && previous.connection.app_id == source.connection.app_id,
                        reconnect()
                    );
                    Some(models::online_secret(&service.data_dir, &previous.id)?.with_context(reconnect)?)
                }
            } else {
                ensure!(
                    !source.connection.platform.supports_account_login(),
                    "请先完成账号授权"
                );
                read_secret(service, current.as_ref().map(|source| source.id.as_str()), &source.connection, secret)?
            };
            ensure!(
                !source.connection.platform.requires_secret() || token.is_some(),
                "请填写连接凭据"
            );
            source.name = source.name.trim().into();
            ensure!(
                !source.name.is_empty() && source.name.chars().count() <= 100,
                "请输入不超过 100 字的连接名称"
            );
            ensure!(
                source.resources.len() <= 500
                    && (!source.resources.is_empty() || source.connection.user_access),
                "请选择 1 至 500 个导入范围"
            );
            normalize_resources(&mut source);
            let first_import = !source.resources.is_empty()
                && current.as_ref().is_none_or(|previous| previous.resources.is_empty());
            let instant = now();
            let scheduled = next_run(&source, instant)?;
            let next = if first_import { Some(instant) } else { scheduled };
            let active = service.online_import.active.lock().unwrap();
            ensure!(!active.contains_key(&source.id), "请先停止此连接的同步任务");
            if let Some(current) = current {
                ensure!(
                    current.connection.platform == source.connection.platform
                        && current.connection.drive == source.connection.drive,
                    "已保存连接不能切换平台或类型"
                );
                ensure!(
                    current.directory == source.directory,
                    "已保存连接不能更改保存位置"
                );
                ensure!(
                    !source.connection.platform.is_repository()
                        || current.connection.address == source.connection.address,
                    "已保存的连接不能更换仓库"
                );
            } else {
                source.id = uuid::Uuid::new_v4().to_string();
                let parent = if source.directory.trim().is_empty() {
                    documents
                        .context("无法获取文稿目录")?
                        .join("sag-wiki/data-sources")
                        .join(platform_slug(source.connection.platform))
                } else {
                    let parent = PathBuf::from(&source.directory);
                    ensure!(
                        parent.is_absolute() && parent.is_dir(),
                        "请选择已存在的保存目录"
                    );
                    parent
                };
                source.directory = parent
                    .canonicalize()
                    .or_else(|error| {
                        if error.kind() == std::io::ErrorKind::NotFound {
                            Ok(parent)
                        } else {
                            Err(error)
                        }
                    })?
                    .join(format!("{}-{}", name_part(&source.name), &source.id[..8]))
                    .to_string_lossy()
                    .into_owned();
            }
            let db = service.store.connection();
            models::save_online_secret(&service.data_dir, &source.id, token.as_deref())?;
            db.execute("INSERT INTO online_sources(id,config,next_at) VALUES(?1,?2,?3) ON CONFLICT(id) DO UPDATE SET config=excluded.config,next_at=excluded.next_at",params![source.id,serde_json::to_string(&source)?,next])?;
            if let Some(session) = auth_session.as_deref() {
                account_auth::cancel_login(&service.online_import, session);
            }
            drop(db);
            drop(active);
            drop(_guard);
            if first_import {
                start(service, &source.id, false, true)?;
            }
            Ok(json!(source))
        }
        Action::History { id } => Ok(json!(history(&service.store, &id)?)),
        Action::Run { id, retry } => Ok(json!(start(service, &id, retry, false)?)),
        Action::Cancel { id } => {
            if let Some(task) = service.online_import.active.lock().unwrap().get(&id) {
                task.cancel();
            }
            Ok(Value::Null)
        }
        Action::Remove { id } => {
            let _guard = service
                .online_import
                .access
                .clone()
                .try_read_owned()
                .context("正在清除本地数据")?;
            let _refresh = service.online_import.account_refresh.lock().await;
            let active = service.online_import.active.lock().unwrap();
            ensure!(!active.contains_key(&id), "请先停止此连接的同步任务");
            let db = service.store.connection();
            models::save_online_secret(&service.data_dir, &id, None)?;
            db.execute("DELETE FROM online_sources WHERE id=?1", [id])?;
            Ok(Value::Null)
        }
    }
}

fn read_secret(
    service: &Service,
    id: Option<&str>,
    connection: &Connection,
    provided: Option<String>,
) -> Result<Option<String>> {
    if connection.platform == Platform::Rss {
        return Ok(None);
    }
    if let Some(provided) = provided.filter(|provided| !provided.trim().is_empty()) {
        ensure!(provided.len() <= 4096, "连接凭据过长");
        return Ok(Some(provided.trim().into()));
    }
    if let Some(id) = id {
        let previous = source(&service.store, id)?.0.connection;
        ensure!(
            !previous.user_access && previous.platform == connection.platform && previous.app_id == connection.app_id.trim()
                && (!connection.platform.is_repository() || previous.address == connection.address),
            "更改账号后请重新填写连接凭据"
        );
        models::online_secret(&service.data_dir, id)
    } else {
        Ok(None)
    }
}

fn start(service: &Arc<Service>, id: &str, retry: bool, allow_pending: bool) -> Result<bool> {
    let guard = service
        .online_import
        .access
        .clone()
        .try_read_owned()
        .context("正在清除本地数据")?;
    ensure!(!service.stop.is_cancelled(), "后台正在退出");
    let mut active = service.online_import.active.lock().unwrap();
    if active.contains_key(id) {
        return Ok(false);
    }
    if active.len() >= 2 && allow_pending {
        return Ok(false);
    }
    ensure!(active.len() < 2, "同步任务已满，请等当前任务完成");
    let (source, next_at) = source(&service.store, id)?;
    let platform = source.connection.platform;
    if platform == Platform::Apify {
        if allow_pending && (source.paused || next_at.is_none_or(|next| next > now())) {
            return Ok(false);
        }
        ensure!(source.schedule != Schedule::Manual, "请使用 Apify 导入入口");
    } else if source.resources.is_empty() {
        ensure!(
            allow_pending,
            "{}",
            if source.connection.drive { "未选择文件夹" } else { "未选择知识库或页面" }
        );
        return Ok(false);
    }
    if let Some(retry_at) = service.online_import.account_limits.retry_at(platform, &source.connection.app_id) {
        if !allow_pending {
            return Err(remote::AccountLimit::Rate { platform, retry_at }.into());
        }
        service.store.connection().execute(
            "UPDATE online_sources SET next_at=?2 WHERE id=?1",
            params![id, retry_at],
        )?;
        return Ok(false);
    }
    let filter = if retry && !matches!(platform, Platform::Api | Platform::Apify) {
        let recent = history(&service.store, id)?
            .into_iter()
            .next()
            .context("暂无同步记录")?;
        let result: Report = serde_json::from_value(recent["result"].clone())?;
        if result.status == "interrupted"
            || result.issues.iter().any(|issue| issue.resource.is_empty())
        {
            None
        } else {
            let issues: Vec<_> = result.issues.into_iter()
                .filter(|issue| source.resources.contains(&issue.resource))
                .collect();
            (!issues.is_empty() || platform != Platform::Github).then_some(issues)
        }
    } else {
        None
    };
    let cancel = service.stop.child_token();
    let started = now();
    let report = Report {
        status: "running".into(),
        ..Default::default()
    };
    let run = {
        let mut db = service.store.connection();
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO online_runs(source,started,result) VALUES(?1,?2,?3)",
            params![id, started, serde_json::to_string(&report)?],
        )?;
        let run = tx.last_insert_rowid();
        tx.commit()?;
        run
    };
    active.insert(id.into(), cancel.clone());
    let service = service.clone();
    tokio::spawn(async move {
        let _guard = guard;
        if let Err(error) = execute(&service, &source, run, cancel, filter, retry, report).await {
            eprintln!("记录在线数据源同步结果失败：{error:#}");
        }
        service
            .online_import
            .active
            .lock()
            .unwrap()
            .remove(&source.id);
    });
    Ok(true)
}

async fn execute(
    service: &Arc<Service>,
    source: &Source,
    run: i64,
    cancel: CancellationToken,
    filter: Option<Vec<Issue>>,
    retry: bool,
    mut report: Report,
) -> Result<()> {
    let result = if source.connection.platform == Platform::Apify {
        apify::sync(service, source, &cancel, &mut report).await
    } else {
        let secret = models::online_secret(&service.data_dir, &source.id);
        let token = match secret {
            Ok(secret) if source.connection.platform == Platform::Api => Ok(secret),
            Ok(secret) => account_auth::token(service, Some(&source.id), &source.connection, secret).await,
            Err(error) => Err(error),
        };
        match token {
            Err(error) => Err(error),
            Ok(token) => tokio::select! {
                _=cancel.cancelled()=>Err(anyhow::anyhow!("同步已取消")),
                result=async {
                    if source.connection.platform == Platform::Api {
                        return api_source::sync(service, source, retry, token.as_deref(), &cancel, &mut report).await;
                    }
                    let remote = remote::Remote::open(
                        source.connection.clone(),
                        token,
                        &service.online_import.account_limits,
                    ).await?;
                    sync(service, source, run, filter.as_deref(), &mut report, &remote, documents_dir().as_deref()).await
                }=>result,
            },
        }
    };
    if source.connection.platform == Platform::Apify && cancel.is_cancelled() {
        if let Err(error) = &result {
            report.issue("", None, error.to_string(), true);
        }
    }
    let limit = result
        .as_ref()
        .err()
        .and_then(|error| error.downcast_ref::<remote::AccountLimit>())
        .copied();
    let reauthorize = result.as_ref().is_err_and(|error| error.is::<account_auth::Reconnect>());
    report.status = match result {
        _ if service.stop.is_cancelled() => "interrupted",
        _ if cancel.is_cancelled() => "canceled",
        Err(error) => {
            report.issue("", None, error.to_string(), true);
            if limit.is_some() && report.imported + report.updated > 0 {
                "partial"
            } else {
                "failed"
            }
        }
        Ok(()) if report.failed + report.conflicts == 0 => "success",
        Ok(())
            if report.imported + report.updated + report.skipped == 0 && report.conflicts == 0 =>
        {
            "failed"
        }
        Ok(()) => "partial",
    }
    .into();
    let db = service.store.connection();
    let mut db = db;
    let tx = db.transaction()?;
    tx.execute(
        "UPDATE online_runs SET finished=?2,result=?3 WHERE id=?1",
        params![run, now(), serde_json::to_string(&report)?],
    )?;
    if report.status != "interrupted" {
        let inaccessible = report.issues.iter().any(|issue| {
            matches!(
                issue.reason.as_str(),
                "授权失败或无权读取该资源" | "请填写连接凭据" | "飞书或 Lark 应用授权失败"
            )
        }) && report.imported + report.updated + report.skipped == 0;
        let next = if matches!(limit, Some(remote::AccountLimit::Quota(_))) || reauthorize || inaccessible {
            None
        } else {
            let next = next_run(source, now())?;
            match limit {
                Some(remote::AccountLimit::Rate { retry_at, .. }) => next.map(|next| next.max(retry_at)),
                _ => next,
            }
        };
        tx.execute(
            "UPDATE online_sources SET next_at=?2 WHERE id=?1",
            params![source.id, next],
        )?;
    }
    tx.execute("DELETE FROM online_runs WHERE source=?1 AND id NOT IN (SELECT id FROM online_runs WHERE source=?1 ORDER BY id DESC LIMIT 20)",[&source.id])?;
    tx.commit()?;
    Ok(())
}

async fn sync(
    service: &Service,
    source: &Source,
    run: i64,
    filter: Option<&[Issue]>,
    report: &mut Report,
    remote: &remote::Remote,
    documents: Option<&Path>,
) -> Result<()> {
    let directory = PathBuf::from(&source.directory);
    let mut seen = HashSet::new();
    let mut index_root = None;
    let mut checked_existing_root = false;
    for resource in &source.resources {
        let filters = filter.map(|issues| {
            issues
                .iter()
                .filter(|issue| issue.resource == *resource)
                .collect::<Vec<_>>()
        });
        if filters.as_ref().is_some_and(Vec::is_empty) {
            continue;
        }
        let items = match remote.items(resource).await {
            Ok(items) => items,
            Err(error) if error.is::<account_auth::Reconnect>() || error.is::<remote::AccountLimit>() => {
                return Err(error);
            }
            Err(error) => {
                report.issue(resource, None, error.to_string(), true);
                continue;
            }
        };
        for item in items {
            if filters.as_ref().is_some_and(|issues| {
                !issues
                    .iter()
                    .any(|issue| issue.item.as_deref().is_none_or(|id| id == item.id))
            }) {
                continue;
            }
            if !seen.insert(item.id.clone()) {
                continue;
            }
            let outcome = sync_item(&service.store, source, remote, &directory, &item).await;
            let saved = matches!(&outcome, Ok(Outcome::Written { .. } | Outcome::Skipped));
            let written = match outcome {
                Ok(Outcome::Skipped) => {
                    report.skipped += 1;
                    None
                }
                Ok(Outcome::Conflict(reason)) => {
                    report.issue(resource, Some(&item), reason, false);
                    None
                }
                Ok(Outcome::Written { path, updated }) => {
                    if updated {
                        report.updated += 1;
                    } else {
                        report.imported += 1;
                    }
                    Some(path)
                }
                Err(error) if error.is::<account_auth::Reconnect>() || error.is::<remote::AccountLimit>() => {
                    return Err(error);
                }
                Err(error) => {
                    report.issue(resource, Some(&item), error.to_string(), true);
                    None
                }
            };
            if index_root.is_none() && (written.is_some() || (saved && !checked_existing_root)) {
                checked_existing_root = true;
                index_root = ensure_index_root(service, source, &directory, documents, written.is_some())?;
            }
            if let Some(path) = written {
                service.scan.send(indexer::IndexRequest::Paths {
                    root: index_root.as_ref().unwrap().clone(),
                    paths: vec![path.canonicalize()?],
                })?;
            }
            service.store.connection().execute(
                "UPDATE online_runs SET result=?2 WHERE id=?1",
                params![run, serde_json::to_string(&report)?],
            )?;
        }
    }
    Ok(())
}

fn ensure_index_root(
    service: &Service,
    source: &Source,
    directory: &Path,
    documents: Option<&Path>,
    wrote_file: bool,
) -> Result<Option<String>> {
    let directory = directory.canonicalize()?;
    let platform = Path::new("sag-wiki/data-sources").join(platform_slug(source.connection.platform));
    let mut target = directory.clone();
    if let Some((documents, parent)) = documents
        .zip(directory.parent())
        .filter(|(_, parent)| parent.ends_with(&platform))
    {
        match documents.canonicalize() {
            Ok(documents) if parent == documents.join(&platform) => {
                target = documents.join("sag-wiki");
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let roots = service.store.roots()?;
    if let Some(root) = roots.iter().find(|root| target.starts_with(&root.path)) {
        return Ok(Some(root.path.clone()));
    }
    // An unchanged item can group a legacy root, but should not re-add a
    // destination the user has removed from the sidebar.
    let old_root = target != directory && roots.iter().any(|root| directory.starts_with(&root.path));
    if wrote_file || old_root {
        return indexer::add_root(&service.store, &service.scan, &target).map(Some);
    }
    Ok(None)
}

enum Outcome {
    Skipped,
    Conflict(String),
    Written { path: PathBuf, updated: bool },
}

pub(super) fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn file_hash(path: &Path) -> Result<Option<String>> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            ensure!(
                meta.is_file() && meta.len() <= 100 * 1024 * 1024,
                "同步文件已被替换或超出大小上限"
            );
            Ok(Some(digest(&fs::read(path)?)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn save_file(path: &Path, bytes: &[u8], replace: bool) -> Result<()> {
    let parent = path.parent().context("保存路径无效")?;
    ensure!(fs::symlink_metadata(parent)?.is_dir(), "保存位置不可用");
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if replace {
        temporary.persist(path)?;
    } else {
        temporary.persist_noclobber(path)?;
    }
    Ok(())
}

fn ensure_path(directory: &Path, path: &Path, create: bool) -> Result<()> {
    let relative = path
        .strip_prefix(directory)
        .context("保存路径不在导入目录内")?;
    let mut current = directory.to_path_buf();
    if create {
        fs::create_dir_all(directory)?;
    }
    match fs::symlink_metadata(directory) {
        Ok(metadata) => ensure!(metadata.is_dir(), "保存位置不可用"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !create => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    for part in relative.components() {
        let std::path::Component::Normal(part) = part else {
            anyhow::bail!("保存路径不在导入目录内");
        };
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => ensure!(metadata.is_dir(), "保存位置不可用"),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && create => {
                fs::create_dir(&current)?
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

async fn sync_item(
    store: &Store,
    source: &Source,
    remote: &remote::Remote,
    directory: &Path,
    item: &remote::Item,
) -> Result<Outcome> {
    let tracked = tracked_item(store, source, item)?;
    if let Some((version, previous)) = &tracked {
        let main = previous.first().context("导入记录缺少文件")?;
        ensure_path(
            directory,
            Path::new(&main.path).parent().context("保存路径无效")?,
            false,
        )?;
        if file_hash(Path::new(&main.path))?.is_none() {
            return Ok(Outcome::Conflict("本地副本已删除，请先恢复文件".into()));
        }
        if !item.version.is_empty() && version == &item.version {
            return Ok(Outcome::Skipped);
        }
    }
    let content = remote.fetch(item).await?;
    install_content(store, source, directory, item, tracked.map(|(_, files)| files), content)
}

fn tracked_item(store: &Store, source: &Source, item: &remote::Item) -> Result<Option<(String, Vec<FileRecord>)>> {
    let row = store.connection().query_row(
        "SELECT version,files FROM online_items WHERE source=?1 AND id=?2",
        params![source.id, item.id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    ).optional()?;
    row.map(|(version, files)| Ok((version, serde_json::from_str(&files)?))).transpose()
}

fn install_content(
    store: &Store,
    source: &Source,
    directory: &Path,
    item: &remote::Item,
    previous: Option<Vec<FileRecord>>,
    mut content: remote::Content,
) -> Result<Outcome> {
    let key = digest(item.id.as_bytes());
    let asset_folder = format!("{key}-assets");
    if !content.assets.is_empty() {
        content.bytes = String::from_utf8(content.bytes)?
            .replace("SAG_ASSETS/", &format!("{asset_folder}/"))
            .into_bytes();
    }
    let hash = digest(&content.bytes);
    let path = previous
        .as_ref()
        .and_then(|files| files.first())
        .map(|file| PathBuf::from(&file.path))
        .unwrap_or_else(|| {
            item.folders
                .iter()
                .take(12)
                .fold(directory.to_path_buf(), |path, folder| {
                    path.join(name_part(folder))
                })
                .join(format!(
                    "{}-{}.{}",
                    name_part(&item.title),
                    &key[..12],
                    content.extension
                ))
        });
    let parent = path.parent().context("保存路径无效")?;
    ensure_path(directory, parent, false)?;
    let actual = file_hash(&path)?;
    if let Some(previous) = &previous {
        if actual.as_deref() != Some(&previous[0].hash) && actual.as_deref() != Some(&hash) {
            return Ok(Outcome::Conflict("本地文件已修改，已保留原文件".into()));
        }
        for file in previous.iter().skip(1) {
            ensure_path(
                directory,
                Path::new(&file.path).parent().context("附件路径无效")?,
                false,
            )?;
            if file_hash(Path::new(&file.path))?.as_deref() != Some(&file.hash) {
                return Ok(Outcome::Conflict(
                    "本地附件已修改或删除，已保留原文件".into(),
                ));
            }
        }
    } else if actual.is_some() && actual.as_deref() != Some(&hash) {
        return Ok(Outcome::Conflict("保存位置已有同名文件".into()));
    }
    ensure_path(directory, parent, true)?;
    let mut files = vec![FileRecord {
        path: path.to_string_lossy().into_owned(),
        hash: hash.clone(),
    }];
    if !content.assets.is_empty() {
        let folder = parent.join(asset_folder);
        ensure_path(directory, &folder, true)?;
        for (name, bytes) in content.assets {
            let path = folder.join(name);
            let hash = digest(&bytes);
            match file_hash(&path)? {
                Some(existing) if existing == hash => {}
                Some(_) => return Ok(Outcome::Conflict("本地附件已修改，已保留原文件".into())),
                None => save_file(&path, &bytes, false)?,
            }
            files.push(FileRecord {
                path: path.to_string_lossy().into_owned(),
                hash,
            });
        }
    }
    if actual.as_deref() != Some(&hash) {
        if file_hash(&path)? != actual {
            return Ok(Outcome::Conflict("本地文件已修改，已保留原文件".into()));
        }
        save_file(&path, &content.bytes, actual.is_some())?;
    }
    store.connection().execute("INSERT INTO online_items(source,id,version,files) VALUES(?1,?2,?3,?4) ON CONFLICT(source,id) DO UPDATE SET version=excluded.version,files=excluded.files",params![source.id,item.id,item.version,serde_json::to_string(&files)?])?;
    if previous.is_some() && actual.as_deref() == Some(&hash) {
        Ok(Outcome::Skipped)
    } else {
        Ok(Outcome::Written {
            path,
            updated: previous.is_some(),
        })
    }
}

pub(crate) async fn schedule(service: Arc<Service>) {
    let recovery = (|| -> Result<()> {
        let db = service.store.connection();
        let mut query = db.prepare("SELECT id,result FROM online_runs WHERE finished IS NULL")?;
        let rows = query.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, result) = row?;
            let mut result: Report = serde_json::from_str(&result)?;
            result.status = "interrupted".into();
            db.execute(
                "UPDATE online_runs SET finished=?2,result=?3 WHERE id=?1",
                params![id, now(), serde_json::to_string(&result)?],
            )?;
        }
        Ok(())
    })();
    if let Err(error) = recovery {
        eprintln!("恢复在线同步记录失败：{error:#}");
    }
    let mut timer = tokio::time::interval(Duration::from_secs(15));
    loop {
        tokio::select! {biased;_=service.stop.cancelled()=>break,_=timer.tick()=>{}}
        let due = (|| -> Result<Vec<String>> {
            let db = service.store.connection();
            let mut query =
                db.prepare("SELECT id FROM online_sources WHERE next_at<=?1 ORDER BY next_at")?;
            let rows = query.query_map([now()], |row| row.get::<_, String>(0))?;
            Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
        })();
        match due {
            Ok(due) => {
                for id in due {
                    if service.online_import.active.lock().unwrap().len() >= 2 {
                        break;
                    }
                    if let Err(error) = start(&service, &id, false, true) {
                        eprintln!("启动定时同步失败：{error:#}");
                    }
                }
            }
            Err(error) => eprintln!("读取同步计划失败：{error:#}"),
        }
    }
}
