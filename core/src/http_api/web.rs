use super::{response, ApiError, ApiService, HttpResponse};
use crate::service::{EventSink, Service};
use http_body_util::{BodyExt, Full, Limited, StreamBody};
use hyper::{
    body::{Bytes, Frame, Incoming},
    header, Method, Request, Response, StatusCode,
};
use sag_protocol::{Operation as Op, Output, ServiceEvent};
use serde_json::{json, Value};
use std::{
    convert::Infallible,
    net::{IpAddr, SocketAddr},
    sync::{Arc, Weak},
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio_util::sync::CancellationToken;

const ASSETS: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/web-assets.rs"));

pub(super) fn available() -> bool {
    !ASSETS.is_empty()
}

pub(super) fn owns(path: &str) -> bool {
    matches!(
        path,
        "/" | "/index.html" | "/favicon.png" | "/web/session" | "/web/operation" | "/web/events"
    ) || path.starts_with("/assets/")
        || path.starts_with("/textfold/")
}

pub(super) async fn handle(
    request: Request<Incoming>,
    service: Weak<Service>,
    api: &ApiService,
    stop: CancellationToken,
    disconnected: CancellationToken,
    local: SocketAddr,
    id: &str,
) -> HttpResponse {
    match route(request, service, api, stop, disconnected, local).await {
        Ok(result) => result,
        Err(error) => error.response(id),
    }
}

async fn route(
    request: Request<Incoming>,
    service: Weak<Service>,
    api: &ApiService,
    stop: CancellationToken,
    disconnected: CancellationToken,
    local: SocketAddr,
) -> Result<HttpResponse, ApiError> {
    let missing = || {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            "网页服务未开启或页面不存在",
        )
    };
    let (authorized, closed) = {
        let config = api.config.lock().unwrap();
        let code = config.web_code.as_deref().ok_or_else(missing)?;
        let value = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok());
        let authorized = value
            .and_then(|v| v.strip_prefix("Bearer "))
            .is_some_and(|token| bool::from(token.as_bytes().ct_eq(code.as_bytes())));
        (authorized, config.web_cancel.clone())
    };
    // A remote site cannot rebind its own domain to this listener and inherit its origin.
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::bad("缺少网页服务地址"))?;
    let authority: hyper::http::uri::Authority = host.parse().map_err(ApiError::bad)?;
    let hostname = authority.host().trim_matches(['[', ']']);
    let host_matches = hostname.parse::<IpAddr>().is_ok_and(|ip| ip == local.ip())
        || (hostname.eq_ignore_ascii_case("localhost") && local.ip().is_loopback());
    if !host_matches || authority.port_u16().unwrap_or(80) != local.port() {
        return Err(ApiError::new(
            StatusCode::FORBIDDEN,
            "invalid_host",
            "请使用设置中显示的网页地址",
        ));
    }
    if let Some(origin) = request.headers().get(header::ORIGIN) {
        if origin.as_bytes() != format!("http://{host}").as_bytes() {
            return Err(ApiError::new(
                StatusCode::FORBIDDEN,
                "invalid_origin",
                "网页请求来源不受支持",
            ));
        }
    }
    let path = request.uri().path();
    if !path.starts_with("/web/") {
        if request.method() != Method::GET {
            return Err(missing());
        }
        let path = if path == "/" { "/index.html" } else { path };
        let (_, bytes) = ASSETS
            .iter()
            .find(|(route, _)| *route == path)
            .ok_or_else(missing)?;
        let bytes = if path == "/index.html" {
            let html = std::str::from_utf8(bytes).expect("Vite outputs UTF-8 HTML");
            Bytes::from(html.replacen("<html ", "<html data-sag-web ", 1))
        } else {
            Bytes::from_static(bytes)
        };
        let content_type = match path.rsplit_once('.').map(|(_, ext)| ext) {
            Some("html") => "text/html; charset=utf-8",
            Some("js") => "text/javascript; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("png") => "image/png",
            Some("svg") => "image/svg+xml",
            Some("woff2") => "font/woff2",
            Some("ico") => "image/x-icon",
            _ => "application/octet-stream",
        };
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, content_type)
            .header(header::CACHE_CONTROL, "no-cache")
            .header("x-content-type-options", "nosniff")
            .header("referrer-policy", "no-referrer")
            .header("content-security-policy", "default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; worker-src 'self' blob:; object-src 'none'; base-uri 'none'; form-action 'none'; frame-ancestors 'self'")
            .body(Full::new(bytes).boxed()).unwrap());
    }
    if !authorized || closed.is_cancelled() {
        return Err(ApiError::unauthorized());
    }
    if path == "/web/session" && request.method() == Method::GET {
        return Ok(response(
            StatusCode::OK,
            json!({"version":sag_protocol::BUILD_VERSION}),
        ));
    }
    let service = service.upgrade().ok_or_else(ApiError::stopped)?;
    let tab = request
        .headers()
        .get("x-sag-tab")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| uuid::Uuid::parse_str(v).ok())
        .ok_or_else(|| ApiError::bad("网页会话无效"))?;
    let owner = format!("web:{tab}");
    if path == "/web/events" && request.method() == Method::GET {
        let rx = service.events.subscribe();
        let frames = futures_util::stream::unfold(
            (rx, owner, stop, closed, true),
            |(mut rx, owner, stop, closed, initial)| async move {
                let data = if initial {
                    ": connected\n\n".into()
                } else {
                    loop {
                        let event = tokio::select! {
                            _ = stop.cancelled() => return None,
                            _ = closed.cancelled() => return None,
                            _ = tokio::time::sleep(Duration::from_secs(15)) => break ": heartbeat\n\n".into(),
                            event = rx.recv() => event,
                        };
                        match event {
                            Ok((target, envelope))
                                if target.as_ref().is_none_or(|value| value == &owner) =>
                            {
                                break format!(
                                    "data: {}\n\n",
                                    serde_json::to_string(&envelope.event).unwrap()
                                );
                            }
                            Ok(_) => {}
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                                break format!(
                                    "data: {}\n\n",
                                    serde_json::to_string(&ServiceEvent::DataChanged).unwrap()
                                );
                            }
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                };
                Some((
                    Ok::<_, Infallible>(Frame::data(Bytes::from(data))),
                    (rx, owner, stop, closed, false),
                ))
            },
        );
        return Ok(Response::builder()
            .header(header::CONTENT_TYPE, "text/event-stream")
            .header(header::CACHE_CONTROL, "no-store")
            .body(StreamBody::new(frames).boxed())
            .unwrap());
    }
    if path != "/web/operation" || request.method() != Method::POST {
        return Err(ApiError::new(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "HTTP 方法不受支持",
        ));
    }
    if request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        != Some("application/json")
    {
        return Err(ApiError::bad("需要 application/json"));
    }
    let bytes = tokio::time::timeout(
        Duration::from_secs(10),
        Limited::new(request.into_body(), 24 * 1024 * 1024).collect(),
    )
    .await
    .map_err(ApiError::bad)?
    .map_err(ApiError::bad)?
    .to_bytes();
    let input: WebOperation = serde_json::from_slice(&bytes).map_err(ApiError::bad)?;
    let cancel = closed.child_token();
    let _guard = cancel.clone().drop_guard();
    let sink = EventSink {
        owner,
        request_id: None,
        cancel: cancel.clone(),
        bus: service.events.clone(),
    };
    let mut task = tokio::spawn(async move {
        match input {
            WebOperation::Appearance {
                theme,
                language,
                start_page,
                editor_size,
                model_profiles,
            } => {
                let mut settings = service.store.settings().map_err(|e| e.to_string())?;
                settings.theme = theme;
                settings.language = language;
                settings.start_page = start_page;
                settings.editor_size = editor_size;
                // A browser can change reasoning effort, not the host's tools or model endpoints.
                for (model, profile) in model_profiles {
                    if settings.answer.models.contains(&model) || settings.answer.model == model {
                        settings
                            .model_profiles
                            .entry(model)
                            .or_default()
                            .reasoning_effort = profile.reasoning_effort;
                    }
                }
                service
                    .store
                    .save_settings(&settings)
                    .map_err(|e| e.to_string())?;
                let _ = service.events.send((
                    None,
                    sag_protocol::EventEnvelope {
                        request_id: None,
                        event: ServiceEvent::SettingsChanged,
                    },
                ));
                Ok(Output::Unit)
            }
            WebOperation::Service(operation) => {
                if !allowed(&operation) {
                    return Err("此操作需在桌面端完成".into());
                }
                if let Op::ReadDocument { path } = &operation {
                    let path = service
                        .store
                        .authorize(std::path::Path::new(path))
                        .map_err(|e| e.to_string())?;
                    if !std::fs::metadata(&path)
                        .map_err(|e| e.to_string())?
                        .is_file()
                        || !(crate::text::is_supported(&path) || crate::docling::is_source(&path))
                    {
                        return Err("此文件不支持网页阅读".into());
                    }
                }
                let permit = if matches!(
                    operation,
                    Op::RunQuery { .. }
                        | Op::SummarizeSearch { .. }
                        | Op::FoldDocument { .. }
                        | Op::ExploreGraph { .. }
                        | Op::ExploreNeighbors { .. }
                ) {
                    Some(tokio::select! {
                        _ = sink.cancel.cancelled() => return Err("已停止".into()),
                        permit = service.requests.acquire() => permit.map_err(|e| e.to_string())?,
                    })
                } else {
                    None
                };
                let result = service.handle(operation, &sink).await;
                drop(permit);
                result
            }
        }
    });
    let result = tokio::select! {
        _ = closed.cancelled() => return Err(ApiError::stopped()),
        _ = stop.cancelled() => return Err(ApiError::stopped()),
        _ = disconnected.cancelled() => return Err(ApiError::stopped()),
        result = &mut task => result.map_err(ApiError::internal)?.map_err(ApiError::bad)?,
    };
    if let Output::Settings(mut settings) = result {
        settings.mcp_servers.clear();
        for profile in settings.model_profiles.values_mut() {
            profile.mcp_servers.clear();
            profile.mcp_tools.clear();
        }
        return Ok(response(
            StatusCode::OK,
            serde_json::to_value(settings).map_err(ApiError::internal)?,
        ));
    }
    let value = serde_json::to_value(result).map_err(ApiError::internal)?;
    Ok(response(
        StatusCode::OK,
        value.get("data").cloned().unwrap_or(Value::Null),
    ))
}

#[derive(serde::Deserialize)]
#[serde(untagged)]
enum WebOperation {
    Service(Op),
    Appearance {
        theme: String,
        language: String,
        start_page: String,
        editor_size: usize,
        model_profiles: std::collections::BTreeMap<String, sag_protocol::types::ModelProfile>,
    },
}

fn allowed(op: &Op) -> bool {
    matches!(
        op,
        Op::ListRoots
            | Op::ListEntries { .. }
            | Op::ListFiles
            | Op::ReadDocument { .. }
            | Op::SaveDocument { .. }
            | Op::GetSettings
            | Op::LoadImage { .. }
            | Op::ListConversations
            | Op::EditConversation { .. }
            | Op::ReplaceMessages { .. }
            | Op::RunQuery { .. }
            | Op::SummarizeSearch { .. }
            | Op::CancelQuery { .. }
            | Op::FoldDocument { .. }
            | Op::PutDraft { .. }
            | Op::ListDrafts
            | Op::DeleteDraft { .. }
            | Op::ListNotes { .. }
            | Op::GetNote { .. }
            | Op::VerifyNotes { .. }
            | Op::SaveNote {
                target: Some(_),
                ..
            }
            | Op::ExploreOverview { .. }
            | Op::ExploreGraph { .. }
            | Op::ExploreNeighbors { .. }
            | Op::ExploreEvidence { .. }
            | Op::GetIndexStatus
            | Op::GetIndexHistory
            | Op::GetIndexHistoryLog { .. }
            | Op::SetIndexPaused { .. }
            | Op::RetryFailedIndexes
            | Op::RefreshRoot { .. }
            | Op::ReindexAll
            | Op::RemovedDirectories
            | Op::GetFolderIndexOptions { .. }
            | Op::SaveFolderIndexOptions { .. }
            | Op::ClearIndexHistory
    )
}
