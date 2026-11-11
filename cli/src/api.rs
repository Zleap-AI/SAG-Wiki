use anyhow::{bail, Result};
use clap::Subcommand;
use sag_protocol::{ApiAction, Operation, Output};
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
    /// 查看 API 状态，不自动启动后台。
    Status {
        #[arg(long)]
        json: bool,
    },
    /// 管理调用 API 所用的 Key。
    Key {
        #[command(subcommand)]
        action: KeyCommand,
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
pub enum AutostartCommand {
    Enable,
    Disable,
    Status,
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
        ApiCommand::Status { .. } => ApiAction::Status,
        ApiCommand::Key {
            action: KeyCommand::Create { name, .. },
        } => ApiAction::CreateKey { name: name.clone() },
        ApiCommand::Key {
            action: KeyCommand::List { .. },
        } => ApiAction::ListKeys,
        ApiCommand::Key {
            action: KeyCommand::Revoke { id },
        } => ApiAction::RevokeKey { id: id.clone() },
        ApiCommand::Autostart { .. } => unreachable!(),
    };
    let value = tokio::runtime::Runtime::new()?.block_on(async {
        let client = if matches!(command, ApiCommand::Status { .. } | ApiCommand::Stop) {
            match sag_client::Client::connect(&data).await? {
                Some(client) => client,
                None => {
                    if matches!(command, ApiCommand::Stop) {
                        // Persist an explicit stop even when the backend is off.
                        return sag_core::http_api::stop_offline(&data);
                    }
                    return sag_core::http_api::offline_status(&data);
                }
            }
        } else {
            sag_client::ensure(&data, &std::env::current_exe()?).await?
        };
        match client
            .call(Operation::Api { action }, Duration::from_secs(30))
            .await?
        {
            Output::Api(value) => Ok(value),
            _ => bail!("API 管理返回了无效响应"),
        }
    })?;
    let json = matches!(
        command,
        ApiCommand::Status { json: true }
            | ApiCommand::Key {
                action: KeyCommand::Create { json: true, .. } | KeyCommand::List { json: true }
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
            "Key ID：{}\n名称：{}\nAPI Key：{}\n请保存此 Key，关闭后无法再次查看。",
            value["id"].as_str().unwrap(),
            value["name"].as_str().unwrap(),
            value["key"].as_str().unwrap()
        ),
        ApiCommand::Key {
            action: KeyCommand::List { .. },
        } => {
            for key in value.as_array().unwrap() {
                println!(
                    "{}\t{}\t{}",
                    key["id"].as_str().unwrap(),
                    key["name"].as_str().unwrap(),
                    key["prefix"].as_str().unwrap()
                )
            }
        }
        ApiCommand::Key {
            action: KeyCommand::Revoke { .. },
        } => println!("API Key 已撤销"),
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
