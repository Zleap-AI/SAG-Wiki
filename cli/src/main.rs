use sag_core::protocol as cli_protocol;
mod install;
use anyhow::{bail, Context, Result};
use clap::{CommandFactory, Parser, Subcommand};
use sag_core::types::ModelConfig;
use cli_protocol::*;
use std::{
    io::{BufRead, BufReader, IsTerminal, Read, Write},
    net::{Shutdown, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "sag",
    version,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    about = "独立搜索本地项目，也可与 SAG-Desktop 共用知识库"
)]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,
    /// 要搜索的内容。
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
    /// 配置检索服务。交互模式下隐藏密钥输入。
    Config(ConfigOptions),
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
#[derive(clap::Args, Default)]
struct ConfigOptions {
    /// 重新启用曾关闭的 CLI 接入。
    #[arg(long)]
    enable: bool,
    /// 已配置时直接使用现有设置，未配置时引导填写。
    #[arg(long)]
    if_needed: bool,
    #[arg(long)]
    url: Option<String>,
    #[arg(long)]
    model: Option<String>,
    /// 直接传入密钥；命令可能保存在终端历史中。
    #[arg(long, conflicts_with = "key_stdin")]
    key: Option<String>,
    #[arg(long)]
    key_stdin: bool,
}
struct Interrupt {
    cancelled: AtomicBool,
    password_prompt: AtomicBool,
    connection: Mutex<Option<TcpStream>>,
}
fn interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal() && std::io::stdout().is_terminal()
}
fn configured(response: Response) -> Result<ModelConfig> {
    match response {
        Response::Configured(value) => Ok(value),
        Response::Error(error) => bail!("{error}"),
        _ => bail!("SAG 核心返回了无效的配置响应"),
    }
}
fn configure(mut options: ConfigOptions, path: PathBuf, interrupt: Arc<Interrupt>) -> Result<ModelConfig> {
    let current = configured(request(Command::Configure { enable: false, url: None, model: None, key: None }, path.clone(), 10, interrupt.clone())?)?;
    if options.if_needed && current.has_key && options.url.is_none() && options.model.is_none() && options.key.is_none() && !options.key_stdin && !options.enable {
        eprintln!("已使用现有 JEV 配置。");
        return Ok(current);
    }
    if let Some(value) = &mut options.key {
        *value = value.trim().to_owned();
        if value.is_empty() { bail!("API 密钥不能为空") }
    }
    if options.key_stdin {
        let mut input = String::new();
        std::io::stdin().read_to_string(&mut input)?;
        if input.trim().is_empty() { bail!("API 密钥不能为空") }
        options.key = Some(input.trim().to_owned());
    } else if options.key.is_none() && !options.enable && interactive() && (options.if_needed || (options.url.is_none() && options.model.is_none())) {
        eprintln!("配置 JEV 检索服务。接口和模型按回车使用当前值，密钥输入不会显示。");
        fn prompt(label: &str, current: &str) -> Result<Option<String>> {
            eprint!("{label} [{current}]：");
            std::io::stderr().flush()?;
            let mut input = String::new();
            if std::io::stdin().read_line(&mut input)? == 0 { bail!("配置已取消") }
            Ok((!input.trim().is_empty()).then(|| input.trim().to_owned()))
        }
        if options.url.is_none() { options.url = prompt("JEV 接口地址", &current.url)?; }
        if options.model.is_none() { options.model = prompt("模型名称", &current.model)?; }
        loop {
            // Let rpassword restore terminal mode before exiting on Ctrl+C.
            interrupt.password_prompt.store(true, Ordering::SeqCst);
            let result = rpassword::prompt_password(if current.has_key { "API 密钥（输入隐藏，留空保留）：" } else { "API 密钥（输入隐藏）：" });
            interrupt.password_prompt.store(false, Ordering::SeqCst);
            let input = result.map_err(|error| {
                if error.kind() == std::io::ErrorKind::Interrupted {
                    interrupt.cancelled.store(true, Ordering::SeqCst);
                }
                error
            })?;
            if !input.trim().is_empty() { options.key = Some(input.trim().to_owned()); break; }
            if current.has_key { break; }
            eprintln!("首次使用需要 API 密钥，请填写后继续。");
        }
    } else if options.if_needed && !current.has_key && options.key.is_none() {
        bail!("尚未配置 JEV。请在终端运行 sag，或使用安装参数 --key / --key-stdin。")
    }
    if !options.enable && options.url.is_none() && options.model.is_none() && options.key.is_none() {
        return Ok(current);
    }
    let result = configured(request(Command::Configure { enable: options.enable, url: options.url, model: options.model, key: options.key }, path, 10, interrupt)?)?;
    eprintln!("配置已保存。");
    Ok(result)
}
fn main() {
    let args = Args::parse();
    if let Some(query) = &args.query {
        if query.trim().is_empty() || query.chars().count() > 2000 {
            eprintln!("问题不能为空或超过 2000 字符");
            std::process::exit(2);
        }
    }
    if args.command.is_none() && args.query.is_none() && !interactive() {
        Args::command().error(clap::error::ErrorKind::MissingRequiredArgument,
            "请提供搜索内容；首次配置请在终端运行 sag").exit();
    }
    let interrupted = Arc::new(Interrupt { cancelled: AtomicBool::new(false), password_prompt: AtomicBool::new(false), connection: Mutex::new(None) });
    let handler = interrupted.clone();
    ctrlc::set_handler(move || {
        handler.cancelled.store(true, Ordering::SeqCst);
        if handler.password_prompt.load(Ordering::SeqCst) { return; }
        if let Some(connection) = handler.connection.lock().unwrap().as_ref() {
            if let Err(error) = connection.shutdown(Shutdown::Both) { eprintln!("停止连接失败：{error}"); }
        } else {
            std::process::exit(130);
        }
    }).unwrap_or_else(|error| {
        eprintln!("无法设置取消处理程序：{error}");
        std::process::exit(1);
    });
    if let Err(e) = run(args, interrupted.clone()) {
        eprintln!("{e:#}");
        std::process::exit(if interrupted.cancelled.load(Ordering::SeqCst) {
            130
        } else {
            1
        });
    }
}
fn run(args: Args, interrupted: Arc<Interrupt>) -> Result<()> {
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
            eprintln!("终端路径已自动配置，请新开终端运行 sag。");
        }
        return Ok(());
    }
    let cwd = std::env::current_dir()?;
    if args.command.is_none() {
        if interactive() && !args.json && !sag_core::models::has_secret(&data_dir()?, "jev")? {
            configure(ConfigOptions::default(), cwd.clone(), interrupted.clone())?;
            if args.query.is_none() {
                eprintln!("接下来运行 sag add 添加当前目录，再运行 sag \"搜索内容\"。");
                return Ok(());
            }
        }
        if args.query.is_none() {
            Args::command().print_help()?;
            println!();
            return Ok(());
        }
    }
    let (command, path, json, timeout) = match args.command {
        Some(CliCommand::Install { .. } | CliCommand::Uninstall { .. }) => unreachable!(),
        Some(CliCommand::Status) => (Command::Status, cwd.clone(), false, 10),
        Some(CliCommand::Config(options)) => {
            let config = configure(options, cwd, interrupted)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
            return Ok(());
        }
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
                query: args.query.expect("a bare invocation returns before search"),
                max_results: args.max_results.into(),
                content: args.content,
                timeout: args.timeout,
            },
            cwd.join(args.path.or(args.directory).unwrap_or_default()),
            args.json,
            args.timeout,
        ),
    };
    let response = request(command, path, timeout, interrupted)?;
    match response {
        Response::Configured(config) => println!("{}", serde_json::to_string_pretty(&config)?),
        Response::Error(error) => bail!("{error}"),
        Response::Status(status) => println!("{}\nCLI 接入：{}\n当前范围有索引：{}\n索引已就绪：{}\nJEV 已配置：{}\n目录：{}", if status.connected { "已连接 SAG-Desktop" } else { "SAG 核心（独立模式）" }, status.enabled, status.allowed, status.index_ready, status.jev_configured, status.path),
        Response::Added(output) => {
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                println!("已加入知识库：{}", output.path);
                if output.cli_authorized { println!("CLI 可搜索此目录。"); }
                else { println!("CLI 接入已关闭，请在 SAG-Desktop 设置中启用。"); }
                match output.index_status.as_str() {
                    "ready" => println!("索引已就绪，可以运行 sag \"搜索内容\"。"),
                    "error" => println!("索引失败，请在 SAG-Desktop 查看详情。"),
                    _ => println!("正在建立索引，请用 sag status 查看进度；就绪后可运行 sag \"搜索内容\"。"),
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

fn request(command: Command, path: PathBuf, timeout: u64, interrupted: Arc<Interrupt>) -> Result<Response> {
    if interrupted.cancelled.load(Ordering::SeqCst) { bail!("操作已取消") }
    let data = data_dir()?;
    let endpoint: Option<Endpoint> = match std::fs::read(data.join("cli-endpoint.json")) {
        Ok(bytes) => Some(serde_json::from_slice(&bytes).context("CLI 连接记录格式无效")?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let connection = if let Some(endpoint) = endpoint {
        if endpoint.protocol_version != VERSION {
            bail!("CLI 与 SAG 核心版本不兼容")
        }
        match TcpStream::connect_timeout(&([127, 0, 0, 1], endpoint.port).into(), Duration::from_secs(3)) {
            Ok(stream) => Some((endpoint, stream)),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::TimedOut) => None,
            Err(error) => return Err(error.into()),
        }
    } else { None };
    let Some((endpoint, mut stream)) = connection else {
        // A standalone invocation owns the shared index only for its lifetime.
        // Cover synchronous index refresh as well as the async request. Dropping
        // the sender stops the timer after success; process exit releases SQLite locks.
        let (deadline, timer) = std::sync::mpsc::channel::<()>();
        if matches!(command, Command::Search { .. }) {
            std::thread::spawn(move || {
                if matches!(timer.recv_timeout(Duration::from_secs(timeout)), Err(std::sync::mpsc::RecvTimeoutError::Timeout)) {
                    eprintln!("请求超时（包括索引检查），请缩小搜索目录或增加 --timeout");
                    std::process::exit(1);
                }
            });
        }
        let _deadline = deadline;
        let runtime = tokio::runtime::Runtime::new()?;
        return runtime.block_on(async {
            let core = sag_core::cli_service::CliService::local(data)?;
            core.run_local(path, command, tokio_util::sync::CancellationToken::new()).await
        });
    };
    stream.set_read_timeout(Some(Duration::from_secs(timeout + 5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    *interrupted.connection.lock().unwrap() = Some(stream.try_clone()?);
    let response = (|| -> Result<Response> {
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
        Ok(serde_json::from_slice(&line)?)
    })();
    *interrupted.connection.lock().unwrap() = None;
    response
}
