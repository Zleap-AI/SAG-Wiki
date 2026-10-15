# CLI

## 启用与安装

1. 在 SAG-Desktop 中添加项目文件夹并完成索引，配置 JEV。
2. 打开设置 → CLI，启用 CLI，勾选允许搜索的文件夹，点击底部“保存设置”。点击“取消”将放弃尚未保存的权限修改。
3. 安装 SAG CLI。界面显示实际安装路径，安装器自动配置终端路径；完成后新开终端即可运行 `sag`。macOS/Linux 在当前 Shell 配置中添加独立的 SAG 区段，Windows 配置当前用户 PATH。卸载时只移除 SAG 添加的内容，保留用户原有配置；不会覆盖缺少 SAG 安装记录的同名文件。
4. 安装对应 Harness 的 Skill，并重新启动 Harness。测试按钮执行已安装 CLI 的 `status`；真实模型是否使用 Skill，仍需通过任务记录确认。

关闭窗口后应用继续后台运行。退出 SAG-Desktop 后 CLI 不可用；CLI 不会启动应用。默认关闭搜索授权。本机监听仅提供 status/search，认证信息保存在当前用户的数据目录，不进入命令参数。

## 命令

```sh
sag status
sag search "登录之后在哪里验证权限？"
sag search -c "配置如何影响请求重试？" ./src -m 5
sag search "连接池在哪里初始化？" --path ./server --json
sag search -c "任务如何取消？" --timeout 900 --json
sag install claude-code
sag install codex
sag install opencode
sag uninstall codex
sag uninstall cli
```

默认搜索当前执行目录；位置参数和 `--path` 不能同时使用。默认最多 10 个片段，允许 1–100。`-c` 仅改变输出，不改变召回与排序。路径相对于命令执行目录；JSON 同时返回完整路径、行号、实际范围和截断标记。仅 `-c` 返回 `content`。分离的原文范围分别输出，避免把不连续片段标成连续行。

请求默认超时 600 秒，可用 `--timeout` 设置 1–86400 秒。该默认值覆盖现有默认单轮超时的三轮检索；大规模多批次仍可能需要提高。Ctrl+C 终止当前请求，退出码 130。参数错误为 2，执行错误为 1，成功及空结果均为 0。stdout 为结果，stderr 为错误、安装信息和截断提示。

请求上限 32 KiB，响应上限 512 KiB。响应达到上限时保留完整片段并明确标记截断。不会为了缩短输出降低检索候选预算。文件发生修改时需要重新查询，不返回旧片段对应的新行号。

## 安装位置与升级

| 对象 | 用户级位置 |
| --- | --- |
| CLI | 固定使用 `~/.local/bin`（Windows 文件名为 `sag.exe`） |
| Claude Code | `~/.claude/skills/sag/SKILL.md` |
| Codex | `~/.agents/skills/sag/SKILL.md` |
| OpenCode | `~/.config/opencode/skills/sag/SKILL.md` |

重复安装可更新 SAG 自有文件。安装记录保存文件校验值、版本和实际 CLI 安装位置；后续 PATH 顺序变化不会影响安装位置；重复安装会将旧临时目录中的 CLI 迁移到固定目录，卸载按记录清理。检测到外部文件、用户修改或被改写的 SAG 终端配置区段时停止并提示。卸载只删除经校验的 SAG 文件，保留同目录的其他内容。不会更改 AGENTS.md、MCP 配置或增加文件监听进程。

Windows 使用独立控制台 `sag.exe`。应用安装包附带该文件，默认不需要管理员权限安装 CLI。Windows 打包入口为 `npm run package:windows`。本轮在 macOS 验证；Windows 安装、签名和升级行为未验证。

## 错误处理

- 无法连接：启动 SAG-Desktop；确认当前 CLI 与应用使用同一用户数据目录。
- 接入关闭：在 CLI 设置页启用并应用。
- 目录未授权：先添加项目目录，再在 CLI 设置页授权。程序不会改搜其他知识库。
- 索引未就绪：等待索引完成；查看文件索引错误。
- JEV 未配置或请求失败：检查模型设置；失败不会切换成其他检索算法。
- 文件已修改：等待监听更新后重新查询。
- 安装冲突：保留现有文件，人工确认来源后再处理。

开发和测试可以用 `SAG_DESKTOP_DATA_DIR` 指向独立数据目录，CLI 与桌面端必须使用同一个值。

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

`npm run tauri -- dev` 会先生成调试版 CLI，再启动界面。直接运行 Rust 测试前，先执行 `npm run build:cli:dev` 以准备打包资源；后端回归使用 `cargo test --manifest-path src-tauri/Cargo.toml --lib --bins`，原生 CLI 测试使用 `cargo test --manifest-path cli/Cargo.toml`。
