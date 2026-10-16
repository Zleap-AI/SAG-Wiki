# sgrep CLI

## 独立安装与配置

sgrep 可以独立运行，不需要安装或启动 SAG-Desktop，也不需要后台常驻进程。

从对应平台的预编译安装包解压后，在该目录执行一条命令：

```sh
sh install.sh
```

Windows 解压对应 ZIP 后执行 `powershell -File .\install.ps1`。安装器放置用户级命令并自动配置 PATH；预编译包不要求 Rust、Node.js 或桌面框架。

开发者从本仓库源码安装也只需一条命令（需要 Rust 工具链）：

```sh
sh scripts/install.sh
```

交互安装会立即引导配置 JEV 检索服务。接口地址和模型名显示当前值，直接回车即可保留；首次使用必须填写密钥，输入隐藏。默认地址为 `https://api.302ai.cn/typesafeai/v1`，模型为 `jev-latest`。纯 CLI 搜索只需要这一项检索配置，问答模型在桌面端另行设置。

如果安装时没有终端，首次在终端运行 `sgrep` 或 `sgrep "搜索内容"` 会自动进入同一引导；从搜索进入时，保存后继续原来的查询。已有密钥时自动使用现有配置。Ctrl+C 可取消，填写完成前不会保存。`sgrep config` 可以随时重新配置。

安装时也可直接传入密钥，并按需指定接口和模型：

```sh
sh install.sh --key '你的密钥'
sh install.sh --key '你的密钥' --url 'https://example.com/decisions' --model '模型名'
# 通过标准输入传递密钥
printf '%s' "$JEV_API_KEY" | sh install.sh --key-stdin
```

Windows 对应命令为 `powershell -File .\install.ps1 -Key '你的密钥'`，也支持 `-Url`、`-Model`。命令参数可能保存在终端历史中；希望避免记录密钥时，使用默认的隐藏输入或 `--key-stdin`。

密钥存入当前用户私有配置，不显示在配置结果或安装日志中。非交互环境不会等待输入；Agent 可读取明确错误并提示用户在终端运行 `sgrep`。`sgrep config --if-needed` 仅在缺少配置时引导，已有配置则保留；普通管道环境运行 `sgrep config` 只输出脱敏配置和是否已有密钥。

公共下载地址暂未设置。`scripts/install.sh` 支持 `SGREP_RELEASE_BASE_URL` 接入之后的发布地址并校验下载包 SHA-256；`node scripts/package-cli.mjs` 生成当前平台的压缩包和校验文件，`SGREP_TARGET` 可指定已配置的 Rust 目标平台。发布地址确定后才能提供公网一键安装命令。

## 与桌面端共用

CLI 和可选的 SAG-Desktop 都引用 `core/` 中同一份索引、检索和配置实现，使用同一用户数据目录及 `sag.sqlite`、密钥和忽略规则。

- 桌面端正在运行时，CLI 复用其本机服务，目录变更会同步显示。
- 桌面端未运行时，CLI 直接运行核心；`sgrep add` 索引完成后返回，搜索前检查文件和忽略规则变更。
- 两种模式都保留访问开关及目录范围约束，失败不会切换为其他搜索算法。曾关闭 CLI 接入时，可显式运行 `sgrep config --enable` 恢复。
- 安装对应编程工具的 SAG Skill 后，新开会话即可使用；Agent 可运行 `sgrep add` 添加当前项目，再使用 `sgrep "搜索内容"`。

## 命令

```sh
sgrep status
sgrep config
sgrep add
sgrep add /path/to/project --json
sgrep remove
sgrep remove /path/to/project --json
sgrep "登录之后在哪里验证权限？"
sgrep -c "配置如何影响请求重试？" ./src -m 5
sgrep "连接池在哪里初始化？" --path ./server --json
sgrep -c "任务如何取消？" --timeout 900 --json
sgrep install claude-code
sgrep install codex
sgrep install opencode
sgrep uninstall codex
sgrep uninstall cli
```

`sgrep add` 默认添加当前目录，也可指定目录。命令复用同一套添加与索引流程，已添加目录不会重复创建。桌面服务模式索引在后台进行，命令返回当前状态；独立模式完成索引后返回。用 `sgrep status` 检查就绪情况，无需 sync 参数。`--json` 返回路径、索引状态和当前 CLI 可用状态。添加的目录也会显示在桌面知识库。
`sgrep remove` 从知识库移除当前目录，也可指定已添加的根目录或其中的子目录。根目录会取消接入；子目录会清理索引并持久排除，后续扫描和文件监听不会重新加入。全部本地文件保持不变，执行 `sgrep add 同一子目录` 可恢复并重新索引。目录已移动或磁盘离线时，仍可按原来登记的完整路径移除记录。`--json` 返回移除的目录路径。移除后目录立即从桌面知识库中消失。UI 的文件夹菜单同样提供移除操作；文件设置 → 已移除的子目录中可以恢复。

搜索不需要 `search` 子命令，直接运行 `sgrep "搜索内容"`。`add`、`remove`、`status`、`config`、`install`、`uninstall` 保留为管理命令。搜索这些字面内容或以 `-` 开头的内容时，用 `--` 分隔，例如 `sgrep -c -- "status"`。

默认搜索当前执行目录；位置参数和 `--path` 不能同时使用。默认最多 10 个片段，允许 1–100。`-c` 仅改变输出，不改变召回与排序。路径相对于命令执行目录；JSON 同时返回完整路径、行号、实际范围和截断标记。仅 `-c` 返回 `content`。分离的原文范围分别输出，避免把不连续片段标成连续行。

请求默认超时 600 秒（独立模式包含索引检查时间），可用 `--timeout` 设置 1–86400 秒。该默认值覆盖现有默认单轮超时的三轮检索；大规模多批次仍可能需要提高。Ctrl+C 终止当前请求，退出码 130。参数错误为 2，执行错误为 1，成功及空结果均为 0。stdout 为结果，stderr 为错误、安装信息和截断提示。

请求上限 32 KiB，响应上限 512 KiB。响应达到上限时保留完整片段并明确标记截断。不会为了缩短输出降低检索候选预算。文件发生修改时需要重新查询，不返回旧片段对应的新行号。

## 项目忽略规则

索引、文件监听和内置 list/grep 同时遵守项目根目录及其子目录中的 `.gitignore`、`.sagignore`。没有 Git 仓库也生效；不会读取知识库根目录之外的规则。`.sagignore` 的优先级高于 `.gitignore`，同类规则中更深目录优先。

例如在项目根目录创建 `.sagignore`：

```gitignore
# 排除生成文件和缓存
output/
**/*.generated.md
# 排除草稿，但保留指定文件
notes/draft-*.md
!notes/draft-final.md
```

支持 Git 忽略语法，包括 `*`、`**`、以 `/` 结尾的目录规则和 `!` 例外。父目录整体被忽略后，需先取消对父目录的忽略才能重新纳入其文件。现有的目录名排除设置和手动移除的子目录仍然生效，`!` 不会覆盖这两类设置。

独立模式在下次搜索前重新检查索引。桌面端启用自动监听时，创建、修改、删除或重命名规则文件都会重新检查项目：清除新增忽略项的索引，并索引不再被忽略的文件。关闭监听时，可在文件设置中点击重新索引。规则不会删除或隐藏本地文件，SAG 文件树仍可浏览它们；只有手动移除的子目录会从文件树隐藏。

## 安装位置与升级

| 对象 | 用户级位置 |
| --- | --- |
| CLI | 固定使用 `~/.local/bin`（Windows 文件名为 `sgrep.exe`） |
| Claude Code | `~/.claude/skills/sag/SKILL.md` |
| Codex | `~/.agents/skills/sag/SKILL.md` |
| OpenCode | `~/.config/opencode/skills/sag/SKILL.md` |

重复安装可更新 SAG 自有文件。安装记录保存文件校验值、版本和实际 CLI 安装位置；后续 PATH 顺序变化不会影响安装位置；重复安装会将旧临时目录中的 CLI 迁移到固定目录，卸载按记录清理。检测到外部文件、用户修改或被改写的 SAG 终端配置区段时停止并提示。卸载只删除经校验的 SAG 文件，保留同目录的其他内容。不会更改 AGENTS.md、MCP 配置或增加文件监听进程。

CLI 命令为 `sgrep`。`.sagignore` 文件名和已有知识库保持不变。

Windows 使用独立控制台 `sgrep.exe`。应用安装包附带该文件，默认不需要管理员权限安装 CLI。Windows 打包入口为 `npm run package:windows`。本轮在 macOS 验证；Windows 安装、签名和升级行为未验证。

## 错误处理

- 独立模式：没有桌面服务时自动使用本地核心，无需启动应用。
- 核心忙碌：另一个进程正在使用索引，请稍后重试。
- 接入关闭：在 CLI 设置页启用并保存。
- 没有已索引内容：运行 `sgrep add` 添加当前项目。程序不会改搜指定目录之外的知识库。
- 索引未就绪：等待索引完成；查看文件索引错误。
- JEV 未配置或请求失败：运行 `sgrep config` 检查配置，或在可选桌面端检查模型设置。
- 文件已修改：等待监听更新后重新查询。
- 安装冲突：保留现有文件，人工确认来源后再处理。

开发和测试可以用 `SGREP_DATA_DIR` 指向独立数据目录，CLI 与桌面端必须使用同一个值。

## 终端自动配置

- macOS/Linux 支持 zsh、bash、fish 和 POSIX sh；尊重 `ZDOTDIR` 与 `XDG_CONFIG_HOME`。重复安装不会叠加配置，卸载保留已有环境设置。
- Windows 更新当前用户 PATH 并广播环境变化，不需要管理员权限。已打开的终端保留旧环境，请在安装后新开终端。
- 实现依据：[zsh 启动文件](https://zsh.sourceforge.io/Doc/Release/Files.html)、[Windows 环境变化通知](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange)。

## 参考与来源

参考 mgrep 固定版本 `5c1ba628c62d9f3cf96cb98b3616f2ac23698aad` 的 Skill、四种安装器、`search.ts`、`watch_mcp.ts` 和监听 hooks。采用其 CLI + Skill 的接入方式。本实现独立编写 Rust 代码和 SAG Skill，没有复制或改写上游代码；未引入其 Node 运行时、云端索引、监听 hooks 或后台 MCP。

- [mgrep 固定版本](https://github.com/mixedbread-ai/mgrep/tree/5c1ba628c62d9f3cf96cb98b3616f2ac23698aad)，[Apache-2.0 许可证](https://github.com/mixedbread-ai/mgrep/blob/5c1ba628c62d9f3cf96cb98b3616f2ac23698aad/LICENSE)
- [Claude Code Skill 安装位置](https://code.claude.com/docs/en/skills)
- [Codex Skill 安装位置](https://learn.chatgpt.com/docs/build-skills)
- [OpenCode Skill 安装位置](https://opencode.ai/docs/skills/)

当前提供 Claude Code、Codex 和 OpenCode 的 Skill 安装。

## 开发验证

`npm run tauri -- dev` 会先生成调试版 CLI，再启动界面。直接运行 Rust 测试前，先执行 `npm run build:cli:dev` 以准备打包资源；共享核心回归使用 `cargo test --manifest-path core/Cargo.toml`，桌面后端回归使用 `cargo test --manifest-path src-tauri/Cargo.toml --lib --bins`，原生 CLI 测试使用 `cargo test --manifest-path cli/Cargo.toml`。
