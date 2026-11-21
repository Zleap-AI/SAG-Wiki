use anyhow::{bail, Result};
use clap::Subcommand;
use sag_protocol::{ApiAction, ApiScope, Operation, Output};
use serde_json::{json, Value};
use std::time::Duration;

#[derive(Subcommand)]
pub enum ApiCommand {
    /// 开启本机和局域网 HTTP API；即使没有 Key 也可开启。
    Start {
        #[arg(long)]
        host: Option<String>,
        #[arg(long,value_parser=clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    /// 仅关闭 HTTP API，保留后台、索引和普通 CLI。
    Stop,
    /// 管理浏览器使用的网页服务。
    Web {
        #[command(subcommand)]
        action: WebCommand,
    },
    /// 查看 API 状态及 HTTP、MCP 地址，不自动启动后台。
    Status {
        #[arg(long)]
        json: bool,
    },
    /// 管理调用 API 所用的 Key。
    Key {
        #[command(subcommand)]
        action: KeyCommand,
    },
    /// 导出 MCP 客户端配置。
    Mcp {
        #[command(subcommand)]
        action: McpCommand,
    },
    /// 配置后台自启，不改变 API 启用状态。
    Autostart {
        #[command(subcommand)]
        action: AutostartCommand,
    },
}
#[derive(Subcommand)]
pub enum KeyCommand {
    Create {
        #[arg(long)]
        name: String,
        /// 允许访问的完整目录路径，可重复传入；省略时保持全库权限。
        #[arg(long = "path", conflicts_with = "all")]
        paths: Vec<String>,
        /// 显式允许访问全部知识库，包括以后新增的目录。
        #[arg(long, conflicts_with = "paths")]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    /// 修改指定 Key 的目录范围，并取消该 Key 的在途请求。
    SetScope {
        id: String,
        #[arg(long = "path", required_unless_present = "all", conflicts_with = "all")]
        paths: Vec<String>,
        #[arg(long, conflicts_with = "paths")]
        all: bool,
        #[arg(long)]
        json: bool,
    },
    List {
        #[arg(long)]
        json: bool,
    },
    Revoke {
        id: String,
    },
}
#[derive(Subcommand)]
pub enum McpCommand {
    /// 输出包含 API Key 的 MCP JSON 配置。
    Config {
        /// status 中列出的 MCP 地址，默认使用本机地址。
        #[arg(long)]
        address: Option<String>,
        /// key list 中的 Key ID，默认使用最近创建的可复制 Key。
        #[arg(long)]
        key_id: Option<String>,
    },
}
#[derive(Subcommand)]
pub enum AutostartCommand {
    Enable,
    Disable,
    Status,
}
#[derive(Subcommand)]
pub enum WebCommand {
    Start {
        #[arg(long)]
        host: Option<String>,
        #[arg(long,value_parser=clap::value_parser!(u16).range(1..))]
        port: Option<u16>,
    },
    Stop,
    Code,
    /// 设置网页访问码：off 允许能连接网页地址的任何人直接进入。
    AccessCode {
        #[arg(value_enum)]
        mode: WebAccessCode,
    },
    /// 保存网页外部访问地址；使用 --clear 清除。
    ExternalUrl {
        #[arg(required_unless_present = "clear", conflicts_with = "clear")]
        url: Option<String>,
        #[arg(long)]
        clear: bool,
    },
}

#[derive(Clone, clap::ValueEnum)]
pub enum WebAccessCode {
    On,
    Off,
}

async fn call(client: &sag_client::Client, action: ApiAction) -> Result<Value> {
    match client
        .call(Operation::Api { action }, Duration::from_secs(30))
        .await?
    {
        Output::Api(value) => Ok(value),
        _ => bail!("API 管理返回了无效响应"),
    }
}

fn key_scope(paths: &[String]) -> ApiScope {
    if paths.is_empty() { ApiScope::All } else { ApiScope::Folders { paths: paths.to_vec() } }
}

fn mcp_addresses(status: &Value) -> (String, Vec<String>) {
    let local = format!("{}/mcp", status["local_url"].as_str().unwrap());
    let lan = status["lan_urls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|url| format!("{}/mcp", url.as_str().unwrap()))
        .collect();
    (local, lan)
}

pub fn run(command: &ApiCommand) -> Result<()> {
    let data = sag_protocol::data_dir()?;
    if let ApiCommand::Autostart { action } = command {
        std::fs::create_dir_all(&data)?;
        let enabled = match action {
            AutostartCommand::Status => sag_core::api_autostart::enabled(&data)?,
            AutostartCommand::Enable => {
                sag_core::api_autostart::configure(&data, &std::env::current_exe()?, true)?
            }
            AutostartCommand::Disable => {
                sag_core::api_autostart::configure(&data, &std::env::current_exe()?, false)?
            }
        };
        println!("自动启动：{}", if enabled { "已开启" } else { "已关闭" });
        return Ok(());
    }
    let action = match command {
        ApiCommand::Start { host, port } => ApiAction::Start {
            host: host.clone(),
            port: *port,
        },
        ApiCommand::Stop => ApiAction::Stop,
        ApiCommand::Web { action: WebCommand::Start { host, port } } => ApiAction::WebStart { host: host.clone(), port: *port },
        ApiCommand::Web { action: WebCommand::Stop } => ApiAction::WebStop,
        ApiCommand::Web { action: WebCommand::Code } => ApiAction::WebCode,
        ApiCommand::Web { action: WebCommand::AccessCode { mode } } => ApiAction::WebSetAccessCode { required: matches!(mode, WebAccessCode::On) },
        ApiCommand::Web { action: WebCommand::ExternalUrl { url, .. } } => ApiAction::WebSetExternalUrl { url: url.clone() },
        ApiCommand::Status { .. } => ApiAction::Status,
        ApiCommand::Key {
            action: KeyCommand::Create { name, paths, .. },
        } => if paths.is_empty() {
            ApiAction::CreateKey { name: name.clone() }
        } else {
            ApiAction::CreateScopedKey { name: name.clone(), paths: paths.clone() }
        },
        ApiCommand::Key {
            action: KeyCommand::SetScope { id, paths, .. },
        } => ApiAction::UpdateKeyScope { id: id.clone(), scope: key_scope(paths) },
        ApiCommand::Key {
            action: KeyCommand::List { .. },
        } => ApiAction::ListKeys,
        ApiCommand::Key {
            action: KeyCommand::Revoke { id },
        } => ApiAction::RevokeKey { id: id.clone() },
        ApiCommand::Mcp {
            action: McpCommand::Config { key_id, .. },
        } => ApiAction::CopyKey { id: key_id.clone() },
        ApiCommand::Autostart { .. } => unreachable!(),
    };
    let mut value = tokio::runtime::Runtime::new()?.block_on(async {
        let client = if matches!(
            command,
            ApiCommand::Status { .. } | ApiCommand::Stop | ApiCommand::Mcp { .. }
        ) {
            match sag_client::Client::connect(&data).await? {
                Some(client) => client,
                None => {
                    return match command {
                        // Persist an explicit stop even when the backend is off.
                        ApiCommand::Stop => sag_core::http_api::stop_offline(&data),
                        ApiCommand::Mcp { .. } => bail!("后台未运行，请先执行 sag api start"),
                        _ => sag_core::http_api::offline_status(&data),
                    };
                }
            }
        } else {
            sag_client::ensure(&data, &std::env::current_exe()?).await?
        };
        if let ApiCommand::Mcp {
            action: McpCommand::Config { address, .. },
        } = command
        {
            let (local, lan) = mcp_addresses(&call(&client, ApiAction::Status).await?);
            let url = address.as_deref().unwrap_or(&local);
            if url != local && !lan.iter().any(|candidate| candidate == url) {
                bail!("请使用 sag api status 列出的 MCP 地址");
            }
            let key = call(&client, action).await?;
            let secret = key["key"].as_str().unwrap();
            return Ok(json!({"mcpServers":{"sag-wiki":{
                "type":"http","url":url,"headers":{"Authorization":format!("Bearer {secret}")}
            }}}));
        }
        call(&client, action).await
    })?;
    if matches!(command, ApiCommand::Status { .. }) {
        let (local, lan) = mcp_addresses(&value);
        value["mcp_local_url"] = json!(local);
        value["mcp_lan_urls"] = json!(lan);
    }
    let json = matches!(
        command,
        ApiCommand::Status { json: true }
            | ApiCommand::Mcp { .. }
            | ApiCommand::Key {
                action: KeyCommand::Create { json: true, .. } | KeyCommand::SetScope { json: true, .. } | KeyCommand::List { json: true }
            }
    );
    if json {
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    match command {
        ApiCommand::Key {
            action: KeyCommand::Create { .. },
        } => println!(
            "Key ID：{}\n名称：{}\nAPI Key：{}\n请妥善保管此 Key。",
            value["id"].as_str().unwrap(),
            value["name"].as_str().unwrap(),
            value["key"].as_str().unwrap()
        ),
        ApiCommand::Key {
            action: KeyCommand::List { .. },
        } => {
            for key in value.as_array().unwrap() {
                println!(
                    "{}\t{}\t{}\t{}",
                    key["id"].as_str().unwrap(),
                    key["name"].as_str().unwrap(),
                    key["prefix"].as_str().unwrap(),
                    if key["scope"]["type"] == "folders" { key["scope"]["paths"].as_array().unwrap().iter().map(|path| path.as_str().unwrap()).collect::<Vec<_>>().join("; ") } else { "全部知识库".into() }
                )
            }
        }
        ApiCommand::Key {
            action: KeyCommand::Revoke { .. },
        } => println!("API Key 已撤销"),
        ApiCommand::Key {
            action: KeyCommand::SetScope { .. },
        } => println!("API Key 访问范围已更新"),
        ApiCommand::Web { action: WebCommand::Code } => println!("{}", value["code"].as_str().unwrap()),
        _ => {
            println!(
                "API：{}\n启用状态：{}\n监听地址：{}:{}\n本机地址：{}\n有效 Key：{}\n自动启动：{}",
                if value["running"] == true {
                    "运行中"
                } else {
                    "未运行"
                },
                value["enabled"],
                value["host"].as_str().unwrap(),
                value["port"],
                value["local_url"].as_str().unwrap(),
                value["key_count"],
                value["autostart"]
            );
            for url in value["lan_urls"].as_array().unwrap() {
                println!("局域网地址：{}", url.as_str().unwrap())
            }
            if matches!(command, ApiCommand::Status { .. } | ApiCommand::Web { .. }) {
                println!("网页服务：{}", if value["web_running"] == true {"运行中"} else {"未运行"});
                println!("网页访问码：{}", if value["web_access_code_required"] == false {"已关闭（可直接进入）"} else {"已开启"});
                if let Some(url) = value["web_external_url"].as_str() {
                    println!("外部访问地址：{url}");
                }
            }
            if matches!(command, ApiCommand::Status { .. }) {
                println!("本机 MCP：{}", value["mcp_local_url"].as_str().unwrap());
                for url in value["mcp_lan_urls"].as_array().unwrap() {
                    println!("局域网 MCP：{}", url.as_str().unwrap());
                }
            }
            if let Some(error) = value["last_error"].as_str() {
                println!("错误：{error}")
            }
            if value["running"] == true && value["key_count"] == 0 {
                println!("请求需要有效 API Key；当前请求将返回未授权（401）。")
            }
        }
    }
    Ok(())
}
