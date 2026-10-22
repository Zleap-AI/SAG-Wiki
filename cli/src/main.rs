use sag_protocol as cli_protocol;
mod install;
use anyhow::{bail, Context, Result};
use clap::{CommandFactory, FromArgMatches, Parser, Subcommand};
use sag_protocol::types::{ModelAuthMode, ModelConfig};
use cli_protocol::*;
use std::{
    io::{IsTerminal, Read, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Parser)]
#[command(
    name = "sag",
    version,
    args_conflicts_with_subcommands = true,
    subcommand_negates_reqs = true,
    about = "独立搜索本地项目，也可与 SAG Wiki 共用知识库",
    group(clap::ArgGroup::new("match_mode").args(["extended_regexp", "fixed_strings"]))
)]
struct Args {
    #[command(subcommand)]
    command: Option<CliCommand>,
    /// 要搜索的内容。
    query: Option<String>,
    /// 按行匹配正则表达式，不调用模型。
    #[arg(short = 'E', long, conflicts_with = "fixed_strings")]
    extended_regexp: bool,
    /// 按行匹配固定字符串，不调用模型。
    #[arg(short = 'F', long)]
    fixed_strings: bool,
    #[command(flatten)]
    matching: MatchingArgs,
    #[arg(conflicts_with = "path")]
    directory: Option<PathBuf>,
    #[arg(long)]
    path: Option<PathBuf>,
    /// 最多选择的语义证据块（各段完整返回）；-E/-F 为匹配行数。默认 10，--count 默认统计全部。
    #[arg(short = 'm', long, value_parser = clap::value_parser!(u16).range(1..=100))]
    max_results: Option<u16>,
    /// 显示原文。语义搜索默认显示；-E/-F 匹配时可显式启用。
    #[arg(short = 'c', long)]
    content: bool,
    #[arg(long)]
    json: bool,
    #[arg(long, default_value_t = DEFAULT_TIMEOUT, value_parser = clap::value_parser!(u64).range(1..=86400))]
    timeout: u64,
}
#[derive(clap::Args, Default)]
#[group(id = "match_controls", multiple = true, requires = "match_mode")]
struct MatchingArgs {
    /// 忽略大小写（仅 -E / -F）。
    #[arg(short = 'i', long)]
    ignore_case: bool,
    /// 仅匹配完整单词（仅 -E / -F）。
    #[arg(short = 'w', long)]
    word_regexp: bool,
    /// 仅匹配完整行（仅 -E / -F）。
    #[arg(short = 'x', long)]
    line_regexp: bool,
    /// 选择不匹配的行（仅 -E / -F）。
    #[arg(short = 'v', long)]
    invert_match: bool,
    /// 显示匹配行之前的行数（仅 -E / -F）。
    #[arg(short = 'B', long, value_parser = clap::value_parser!(u16).range(0..=1000))]
    before_context: Option<u16>,
    /// 显示匹配行之后的行数（仅 -E / -F）。
    #[arg(short = 'A', long, value_parser = clap::value_parser!(u16).range(0..=1000))]
    after_context: Option<u16>,
    /// 前后各显示指定行数（仅 -E / -F）。
    #[arg(short = 'C', long, value_parser = clap::value_parser!(u16).range(0..=1000))]
    context: Option<u16>,
    /// 每个匹配文件只输出一次文件名。
    #[arg(short = 'l', long, group = "match_output", conflicts_with_all = ["before_context", "after_context", "context"])]
    files_with_matches: bool,
    /// 按文件统计匹配行数。
    #[arg(long, group = "match_output", conflicts_with_all = ["before_context", "after_context", "context"])]
    count: bool,
    /// 只输出非空匹配片段。
    #[arg(short = 'o', long, group = "match_output", conflicts_with_all = ["invert_match", "before_context", "after_context", "context"])]
    only_matching: bool,
    /// 静默判断：匹配退出 0，无匹配退出 1，错误退出 2。
    #[arg(short = 'q', long, group = "match_output", conflicts_with_all = ["json", "before_context", "after_context", "context"])]
    quiet: bool,
    /// 增加一个匹配条件，可重复使用。
    #[arg(short = 'e', long = "regexp", allow_hyphen_values = true)]
    patterns: Vec<String>,
    /// 从 UTF-8 文件逐行读取条件；- 表示标准输入。
    #[arg(short = 'f', long = "file", allow_hyphen_values = true)]
    pattern_files: Vec<PathBuf>,
    /// 只匹配符合通配符的文件名，可重复使用。
    #[arg(long)]
    include: Vec<String>,
    /// 排除符合通配符的文件名，可重复使用。
    #[arg(long)]
    exclude: Vec<String>,
    #[arg(skip)]
    filters: Vec<MatchFilter>,
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
    /// 查看后台服务、CLI 接入和模型配置状态。
    Status,
    /// 列出所有已加入知识库的文件夹及索引状态。
    List {
        #[arg(long)]
        json: bool,
    },
    /// 检查当前或指定文件夹的索引覆盖范围。
    Check {
        directory: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// 管理独立后台服务。
    Service { #[command(subcommand)] action: ServiceCommand },
    /// 配置检索服务。交互模式下隐藏密钥输入。
    Config(ConfigOptions),
    /// 将当前目录或指定目录加入知识库。
    Add {
        directory: Option<PathBuf>,
        /// 只索引指定扩展名，如 md,txt,html,py,ts；省略时保留已有设置。
        #[arg(long, value_delimiter = ',', num_args = 1)]
        types: Option<Vec<String>>,
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
#[derive(Subcommand)]
enum ServiceCommand { Status, Stop, Restart, #[command(hide=true)] Serve }
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
    /// Jev 鉴权方式；本地无鉴权服务使用 none。
    #[arg(long, value_parser = ["bearer", "none"])]
    auth: Option<String>,
    /// 直接传入密钥；命令可能保存在终端历史中。
    #[arg(long, conflicts_with = "key_stdin")]
    key: Option<String>,
    #[arg(long)]
    key_stdin: bool,
}
struct Interrupt {
    cancelled: AtomicBool,
    password_prompt: AtomicBool,
    connection: Mutex<Option<tokio_util::sync::CancellationToken>>,
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
fn configure(mut options: ConfigOptions, path: PathBuf, interrupt: Arc<Interrupt>, current:Option<ModelConfig>) -> Result<ModelConfig> {
    let current=match current{Some(current)=>current,None=>configured(request(Command::Configure { enable: false, url: None, model: None, key: None, auth_mode: None }, path.clone(), 10, interrupt.clone(), false)?)?};
    let auth_mode = options.auth.as_deref().map(|auth| if auth == "none" { ModelAuthMode::None } else { ModelAuthMode::Bearer });
    let requires_key = auth_mode.unwrap_or(current.auth_mode) == ModelAuthMode::Bearer;
    if !requires_key && (options.key.is_some() || options.key_stdin) { bail!("无需鉴权模式不能同时填写 API 密钥") }
    if options.if_needed && current.is_configured() && options.auth.is_none() && options.url.is_none() && options.model.is_none() && options.key.is_none() && !options.key_stdin && !options.enable {
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
    } else if requires_key && options.auth.is_none() && options.key.is_none() && !options.enable && interactive() && (options.if_needed || (options.url.is_none() && options.model.is_none())) {
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
    } else if options.if_needed && requires_key && !current.has_key && options.key.is_none() {
        bail!("尚未配置 JEV。请在终端运行 sag，或使用安装参数 --key / --key-stdin。")
    }
    if !options.enable && options.auth.is_none() && options.url.is_none() && options.model.is_none() && options.key.is_none() {
        return Ok(current);
    }
    let result = configured(request(Command::Configure { enable: options.enable, url: options.url, model: options.model, key: options.key, auth_mode }, path, 10, interrupt, false)?)?;
    eprintln!("配置已保存。");
    Ok(result)
}
fn main() {
    if std::env::args().nth(1).as_deref()==Some("--build-info") {
        println!("{}",serde_json::to_string(&sag_protocol::BuildInfo::current()).unwrap());return;
    }
    let matches = Args::command().get_matches();
    let mut args = Args::from_arg_matches(&matches).unwrap_or_else(|error| error.exit());
    if let Some(CliCommand::Service { action }) = &args.command {
        let result=run_service(action);
        if let Err(error)=result {eprintln!("{error:#}");std::process::exit(1);}
        return;
    }

    let mut filters = vec![];
    for (name, include) in [("include", true), ("exclude", false)] {
        if let (Some(indices), Some(values)) = (matches.indices_of(name), matches.get_many::<String>(name)) {
            filters.extend(indices.zip(values).map(|(index, pattern)| (index, MatchFilter { include, pattern: pattern.clone() })));
        }
    }
    filters.sort_by_key(|(index, _)| *index);
    args.matching.filters = filters.into_iter().map(|(_, filter)| filter).collect();
    let explicit_patterns = !args.matching.patterns.is_empty() || !args.matching.pattern_files.is_empty();
    if explicit_patterns {
        if args.directory.is_some() || args.query.is_some() && args.path.is_some() {
            Args::command().error(clap::error::ErrorKind::ArgumentConflict, "使用 -e/-f 时只可指定一个搜索目录").exit();
        }
        args.directory = args.query.take().map(PathBuf::from);
    }
    if let Some(query) = &args.query {
        let empty = if args.extended_regexp || args.fixed_strings { query.is_empty() } else { query.trim().is_empty() };
        if empty || query.chars().count() > 2000 {
            eprintln!("搜索内容不能为空或超过 2000 字符");
            std::process::exit(2);
        }
    }
    if args.command.is_none() && args.query.is_none() && !explicit_patterns && !interactive() {
        Args::command().error(clap::error::ErrorKind::MissingRequiredArgument,
            "请提供搜索内容；首次配置请在终端运行 sag").exit();
    }
    let interrupted = Arc::new(Interrupt { cancelled: AtomicBool::new(false), password_prompt: AtomicBool::new(false), connection: Mutex::new(None) });
    let handler = interrupted.clone();
    ctrlc::set_handler(move || {
        handler.cancelled.store(true, Ordering::SeqCst);
        if handler.password_prompt.load(Ordering::SeqCst) { return; }
        if let Some(connection) = handler.connection.lock().unwrap().as_ref() {
            connection.cancel();
        } else {
            std::process::exit(130);
        }
    }).unwrap_or_else(|error| {
        eprintln!("无法设置取消处理程序：{error}");
        std::process::exit(1);
    });
    let quiet = args.matching.quiet;
    let checking = matches!(args.command, Some(CliCommand::Check { .. }));
    let result = run(args, interrupted.clone());
    if let Ok(code) = result { if code != 0 { std::process::exit(code); } }
    else if let Err(e) = result {
        eprintln!("{e:#}");
        std::process::exit(if interrupted.cancelled.load(Ordering::SeqCst) {
            130
        } else if quiet || checking { 2 } else {
            1
        });
    }
}
fn run(args: Args, interrupted: Arc<Interrupt>) -> Result<i32> {
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
        return Ok(0);
    }
    let cwd = std::env::current_dir()?;
    let explicit_patterns = !args.matching.patterns.is_empty() || !args.matching.pattern_files.is_empty();
    if args.command.is_none() {
        if !args.extended_regexp && !args.fixed_strings && interactive() && !args.json {
            let current=configured(request(Command::Configure{enable:false,url:None,model:None,key:None,auth_mode:None},cwd.clone(),10,interrupted.clone(),false)?)?;
            if !current.is_configured() {
            configure(ConfigOptions::default(), cwd.clone(), interrupted.clone(),Some(current))?;
            if args.query.is_none() {
                eprintln!("接下来运行 sag add 添加当前目录，再运行 sag \"搜索内容\"。");
                return Ok(0);
            }
        }
            }
        if args.query.is_none() && !explicit_patterns {
            Args::command().print_help()?;
            println!();
            return Ok(0);
        }
    }
    let patterns = if args.extended_regexp || args.fixed_strings {
        let mut patterns = vec![];
        if explicit_patterns {
            for pattern in &args.matching.patterns {
                if pattern.is_empty() { patterns.push(String::new()); }
                else { patterns.extend(pattern.lines().map(str::to_owned)); }
            }
            let mut pattern_bytes: usize = patterns.iter().map(String::len).sum();
            for file in &args.matching.pattern_files {
                let mut source = String::new();
                if file == std::path::Path::new("-") {
                    std::io::stdin().lock().take(16 * 1024 + 1).read_to_string(&mut source)?;
                } else {
                    std::fs::File::open(file).with_context(|| format!("无法读取模式文件：{}", file.display()))?
                        .take(16 * 1024 + 1).read_to_string(&mut source).with_context(|| format!("模式文件必须为 UTF-8：{}", file.display()))?;
                }
                if source.len() > 16 * 1024 { bail!("模式文件超过 16 KiB") }
                pattern_bytes += source.len();
                if pattern_bytes > 16 * 1024 { bail!("匹配条件总大小超过 16 KiB") }
                patterns.extend(source.lines().map(str::to_owned));
            }
        } else { patterns.push(args.query.clone().expect("a bare invocation returns before matching")); }
        Some(patterns)
    } else { None };
    let (command, path, json, timeout) = match args.command {
        Some(CliCommand::Install { .. } | CliCommand::Uninstall { .. } | CliCommand::Service{..}) => unreachable!(),
        Some(CliCommand::Status) => (Command::Status, cwd.clone(), false, 10),
        Some(CliCommand::List { json }) => (Command::List, cwd.clone(), json, 10),
        Some(CliCommand::Check { directory, json }) => (
            Command::Check, cwd.join(directory.unwrap_or_default()), json, 10,
        ),
        Some(CliCommand::Config(options)) => {
            let config = configure(options, cwd, interrupted,None)?;
            println!("{}", serde_json::to_string_pretty(&config)?);
            return Ok(0);
        }
        Some(CliCommand::Add { directory, types, json }) => (
            match types { Some(extensions) => Command::AddWithTypes { extensions }, None => Command::Add },
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
            if args.extended_regexp || args.fixed_strings { Command::Match(MatchRequest {
                patterns: patterns.unwrap(),
                mode: if args.fixed_strings { MatchMode::Fixed } else { MatchMode::Regex },
                max_results: if args.matching.count { args.max_results.map(usize::from) } else { Some(args.max_results.unwrap_or(10).into()) },
                content: args.content || args.matching.before_context.is_some() || args.matching.after_context.is_some() || args.matching.context.is_some(),
                timeout: args.timeout,
                options: MatchOptions { ignore_case: args.matching.ignore_case, word_regexp: args.matching.word_regexp,
                    line_regexp: args.matching.line_regexp, invert_match: args.matching.invert_match,
                    before_context: args.matching.before_context.or(args.matching.context).unwrap_or(0).into(),
                    after_context: args.matching.after_context.or(args.matching.context).unwrap_or(0).into(),
                    output: if args.matching.quiet { MatchOutputMode::Quiet } else if args.matching.count { MatchOutputMode::Count }
                        else if args.matching.files_with_matches { MatchOutputMode::Files } else if args.matching.only_matching { MatchOutputMode::OnlyMatching } else { MatchOutputMode::Lines },
                    filters: args.matching.filters },
            }) } else { Command::Search {
                query: args.query.expect("a bare invocation returns before search"),
                max_results: args.max_results.unwrap_or(10).into(),
                content: true,
                timeout: args.timeout,
            } },
            cwd.join(args.path.or(args.directory).unwrap_or_default()),
            args.json,
            args.timeout,
        ),
    };
    let progress = !json && !args.matching.quiet && interactive()
        && matches!(&command, Command::Search { .. } | Command::Match(_));
    let response = request(command, path, timeout, interrupted, progress)?;
    match response {
        Response::Configured(config) => println!("{}", serde_json::to_string_pretty(&config)?),
        Response::Error(error) => bail!("{error}"),
        Response::Status(status) => println!("{}\nCLI 接入：{}\nJEV 已配置：{}", if status.connected { "已连接 SAG 后台服务" } else { "SAG 后台服务未运行" }, status.enabled, status.jev_configured),
        Response::Roots(roots) => {
            if json { println!("{}", serde_json::to_string(&roots)?); }
            else if roots.is_empty() { println!("尚未添加索引目录。请在目标目录运行 sag add。"); }
            else { print_roots(&roots); }
        }
        Response::Checked(output) => {
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                println!("目录：{}\n索引覆盖：{}\n索引已就绪：{}\nCLI 接入：{}\n当前目录内已索引文件数：{}",
                    output.path, match output.coverage {
                        IndexCoverage::Included => "已在索引目录内",
                        IndexCoverage::Partial => "仅部分子目录已加入索引",
                        IndexCoverage::Excluded => "已排除",
                        IndexCoverage::None => "未加入索引",
                    }, output.index_ready, output.enabled, output.indexed_files);
                if let Some(extensions) = &output.extensions { println!("原生索引格式：{}", extensions.join(",")); }
                if !output.roots.is_empty() { print_roots(&output.roots); }
                match output.coverage {
                    IndexCoverage::None | IndexCoverage::Partial => println!("如需索引整个目录，请在该目录运行 sag add。"),
                    IndexCoverage::Excluded => println!("请检查忽略规则、目录排除设置或已移除目录记录。"),
                    IndexCoverage::Included => println!("文件仍受忽略规则、文件类型和目录排除设置限制。"),
                }
            }
            return Ok(if output.coverage == IndexCoverage::Included && output.index_ready
                && output.enabled && output.indexed_files > 0 { 0 } else { 1 });
        }
        Response::Added(output) => {
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                println!("已加入知识库：{}", output.path);
                if output.cli_authorized { println!("CLI 可搜索此目录。"); }
                else { println!("CLI 接入已关闭，请在 SAG Wiki 设置中启用。"); }
                match output.index_status.as_str() {
                    "ready" => println!("索引已就绪，可以运行 sag \"搜索内容\"。"),
                    "error" => println!("索引失败，请在 SAG Wiki 查看详情。"),
                    _ => println!("正在建立索引，请用 sag check 查看进度；就绪后可运行 sag \"搜索内容\"。"),
                }
            }
        }
        Response::Removed { path } => {
            if json { println!("{}", serde_json::json!({"path":path})); }
            else { println!("已从知识库移除：{path}\n本地文件已保留。"); }
        }
        Response::Matches(mut output) => {
            if args.matching.quiet { return Ok(if output.matched { 0 } else { 1 }); }
            relative_results(&mut output.results, &cwd);
            for file in &mut output.files {
                if let Some(relative) = pathdiff::diff_paths(&file.absolute_path, &cwd) {
                    file.path = if relative.starts_with("..") { relative.display().to_string() } else { format!("./{}", relative.display()) };
                }
            }
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                print_results(&output.results);
                for file in &output.files {
                    if let Some(count) = file.count { println!("{}:{count}", file.path); }
                    else { println!("{}", file.path); }
                }
            }
            if output.truncated { eprintln!("输出已触及长度上限，部分内容未显示。请缩小搜索目录。"); }
        }
        Response::Search(mut output) => {
            relative_results(&mut output.results, &cwd);
            if json { println!("{}", serde_json::to_string(&output)?); }
            else {
                print_search_results(&output.results);
            }
            if output.truncated { eprintln!("输出已触及长度上限，部分片段未显示。请缩小搜索目录。"); }
        }
    }
    Ok(0)
}

fn relative_results(results: &mut [SearchResult], cwd: &std::path::Path) {
    for item in results {
        if let Some(relative) = pathdiff::diff_paths(&item.absolute_path, cwd) {
            item.path = if relative.starts_with("..") { relative.display().to_string() } else { format!("./{}", relative.display()) };
        }
    }
}
fn print_roots(roots: &[sag_protocol::types::Root]) {
    for root in roots {
        println!("{}\t{}\t{} 个文件", root.path, match root.status.as_str() {
            "ready" => "已就绪",
            "pending" => "等待索引",
            "indexing" => "正在索引",
            "partial" => "部分文件索引失败",
            "error" => "索引失败",
            other => other,
        }, root.count);
        if let Some(error) = &root.error { println!("  {error}"); }
    }
}
fn print_results(results: &[SearchResult]) {
    let mut previous_heading = None;
    for item in results {
        println!("{}:{}-{}", item.path, item.start_line, item.end_line);
        let heading = (&item.path, &item.heading);
        if !item.heading.is_empty() && previous_heading != Some(heading) {
            println!("标题：{}", item.heading);
        }
        previous_heading = Some(heading);
        if let Some(content) = &item.content { println!("{content}\n"); }
    }
}

fn print_search_results(results: &[SearchResult]) {
    let groups: Vec<_> = results.chunk_by(|left, right| {
        left.absolute_path == right.absolute_path && left.heading == right.heading
    }).collect();
    for (index, group) in groups.iter().enumerate() {
        let item = &group[0];
        println!("[{}/{}]", index + 1, groups.len());
        println!("filepath: {}", item.path);
        if !item.heading.is_empty() {
            println!("heading: {}", item.heading);
        }
        for item in *group {
            println!("lines: {}-{}", item.start_line, item.end_line);
            if let Some(content) = &item.content {
                println!("content:\n{content}");
            }
            println!();
        }
    }
}

fn request(command: Command, path: PathBuf, timeout: u64, interrupted: Arc<Interrupt>, progress: bool) -> Result<Response> {
    if interrupted.cancelled.load(Ordering::SeqCst) {bail!("操作已取消")}
    let cancel=tokio_util::sync::CancellationToken::new();
    *interrupted.connection.lock().unwrap()=Some(cancel.clone());
    let result=tokio::runtime::Runtime::new()?.block_on(async {
        let pending = async {
        let data=data_dir()?;
        let client=if matches!(&command,Command::Status) {
            match sag_client::Client::connect(&data).await? {
                Some(client)=>client,
                None=>return Ok(Response::Status(Status{protocol_version:VERSION,connected:false,enabled:false,jev_configured:false})),
            }
        } else {sag_client::ensure(&data,&std::env::current_exe()?).await?};
        if matches!(&command,Command::Status) {
            let info=client.info();
            println!("服务版本：{}\n协议版本：{}\n服务进程：{}\n执行文件：{}",info.build_id,info.protocol_version,info.pid,info.executable);
        }
        client.execute(path,command,Duration::from_secs(timeout)).await
        };
        tokio::pin!(pending);
        let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let started = Instant::now();
        let mut timer = tokio::time::interval(Duration::from_millis(100));
        let mut frame = 0;
        let result = loop {
            tokio::select! {
                biased;
                _ = cancel.cancelled() => break Err(anyhow::anyhow!("操作已取消")),
                result = &mut pending => break result,
                _ = timer.tick(), if progress => {
                    eprint!("\r{} {:.1}s", frames[frame % frames.len()], started.elapsed().as_secs_f32());
                    std::io::stderr().flush()?;
                    frame += 1;
                }
            }
        };
        if progress {
            eprint!("\r\x1b[2K");
            std::io::stderr().flush()?;
        }
        result
    });
    *interrupted.connection.lock().unwrap()=None;
    result
}
fn run_service(action:&ServiceCommand)->Result<()> {
    let runtime=tokio::runtime::Runtime::new()?;
    let data=data_dir()?;
    if matches!(action,ServiceCommand::Serve) {
        let stop=tokio_util::sync::CancellationToken::new();
        let signal=stop.clone();
        ctrlc::set_handler(move||signal.cancel())?;
        return runtime.block_on(sag_core::server::run_until(data,stop));
    }
    runtime.block_on(async {
        let connected=sag_client::Client::connect(&data).await?;
        if matches!(action,ServiceCommand::Status) {
            if let Some(client)=connected {println!("{}",serde_json::to_string_pretty(client.info())?);}
            else {println!("SAG 后台服务未运行");}
            return Ok(());
        }
        if let Some(client)=connected {
            client.call(sag_protocol::Operation::Shutdown,Duration::from_secs(10)).await?;
            drop(client);
            sag_client::wait_stopped(&data).await?;
        }
        if matches!(action,ServiceCommand::Restart) {
            let client=sag_client::ensure(&data,&std::env::current_exe()?).await?;
            println!("{}",serde_json::to_string_pretty(client.info())?);
        } else {println!("SAG 后台服务已停止");}
        Ok(())
    })
}
