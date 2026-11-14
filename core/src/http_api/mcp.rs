use super::{folders, run_search, ApiError, HttpResponse, SearchRequest};
use crate::service::Service;
use hyper::{body::Incoming, Request, StatusCode};
use rmcp::{
    model::{
        CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
        PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
    },
    service::{RequestContext, RoleServer},
    transport::streamable_http_server::{
        session::never::NeverSessionManager, StreamableHttpServerConfig, StreamableHttpService,
    },
    ErrorData, ServerHandler,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
struct ApiTools {
    service: Arc<Service>,
    cancel: CancellationToken,
    started: Instant,
    id: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FolderArgs {
    path: Option<String>,
}

fn tools() -> [Tool; 2] {
    [
        Tool::new("search", "Search indexed documents and return original passages. Deep search uses the configured decision model; fast search uses the local index.", json!({
            "type":"object", "required":["query"], "additionalProperties":false,
            "properties":{
                "query":{"type":"string","minLength":1,"maxLength":2000},
                "mode":{"type":"string","enum":["deep","fast"],"default":"deep"},
                "paths":{"type":"array","items":{"type":"string"},"description":"Absolute paths on the server, within indexed folders."},
                "exclude":{"type":"array","items":{"type":"string"},"description":"Glob exclusions relative to each selected directory, e.g. [\".git/**\",\"archive/\",\"*.zip\"]. Applied before retrieval and decision scoring; does not modify the index."},
                "limit":{"type":"integer","minimum":1,"maximum":100,"default":10}
            }
        }).as_object().unwrap().clone()),
        Tool::new("folders", "List knowledge-base roots or the immediate subfolders of an indexed directory.", json!({
            "type":"object", "additionalProperties":false,
            "properties":{"path":{"type":"string","description":"Absolute directory path on the server. Omit to list roots."}}
        }).as_object().unwrap().clone()),
    ].map(|mut tool| {
        let mut annotations = ToolAnnotations::default();
        annotations.read_only_hint = Some(true);
        tool.annotations = Some(annotations);
        tool
    })
}

impl ServerHandler for ApiTools {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info = Implementation::new("sag-wiki", sag_protocol::BUILD_VERSION);
        info
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        tools().into_iter().find(|tool| tool.name == name)
    }

    async fn list_tools(
        &self,
        _: Option<PaginatedRequestParams>,
        _: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: tools().into(),
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let args = Value::Object(request.arguments.unwrap_or_default());
        let result = match request.name.as_ref() {
            "search" => {
                let input = serde_json::from_value::<SearchRequest>(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                run_search(
                    self.service.clone(),
                    input,
                    self.cancel.clone(),
                    self.started,
                    self.id.clone(),
                )
                .await
            }
            "folders" => {
                let input = serde_json::from_value::<FolderArgs>(args)
                    .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;
                let store = self.service.store.clone();
                tokio::task::spawn_blocking(move || folders(&store, input.path.as_deref()))
                    .await
                    .map_err(|e| ErrorData::internal_error(e.to_string(), None))?
            }
            _ => return Err(ErrorData::invalid_params("Unknown tool", None)),
        };
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(e) => CallToolResult::structured_error(json!({"code":e.code,"message":e.message})),
        }
        .into())
    }
}

pub(super) async fn handle(
    request: Request<Incoming>,
    service: Arc<Service>,
    cancel: CancellationToken,
    started: Instant,
    id: &str,
    local: SocketAddr,
) -> Result<HttpResponse, ApiError> {
    let mut hosts = vec![local.to_string()];
    if local.ip().is_loopback() {
        hosts.push(format!("localhost:{}", local.port()));
    }
    let handler = ApiTools {
        service,
        cancel: cancel.clone(),
        started,
        id: id.into(),
    };
    let config = StreamableHttpServerConfig::default()
        .with_legacy_session_mode(false)
        .with_json_response(true)
        .with_cancellation_token(cancel)
        .with_allowed_hosts(hosts)
        .enforce_origin_validation()
        .with_max_request_body_bytes(sag_protocol::MAX_REQUEST as usize);
    let transport = StreamableHttpService::new(
        move || Ok(handler.clone()),
        Arc::new(NeverSessionManager::default()),
        config,
    );
    let mut response =
        tokio::time::timeout_at(started + Duration::from_secs(65), transport.handle(request))
            .await
            .map_err(|_| {
                ApiError::new(
                    StatusCode::REQUEST_TIMEOUT,
                    "request_timeout",
                    "MCP 请求超时",
                )
            })?;
    response.headers_mut().insert(
        hyper::header::CACHE_CONTROL,
        hyper::header::HeaderValue::from_static("no-store"),
    );
    Ok(response)
}
