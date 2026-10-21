use crate::{sag_mcp::SagTools, types::{McpCredentials, McpServer, McpToolInfo}};
use anyhow::{bail, Context, Result};
use rmcp::{
    model::CallToolRequestParams,
    service::RunningService,
    transport::{
        streamable_http_client::StreamableHttpClientTransportConfig, StreamableHttpClientTransport,
        TokioChildProcess,
    },
    RoleClient, RoleServer, ServiceExt,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

type Client = RunningService<RoleClient, ()>;
#[derive(Clone, Default)]
pub enum StdioRuntime {
    #[default]
    Local,
    Fnos { socket: std::path::PathBuf, uid: u32 },
}

#[derive(Clone)]
pub struct ExternalTool {
    pub server_name: String,
    pub name: String,
    pub require_confirmation: bool,
    pub builtin: bool,
    client_index: usize,
}
pub struct McpSession {
    clients: Vec<Client>,
    pub definitions: Vec<Value>,
    routes: HashMap<String, ExternalTool>,
    timeout: u64,
    builtin: Option<RunningService<RoleServer, SagTools>>,
}
pub fn validate(server: &McpServer) -> Result<()> {
    if server.id.is_empty() || server.name.trim().is_empty() {
        bail!("请填写 MCP 服务名称")
    }
    match server.transport.as_str() {
        "http" => {
            let url = reqwest::Url::parse(&server.url).context("MCP 地址无效")?;
            if !matches!(url.scheme(), "http" | "https") {
                bail!("MCP 地址必须使用 HTTP 或 HTTPS")
            }
        }
        "stdio" => {
            if server.command.trim().is_empty() {
                bail!("请填写 MCP 启动命令")
            }
        }
        _ => bail!("不支持的 MCP 连接方式"),
    }
    Ok(())
}
async fn connect(
    server: &McpServer,
    secrets: &McpCredentials,
    http: &reqwest::Client,
    timeout: u64,
    cancel: &CancellationToken,
    runtime: &StdioRuntime,
) -> Result<Client> {
    validate(server)?;
    let future = async {
        if server.transport == "stdio" {
            match runtime {
                StdioRuntime::Local => {
                    let mut command = tokio::process::Command::new(&server.command);
                    command.args(&server.args).envs(&secrets.env);
                    Ok(().serve(TokioChildProcess::new(command)?).await?)
                }
                StdioRuntime::Fnos { socket, uid } => {
                    #[cfg(unix)] {
                        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                        let mut stream = tokio::net::UnixStream::connect(socket).await.context("无法连接飞牛本地命令服务")?;
                        let request = sag_protocol::McpProcessRequest { uid: *uid, command: server.command.clone(), args: server.args.clone(), env: secrets.env.clone() };
                        stream.write_all(&serde_json::to_vec(&request)?).await?;
                        stream.write_all(b"\n").await?;
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();
                        reader.read_line(&mut line).await?;
                        let reply: sag_protocol::McpProcessReply = serde_json::from_str(&line).context("本地命令服务返回了无效响应")?;
                        if let Some(error) = reply.error { bail!("{error}"); }
                        Ok(().serve(reader).await.with_context(|| format!("MCP 启动失败；错误日志：{}", reply.stderr_path.as_deref().unwrap_or("未生成")))?)
                    }
                    #[cfg(not(unix))] { let _ = (socket, uid); bail!("飞牛本地命令服务需要 Unix 系统") }
                }
            }
        } else {
            let mut config = StreamableHttpClientTransportConfig::with_uri(server.url.clone());
            for (name, value) in &secrets.headers {
                config.custom_headers.insert(
                    name.parse().context("MCP 请求头名称无效")?,
                    value.parse().context("MCP 请求头值无效")?,
                );
            }
            Ok(()
                .serve(StreamableHttpClientTransport::with_client(
                    http.clone(),
                    config,
                ))
                .await?)
        }
    };
    tokio::select! {
        _=cancel.cancelled()=>bail!("已停止"),
        result=tokio::time::timeout(Duration::from_secs(timeout),future)=>result.context("MCP 连接超时")?,
    }
}
impl McpSession {
    pub async fn open(
        servers: Vec<(McpServer, McpCredentials)>,
        http: &reqwest::Client,
        timeout: u64,
        cancel: &CancellationToken,
        selections: &BTreeMap<String, Vec<String>>,
    ) -> Result<Self> {
        Self::open_with_runtime(servers, http, timeout, cancel, selections, &StdioRuntime::Local).await
    }
    pub async fn open_with_runtime(
        servers: Vec<(McpServer, McpCredentials)>,
        http: &reqwest::Client,
        timeout: u64,
        cancel: &CancellationToken,
        selections: &BTreeMap<String, Vec<String>>,
        runtime: &StdioRuntime,
    ) -> Result<Self> {
        let mut session = Self {
            clients: vec![],
            definitions: vec![],
            routes: HashMap::new(),
            timeout,
            builtin: None,
        };
        for (server, secrets) in servers {
            let allowed = selections.get(&server.id);
            if allowed.is_some_and(Vec::is_empty) {
                continue;
            }
            let client = connect(&server, &secrets, http, timeout, cancel, runtime)
                .await
                .with_context(|| format!("无法连接 MCP 服务 {}", server.name))?;
            let tools = tokio::select! {
                _=cancel.cancelled()=>bail!("已停止"),
                result=tokio::time::timeout(Duration::from_secs(timeout),client.list_all_tools())=>result.context("获取 MCP 工具列表超时")??,
            };
            let client_index = session.clients.len();
            for (index, tool) in tools.into_iter().enumerate() {
                if allowed.is_some_and(|names| !names.iter().any(|name| name == tool.name.as_ref()))
                {
                    continue;
                }
                let name = format!("mcp_{client_index}_{index}");
                session.definitions.push(json!({"type":"function","function":{"name":name,"description":format!("{} / {}: {}",server.name,tool.name,tool.description.as_deref().unwrap_or("")),"parameters":tool.input_schema}}));
                session.routes.insert(
                    name,
                    ExternalTool {
                        server_name: server.name.clone(),
                        name: tool.name.to_string(),
                        require_confirmation: server.require_confirmation,
                        builtin: false,
                        client_index,
                    },
                );
            }
            session.clients.push(client);
        }
        Ok(session)
    }
    pub async fn attach_sag(&mut self, tools: SagTools) -> Result<()> {
        let (server_io, client_io) = tokio::io::duplex(65536);
        let (server, client) = tokio::join!(tools.serve(server_io), ().serve(client_io));
        self.builtin = Some(server.context("无法启动内置 SAG MCP")?);
        let client = client.context("无法连接内置 SAG MCP")?;
        let client_index = self.clients.len();
        for tool in client.list_all_tools().await? {
            let name = format!("sag2_{}", tool.name);
            self.definitions.push(json!({"type":"function","function":{"name":name,"description":tool.description,"parameters":tool.input_schema}}));
            self.routes.insert(name, ExternalTool { server_name:"SAG".into(), name:tool.name.into(), require_confirmation:false, builtin:true, client_index });
        }
        self.clients.push(client);
        Ok(())
    }
    pub fn tool(&self, name: &str) -> Option<&ExternalTool> {
        self.routes.get(name)
    }
    pub async fn call(
        &self,
        name: &str,
        arguments: Value,
        cancel: &CancellationToken,
    ) -> Result<Value> {
        let tool = self
            .routes
            .get(name)
            .context("模型请求了未绑定的 MCP 工具")?;
        let arguments = arguments
            .as_object()
            .context("MCP 工具参数必须是 JSON 对象")?
            .clone();
        let request = CallToolRequestParams::new(tool.name.clone()).with_arguments(arguments);
        let result = tokio::select! {
            _=cancel.cancelled()=>bail!("已停止"),
            result=tokio::time::timeout(Duration::from_secs(self.timeout),self.clients[tool.client_index].call_tool(request))=>result.context("MCP 工具调用超时")??,
        };
        Ok(serde_json::to_value(result)?)
    }
}
pub async fn test(
    server: &McpServer,
    secrets: &McpCredentials,
    http: &reqwest::Client,
    timeout: u64,
) -> Result<Vec<McpToolInfo>> {
    test_with_runtime(server, secrets, http, timeout, &StdioRuntime::Local).await
}
pub async fn test_with_runtime(
    server: &McpServer,
    secrets: &McpCredentials,
    http: &reqwest::Client,
    timeout: u64,
    runtime: &StdioRuntime,
) -> Result<Vec<McpToolInfo>> {
    let client = connect(server, secrets, http, timeout, &CancellationToken::new(), runtime).await?;
    let tools = tokio::time::timeout(Duration::from_secs(timeout), client.list_all_tools())
        .await
        .context("获取 MCP 工具列表超时")??;
    Ok(tools
        .into_iter()
        .map(|t| McpToolInfo {
            name: t.name.to_string(),
            description: t.description.map(|v| v.into_owned()).unwrap_or_default(),
        })
        .collect())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn local_server() -> McpServer {
        McpServer {
            id: "test".into(),
            name: "测试 MCP".into(),
            transport: "stdio".into(),
            command: "/usr/bin/python3".into(),
            args: vec![format!(
                "{}/../tests/mock_mcp.py",
                env!("CARGO_MANIFEST_DIR")
            )],
            ..Default::default()
        }
    }
    #[tokio::test]
    async fn stdio_discovery_arguments_credentials_and_error_results() {
        let mut credentials = McpCredentials::default();
        credentials
            .env
            .insert("MCP_TEST_SUFFIX".into(), "-from-env".into());
        let token = CancellationToken::new();
        let session = McpSession::open(
            vec![(local_server(), credentials)],
            &reqwest::Client::new(),
            5,
            &token,
            &Default::default(),
        )
        .await
        .unwrap();
        assert_eq!(session.definitions.len(), 2);
        assert_eq!(session.tool("mcp_0_0").unwrap().name, "echo");
        let result = session
            .call("mcp_0_0", json!({"text":"hello","fail":true}), &token)
            .await
            .unwrap();
        assert_eq!(result["content"][0]["text"], "hello-from-env");
        assert_eq!(result["isError"], true);
        assert!(session.call("unbound", json!({}), &token).await.is_err());
        assert!(session.call("mcp_0_0", json!([]), &token).await.is_err());
    }
    #[tokio::test]
    async fn only_selected_tools_are_exposed_and_callable() {
        let token = CancellationToken::new();
        let http = reqwest::Client::new();
        let selected = BTreeMap::from([("test".into(), vec!["reverse".into()])]);
        let session = McpSession::open(
            vec![(local_server(), Default::default())],
            &http,
            5,
            &token,
            &selected,
        )
        .await
        .unwrap();
        assert_eq!(session.definitions.len(), 1);
        assert!(session.tool("mcp_0_0").is_none());
        assert_eq!(session.tool("mcp_0_1").unwrap().name, "reverse");
        assert!(session
            .call("mcp_0_0", json!({"text":"disabled"}), &token)
            .await
            .is_err());
        let result = session
            .call("mcp_0_1", json!({"text":"abc"}), &token)
            .await
            .unwrap();
        assert_eq!(result["content"][0]["text"], "cba");
        let mut disabled = local_server();
        disabled.command = "/does/not/exist".into();
        let selected = BTreeMap::from([("test".into(), vec![])]);
        let session = McpSession::open(
            vec![(disabled, Default::default())],
            &http,
            5,
            &token,
            &selected,
        )
        .await
        .unwrap();
        assert!(session.definitions.is_empty());
    }
    #[tokio::test]
    async fn slow_mcp_tool_can_be_cancelled() {
        let token = CancellationToken::new();
        let session = McpSession::open(
            vec![(local_server(), McpCredentials::default())],
            &reqwest::Client::new(),
            5,
            &token,
            &Default::default(),
        )
        .await
        .unwrap();
        let cancellation = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancellation.cancel();
        });
        let result = session
            .call("mcp_0_0", json!({"text":"slow","delay":2}), &token)
            .await;
        assert!(result.unwrap_err().to_string().contains("已停止"));
    }
    #[tokio::test]
    #[ignore = "Requires the public DeepWiki MCP service"]
    async fn deepwiki_http_connection_and_real_tool() {
        let server = McpServer {
            id: "deepwiki".into(),
            name: "DeepWiki".into(),
            url: "https://mcp.deepwiki.com/mcp".into(),
            require_confirmation: false,
            ..Default::default()
        };
        let token = CancellationToken::new();
        let session = McpSession::open(
            vec![(server, McpCredentials::default())],
            &reqwest::Client::new(),
            90,
            &token,
            &Default::default(),
        )
        .await
        .unwrap();
        assert!(session.definitions.len() >= 3);
        let (name, _) = session
            .routes
            .iter()
            .find(|(_, t)| t.name == "read_wiki_structure")
            .unwrap();
        let result = session
            .call(
                name,
                json!({"repoName":"modelcontextprotocol/rust-sdk"}),
                &token,
            )
            .await
            .unwrap();
        assert_ne!(result["isError"], true);
        assert!(result["content"][0]["text"].as_str().unwrap().len() > 20);
        println!(
            "DeepWiki verified: {} tools; public repository structure returned",
            session.definitions.len()
        );
    }
}
