# CLI 与 Harness 验证记录

日期：2026-10-16。代码已实现；完整 Harness 任务验收尚未完成。

## 范围与实现

本次复用 `SagTools::execute` → `retrieval::retrieve` → SAG2 V10 → 已有 JEV 请求。只开放该执行入口的 crate 可见性，未修改评分、扩展或排序规则。CLI 为独立 Rust crate，不链接 Tauri、SQLite、JEV 客户端或索引器。协议与安装器源码在 CLI 和桌面端共用。

设置页、启用开关、目录授权、原生 CLI 安装、四类 Skill 安装/卸载和连接测试均已实现。连接测试只检查 CLI 状态，界面明确提示真实任务仍需在 Harness 中验证。先完成界面配置和安装，关闭窗口后继续运行；退出应用后清理连接记录。

构建、回归与安装测试使用隔离副本、临时 HOME 和临时数据目录，保留并行探索视图开发。最终 CLI 安装包来自该隔离副本，不包含并行探索视图改动。日常 Harness 配置未修改。

## 自动验证

| 验证 | 结果 |
| --- | --- |
| Svelte 类型检查与前端构建 | 通过 |
| 前端全量测试（CLI 隔离副本） | 31 文件，166 项通过 |
| 后端 `cargo test --lib --bins` | 147 项通过，1 项既有测试忽略 |
| 原生 CLI 测试 | 4 项通过 |
| 通信回归 | 默认关闭、错误令牌、范围拒绝、符号链接越界、请求长度、中文/CRLF、断线取消、超时、连接耗尽后恢复、移除目录后保存 |
| 安装回归 | 重复安装、同名冲突、保留用户修改、保留同目录其他文件、PATH 变化后从原位置卸载 |

直接运行不带目标限制的 `cargo test` 时，仓库既有 `examples/` 有缺失 `document_text` 等模块导入的编译错误；本次未修改这些历史示例。生产库和二进制测试全部通过。

安装器的 HOME 隔离进程测试仅在 Unix 启用。Windows 的用户目录由系统接口决定，不能以 HOME 环境变量隔离，因此 Windows 安装测试未执行。

## macOS 实际操作

在独立的 SAG CLI QA 应用中验证：设置页显示和布局、应用授权配置、安装原生 CLI、安装 Claude Code 与 Codex Skill、通过设置页执行已安装 CLI 的 status，均通过。全部安装位于临时 HOME。

- `sag status` 返回连接、开关、目录、索引和 JEV 状态，不含凭据。
- 关闭窗口后，status 仍成功；真正退出应用后，连接记录被清理，status 返回退出码 1 并提示启动应用。
- Ctrl+C 返回 130，stdout 为空。
- 第二实例被拒绝，原实例继续工作。当前第二实例通过应用启动错误退出；没有覆盖连接记录。
- CLI 检索后，临时数据库聊天记录数仍为 0。

## 12 题真实检索

使用本次生成的 12 文件合成代码项目，原文保存在 `cli-validation/fixture/`。这属于功能验证，不能推断大型实际项目上的检索效果。

CLI 与索引运行于 macOS；JEV 请求经受保护的 SSH 转发由 US 主机 `ser5160924921` 发出，复用已配置的 JEV 服务和 `jev-latest`。四个 CLI 调用并行提交，桌面端与界面查询共用队列；单批 JEV 并发沿用 20。下表耗时含队列等待。

| 题号 | 预期文件 | 前三项结果 | 秒 |
| --- | --- | --- | --- |
| 1 | `src/session.ts` | 已命中 | 3.71 |
| 2 | `src/permissions.ts` | 已命中 | 7.81 |
| 3 | `src/retry.ts` | 已命中 | 5.61 |
| 4 | `src/pool.ts` | 已命中 | 9.65 |
| 5 | `src/cache.ts` | 已命中 | 9.69 |
| 6 | `src/queue.ts` | 已命中 | 9.76 |
| 7 | `src/webhook.ts` | 已命中 | 9.64 |
| 8 | `src/upload.ts` | 已命中 | 9.66 |
| 9 | `src/shutdown.ts` | 已命中 | 8.82 |
| 10 | `src/settings.ts` | 已命中 | 10.63 |
| 11 | `src/transaction.ts` | 已命中 | 10.46 |
| 12 | `src/routes.ts` | 已命中 | 10.59 |

全部 12 题成功，预期文件均在前三项。逐题查询、输出、行号和耗时保存在 `cli-validation/queries.json`。API token 用量未获取。该结果没有与 grep 或 mgrep 进行有效任务效果比较。

## Harness 实测限制

US 主机临时安装 Claude Code 2.1.301、Codex CLI 0.162.1、Node 22.17.0 和 Linux 原生 SAG CLI。尝试相同 12 题、同一个模型 `anthropic/claude-sonnet-4.5`，分 SAG Skill 和原有工具两组。

四次运行均在模型请求阶段失败，OpenRouter 返回 HTTP 403：`The request is prohibited due to a violation of provider Terms Of Service.`。原因未查明。Claude Code 与 Codex 均未实际执行搜索任务，无法确认 Skill 触发、任务成功率、工具次数或 token 对比。失败记录保存在 `cli-validation/`，不将配置安装成功计作 Harness 验收通过。

OpenCode、Factory Droid 已提供安装器，未进行实际客户端任务验证。Factory 官方文档本次无法读取，安装路径沿用固定版本 mgrep 的参考，仍需对应客户端验证。未配置 mgrep 云端账号，未进行原版 mgrep 对比。

测试结束后，US 代理已停止，临时传输的凭据文件已删除，SSH 转发和本地验证应用已关闭。

## 包体与资源

| 项目 | 改造前 | CLI 版本 |
| --- | --- | --- |
| macOS 应用占用（du -sk） | 20860 KiB | 21688 KiB |
| macOS DMG | 12420524 字节 | 12812876 字节 |
| 独立 CLI | 未提供 | 714640 字节 |

CLI 已计入应用和 DMG 总量。应用使用已有异步运行时，没有新增常驻 Node/Python 产品进程，没有第二份索引或第二个 watcher。测试用代理与构建工具不在分发包内。

同一验证会话主进程 RSS 样本：接入关闭时 200064 KiB，启用后 202704 KiB。样本包含先前检索缓存与界面状态差异，仅供记录，不能作为服务自身内存开销的对照。改造前同条件空闲内存未获取。status 五次启动调用的耗时中位数为 5.32 ms。

## Standards

初审发现忙碌连接错误会终止监听、安装检查隐藏读取错误；均已修复并复核。新增安装位置修复的复核发现 Windows HOME 隔离测试有风险，已限制为 Unix。最终未解决规范问题为 0。

## Spec

初审发现连接错误影响后续请求、移除授权目录后设置无法保存；均已修复并通过回归测试。实现缺陷已解决；真实 Harness 任务、Windows 和其他 Harness 平台验收仍未完成，不能视为整个实施计划完成。

## 交付

`releases/0.1.0-20261016-cli/` 包含 macOS arm64 DMG、独立 CLI、构建信息与 SHA-256。DMG 校验和与 CLI 签名核验通过。应用与 CLI 已作本地 ad-hoc 签名，未公证；Windows 打包、签名与升级未验证。
