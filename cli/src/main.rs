#[path = "protocol.rs"]
mod cli_protocol;
mod install;
use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use cli_protocol::*;
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{Shutdown, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "sgrep",
    version,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    about = "使用后台运行的 SAG-Desktop 搜索本地项目"
)]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,
    /// 要搜索的内容。
    #[arg(required = true)]
    query: Option<String>,
    #[arg(conflicts_with = "path")]
    directory: Option<PathBuf>,
    #[arg(long)]
    path: Option<PathBuf>,
    #[arg(short = 'm', long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=100))]
    max_results: u16,
    #[arg(short = 'c', long)]
    content: bool,
    #[arg(long)]
    json: bool,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT, value_parser = clap::value_parser!(u64).range(1..=86400))]
    timeout: u64,
}
#[derive(Subcommand)]
enum CliCommand {
    Install {
        #[arg(value_parser = ["cli", "claude-code", "codex", "opencode"])]
        target: String,
    },
    Uninstall {
        #[arg(value_parser = ["cli", "claude-code", "codex", "opencode"])]
        target: String,
    },
    Status,
    /// 将当前目录或指定目录加入知识库。
    Add {
        directory: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// 从知识库移除当前目录或指定目录，保留本地文件。
    Remove {
        directory: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
}
fn main() {
    let args = Args::parse();
    if let Some(query) = &args.query {
        if query.trim().is_empty() || query.chars().count() > 2000 {
            eprintln!("问题不能为空或超过 2000 字符");
            std::process::exit(2);
        }
    }
    let interrupted = Arc::new(AtomicBool::new(false));
    if let Err(e) = run(args, interrupted.clone()) {
        eprintln!("{e:#}");
        std::process::exit(if interrupted.load(Ordering::SeqCst) {
            130
        } else {
            1
        });
    }
}
fn run(args: Args, interrupted: Arc<AtomicBool>) -> Result<()> {
    if let Some(CliCommand::Install { target } | CliCommand::Uninstall { target }) = &args.command {
        let uninstall = matches!(args.command, Some(CliCommand::Uninstall { .. }));
        let home = dirs::home_dir().context("无法获取用户目录")?;
        let path = if target == "cli" {
            install::binary(&home, &std::env::current_exe()?, uninstall)?
        } else {
            install::harness(&home, target, uninstall)?
        };
        eprintln!(
            "{}：{}",
            if uninstall { "已卸载" } else { "已安装" },
            path.display()
        );
        if target == "cli" && !uninstall {
            eprintln!("终端路径已自动配置，请新开终端运行 sgrep。");
        }
        return Ok(());
    }
    let cwd = std::env::current_dir()?;
    let (command, path, json, timeout) = match args.command {
        Some(CliCommand::Install { .. } | CliCommand::Uninstall { .. }) => unreachable!(),
        Some(CliCommand::Status) => (Command::Status, cwd.clone(), false, 10),
        Some(CliCommand::Add { directory, json }) => (
            Command::Add,
            cwd.join(directory.unwrap_or_default()),
            json,
            10,
        ),
        Some(CliCommand::Remove { directory, json }) => (
            Command::Remove,
            cwd.join(directory.unwrap_or_default()),
            json,
            10,
        ),
        None => (
            Command::Search {
                query: args.query.expect("clap requires a query when no subcommand is given"),
                max_results: args.max_results.into(),
                content: args.content,
                timeout: args.timeout,
            },
            cwd.join(args.path.or(args.directory).unwrap_or_default()),
            args.json,
            args.timeout,
        ),
    };
    let endpoint: Endpoint = serde_json::from_slice(
        &std::fs::read(data_dir()?.join("cli-endpoint.json"))
            .context("无法连接 SAG-Desktop，请启动应用并在设置中检查 CLI 接入")?,
    )?;
    if endpoint.protocol_version != VERSION {
        bail!("CLI 与 SAG-Desktop 版本不兼容，请重新安装 CLI")
    }
    let mut stream = TcpStream::connect_timeout(
        &([127, 0, 0, 1], endpoint.port).into(),
        Duration::from_secs(3),
    )
    .context("无法连接 SAG-Desktop，请启动应用")?;
    stream.set_read_timeout(Some(Duration::from_secs(timeout + 5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let cancel = stream.try_clone()?;
    ctrlc::set_handler(move || {
        interrupted.store(true, Ordering::SeqCst);
        if let Err(e) = cancel.shutdown(Shutdown::Both) {
            eprintln!("停止连接失败：{e}");
        }
    })?;
    let request = Request {
        protocol_version: VERSION,
        token: endpoint.token,
        path,
        command,
    };
    let bytes = serde_json::to_vec(&request)?;
    if bytes.len() as u64 >= MAX_REQUEST {
        bail!("请求超过长度上限")
    }
    stream.write_all(&bytes)?;
    stream.write_all(b"\n")?;
    let mut line = Vec::new();
    BufReader::new(stream)
        .take(MAX_RESPONSE + 1)
        .read_until(b'\n', &mut line)
        .context("请求失败或超时")?;
    if line.len() as u64 > MAX_RESPONSE || !line.ends_with(b"\n") {
        bail!("SAG-Desktop 未返回完整结果")
    }
    match serde_json::from_slice(&line)? {
        Response::Error(error) => bail!("{error}"),
        Response::Status(status) => println!("已连接 SAG-Desktop\nCLI 接入：{}\n当前范围有索引：{}\n索引已就绪：{}\nJEV 已配置：{}\n目录：{}", status.enabled, status.allowed, status.index_ready, status.jev_configured, status.path),
        Response::Added(output) => {
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                println!("已加入知识库：{}", output.path);
                if output.cli_authorized { println!("CLI 可搜索此目录。"); }
                else { println!("CLI 接入已关闭，请在 SAG-Desktop 设置中启用。"); }
                match output.index_status.as_str() {
                    "ready" => println!("索引已就绪，可以运行 sgrep \"搜索内容\"。"),
                    "error" => println!("索引失败，请在 SAG-Desktop 查看详情。"),
                    _ => println!("正在建立索引，请用 sgrep status 查看进度；就绪后可运行 sgrep \"搜索内容\"。"),
                }
            }
        }
        Response::Removed { path } => {
            if json { println!("{}", serde_json::json!({"path":path})); }
            else { println!("已从知识库移除：{path}\n本地文件已保留。"); }
        }
        Response::Search(mut output) => {
            for item in &mut output.results {
                if let Some(relative) = pathdiff::diff_paths(&item.absolute_path, &cwd) {
                    item.path = if relative.starts_with("..") { relative.display().to_string() } else { format!("./{}", relative.display()) };
                }
            }
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                for item in &output.results {
                    println!("{}:{}-{}", item.path, item.start_line, item.end_line);
                    if let Some(content) = &item.content { println!("{content}\n"); }
                }
            }
            if output.truncated { eprintln!("输出已触及长度上限，部分片段未显示。请缩小搜索目录。"); }
        }
    }
    Ok(())
}
