# sag CLI

## 独立安装与配置

sag 可以独立运行，不需要安装或启动 SAG Wiki，也不需要后台常驻进程。

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

如果安装时没有终端，首次在终端运行 `sag` 或 `sag "搜索内容"` 会自动进入同一引导；从搜索进入时，保存后继续原来的查询。已有密钥时自动使用现有配置。Ctrl+C 可取消，填写完成前不会保存。`sag config` 可以随时重新配置。

安装时也可直接传入密钥，并按需指定接口和模型：

```sh
sh install.sh --key '你的密钥'
sh install.sh --key '你的密钥' --url 'https://example.com/decisions' --model '模型名'
# 通过标准输入传递密钥
printf '%s' "$JEV_API_KEY" | sh install.sh --key-stdin
```

Windows 对应命令为 `powershell -File .\install.ps1 -Key '你的密钥'`，也支持 `-Url`、`-Model`。命令参数可能保存在终端历史中；希望避免记录密钥时，使用默认的隐藏输入或 `--key-stdin`。

密钥存入当前用户私有配置，不显示在配置结果或安装日志中。非交互环境不会等待输入；Agent 可读取明确错误并提示用户在终端运行 `sag`。`sag config --if-needed` 仅在缺少配置时引导，已有配置则保留；普通管道环境运行 `sag config` 只输出脱敏配置和是否已有密钥。

公共下载地址暂未设置。`scripts/install.sh` 支持 `SAG_RELEASE_BASE_URL` 接入之后的发布地址并校验下载包 SHA-256；`node scripts/package-cli.mjs` 生成当前平台的压缩包和校验文件，`SAG_TARGET` 可指定已配置的 Rust 目标平台。发布地址确定后才能提供公网一键安装命令。

## 与桌面端共用

CLI 和可选的 SAG Wiki 都引用 `core/` 中同一份索引、检索和配置实现，使用同一用户数据目录及 `sag.sqlite`、密钥和忽略规则。

- 桌面和 CLI 共用独立后台服务、同一份配置与知识库，目录变更会同步显示。
- 退出桌面后服务继续运行；`sag add` 索引完成后返回，文件及忽略规则变化由后台监听更新。
- `sag service status` 查看服务版本与进程，`sag service stop` 停止服务，`sag service restart` 重启服务。查看状态不会自动启动服务。
- 两种模式都保留访问开关及目录范围约束，失败不会切换为其他搜索算法。曾关闭 CLI 接入时，可显式运行 `sag config --enable` 恢复。
- 安装对应编程工具的 SAG Skill 后，新开会话即可使用；Agent 可运行 `sag add` 添加当前项目，再使用 `sag "搜索内容"`。

## 命令

```sh
sag status
sag list
sag list --json
sag check
sag check /path/to/project --json
sag config
sag add
sag add /path/to/project --json
sag remove
sag remove /path/to/project --json
sag "登录之后在哪里验证权限？"
sag -c "配置如何影响请求重试？" ./src -m 5
sag "连接池在哪里初始化？" --path ./server --json
sag -c "任务如何取消？" --timeout 900 --json
sag -E -c '^(ERROR|WARN) [0-9]+' ./logs
sag -F -c 'validatePermission(' ./src --json
sag install claude-code
sag install codex
sag install opencode
sag uninstall codex
sag uninstall cli
```

`sag list` 列出所有已加入知识库的目录、索引状态、文件数和索引错误；`--json` 返回目录数组。尚未添加目录时返回空列表，不会自动添加当前目录。

`sag check` 检查当前目录，也可指定目录。输出区分“已在索引目录内”“仅部分子目录已加入索引”“未加入索引”和“已排除”，同时显示索引就绪状态、CLI 接入状态及当前目录内的实际索引文件数。`--json` 的 `coverage` 分别为 `included`、`partial`、`none`、`excluded`。目录已加入、相关索引就绪、CLI 已启用且有已索引文件时退出码为 0；其他检查结果为 1；参数或执行错误为 2。忽略规则、目录排除设置和文件类型仍决定哪些文件可搜索。这两个命令不需要配置模型，也不会添加索引目录。

`sag status` 仅显示后台服务连接、服务版本、协议版本、服务进程、执行文件、CLI 接入和 JEV 配置状态。它不依赖当前目录的索引状态，也不会启动已停止的服务。目录覆盖、索引就绪状态和文件数统一使用 `sag check` 查看。

`sag add` 默认添加当前目录，也可指定目录。命令复用同一套添加与索引流程，已添加目录不会重复创建。桌面服务模式索引在后台进行，命令返回当前状态；独立模式完成索引后返回。用 `sag check` 检查就绪情况，无需 sync 参数。`--json` 返回路径、索引状态和当前 CLI 可用状态。添加的目录也会显示在桌面知识库。
`sag remove` 从知识库移除当前目录，也可指定已添加的根目录或其中的子目录。根目录会取消接入；子目录会清理索引并持久排除，后续扫描和文件监听不会重新加入。全部本地文件保持不变，执行 `sag add 同一子目录` 可恢复并重新索引。目录已移动或磁盘离线时，仍可按原来登记的完整路径移除记录。`--json` 返回移除的目录路径。移除后目录立即从桌面知识库中消失。UI 的文件夹菜单同样提供移除操作；文件设置 → 已移除的子目录中可以恢复。

搜索不需要 `search` 子命令，直接运行 `sag "搜索内容"`。`add`、`remove`、`list`、`check`、`status`、`config`、`service`、`install`、`uninstall` 保留为管理命令。搜索这些字面内容或以 `-` 开头的内容时，用 `--` 分隔，例如 `sag -c -- "status"`。

默认搜索当前执行目录；位置参数和 `--path` 不能同时使用。默认最多 10 个片段，允许 1–100。`-c` 仅改变输出，不改变召回与排序。路径相对于命令执行目录；JSON 同时返回完整路径、行号、实际范围和截断标记。仅 `-c` 返回 `content`。分离的原文范围分别输出，避免把不连续片段标成连续行。

### 正则与固定字符串匹配

不带模式参数时仍为语义搜索。`-E`（`--extended-regexp`）使用正则表达式，`-F`（`--fixed-strings`）按字面匹配字符串，两者不能同时使用。两种模式完全在本地执行，无需模型或 API 密钥，也不会触发首次模型配置。

两种模式默认区分大小写、逐行匹配；普通输出中同一行出现多次只返回一条结果。正则采用 Rust regex 语法，支持分组、分支、字符类及行首行尾；不支持环视和反向引用。使用单引号保护正则中的 shell 特殊字符。

匹配范围仍为已添加并完成索引的知识库目录，遵守访问开关、文件类型设置、`.gitignore`、`.sagignore` 和手动移除的子目录，读取当前文件内容。结果按文件遍历顺序返回，`-m` 限制匹配行数，`-c` 显示匹配行，`--json` 保留相同的路径和行号字段。没有匹配时返回空结果，退出码为 0。

下列匹配参数均需与 `-E` 或 `-F` 配合，不会改变默认语义搜索：

| 参数 | 用途 |
| --- | --- |
| `-i` / `--ignore-case` | 忽略大小写 |
| `-w` / `--word-regexp` | 匹配完整单词，边界字符为字母、数字和下划线 |
| `-x` / `--line-regexp` | 匹配完整行，同时使用时优先于 `-w` |
| `-v` / `--invert-match` | 选择不匹配的行 |
| `-A N` / `--after-context N` | 显示后面 N 行 |
| `-B N` / `--before-context N` | 显示前面 N 行 |
| `-C N` / `--context N` | 前后各显示 N 行 |
| `-l` / `--files-with-matches` | 每个匹配文件只输出一次文件名 |
| `--count` | 按文件统计匹配行数，包含零命中的文件 |
| `-o` / `--only-matching` | 只显示非空匹配片段，一行多处命中分别输出 |
| `-q` / `--quiet` | 不输出结果；找到为 0，未找到为 1，错误为 2 |
| `-e PATTERN` / `--regexp PATTERN` | 添加一个匹配条件，可重复 |
| `-f FILE` / `--file FILE` | 从 UTF-8 文件逐行读取条件，`-` 表示标准输入 |
| `--include GLOB` / `--exclude GLOB` | 按文件名通配符包含或排除文件，可重复 |

上下文参数自动显示内容，每侧允许 0–1000 行，重叠或相邻范围合并；JSON 的 `matched_lines` 标明其中哪些行命中。`-A`、`-B` 分别覆盖 `-C` 对应一侧。文件列表、计数、片段和静默四种输出方式互斥，不能配合上下文；`-o` 不能配合 `-v`，`-q` 不能配合 `--json`。`-c` 始终表示显示内容，计数仅使用 `--count`。

`-m` 是整个搜索的匹配行上限，普通输出默认 10 行。`-l` 下限制匹配文件数量；`-o` 返回选中行的全部非空命中片段。`--count` 默认统计全部，显式指定 `-m` 时在达到全局匹配行上限后停止，后续文件不再统计。JSON 的 `results` 保存行或片段，`files` 保存文件名或计数，同时提供 `matched` 和 `truncated`。

多个 `-e`、`-f` 条件按“任意一个匹配”处理。使用这些参数后，位置参数只表示搜索目录。空模式文件没有任何条件；`-e ''` 匹配任意行，配合 `-w` 时不匹配，配合 `-x` 时只匹配空行。单个条件最多 2000 字符，所有条件总大小最多 16 KiB。

文件过滤按命令中的顺序执行，最后一条匹配规则优先。第一条是 `--include` 时，其余未匹配名称不纳入；否则默认纳入。通配符只匹配文件名，不会覆盖知识库忽略规则及手动排除。

```sh
sag -F -i -w -c 'token' ./src
sag -E -e 'ERROR' -e 'WARN' ./logs -C 3
sag -F -f patterns.txt ./src --include='*.ts' --exclude='*.test.ts' -l
sag -F 'TODO' ./src --count
sag -F 'ready' ./src -q
```

请求默认超时 600 秒（独立模式包含索引检查时间），可用 `--timeout` 设置 1–86400 秒。大规模多批次仍可能需要提高。Ctrl+C 终止当前请求，退出码 130。搜索参数错误为 2，执行错误为 1，成功及空结果均为 0；`check` 使用前述检查退出码。stdout 为结果，stderr 为错误、安装信息和截断提示。

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
| CLI | 固定使用 `~/.local/bin`（Windows 文件名为 `sag.exe`） |
| Claude Code | `~/.claude/skills/sag/SKILL.md` |
| Codex | `~/.agents/skills/sag/SKILL.md` |
| OpenCode | `~/.config/opencode/skills/sag/SKILL.md` |

重复安装可更新 SAG 自有文件。安装记录保存文件校验值、版本和实际 CLI 安装位置；后续 PATH 顺序变化不会影响安装位置；卸载按记录清理。检测到外部文件、用户修改或被改写的 SAG 终端配置区段时停止并提示。卸载只删除经校验的 SAG 文件，保留同目录的其他内容。不会更改 AGENTS.md、MCP 配置或增加文件监听进程。

CLI 命令为 `sag`。客户端启动时会更新受管理的 CLI 和未被用户修改过的 SAG Skill；明确卸载的 CLI 不会自动恢复。`.sagignore` 文件名和已有知识库保持不变。

Windows 使用独立控制台 `sag.exe`。应用安装包附带该文件，默认不需要管理员权限安装 CLI。Windows 打包入口为 `npm run package:windows`。可用平台以发布包实际包含的原生程序为准。

## 错误处理

- 独立模式：没有桌面服务时自动使用本地核心，无需启动应用。
- 核心忙碌：另一个进程正在使用索引，请稍后重试。
- 接入关闭：在 CLI 设置页启用并保存。
- 没有已索引内容：运行 `sag add` 添加当前项目。程序不会改搜指定目录之外的知识库。
- 索引未就绪：等待索引完成；查看文件索引错误。
- JEV 未配置或请求失败：运行 `sag config` 检查配置，或在可选桌面端检查模型设置。
- 文件已修改：等待监听更新后重新查询。
- 安装冲突：保留现有文件，人工确认来源后再处理。

开发和测试可以用 `SAG_DATA_DIR` 指向独立数据目录，CLI 与桌面端必须使用同一个值。

## 终端自动配置

- macOS/Linux 支持 zsh、bash、fish 和 POSIX sh；尊重 `ZDOTDIR` 与 `XDG_CONFIG_HOME`。重复安装不会叠加配置，卸载保留已有环境设置。
- Windows 更新当前用户 PATH 并广播环境变化，不需要管理员权限。已打开的终端保留旧环境，请在安装后新开终端。
- 实现依据：[zsh 启动文件](https://zsh.sourceforge.io/Doc/Release/Files.html)、[Windows 环境变化通知](https://learn.microsoft.com/en-us/windows/win32/winmsg/wm-settingchange)。

## Harness 说明

当前提供 Claude Code、Codex 和 OpenCode 的 Skill 安装。

- [Claude Code Skill](https://code.claude.com/docs/en/skills)
- [Codex Skill](https://learn.chatgpt.com/docs/build-skills)
- [OpenCode Skill](https://opencode.ai/docs/skills/)

开发、构建与代码检查见[开发指南](开发指南.md)。
