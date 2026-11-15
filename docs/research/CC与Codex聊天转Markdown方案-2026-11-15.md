# Claude Code 与 Codex 聊天记录转 Markdown：现成方案调查

调查日期：2026-11-15。CC 按 Claude Code 理解。范围是本地保存的聊天记录；未将云端聊天或 ChatGPT 网页聊天视为同一格式。

本次核验了项目 README、依赖声明和关键解析源码，没有安装或执行第三方工具，没有读取个人聊天记录。因此，“已确认”表示存在对应代码，不代表已经通过真实会话兼容性测试。

## 高 Star 候选补查与推荐调整

2026-11-15 通过 GitHub REST API 按 Star 排序检索，并读取下列仓库的 README 与导出源码。首轮偏向独立转换器，遗漏了较成熟的聊天归档项目；优先推荐据此调整。Star 为查询时数值，不代表完整性实测结果。

| 项目 | Stars | Markdown 与来源支持 | 接入判断 |
| --- | ---: | --- | --- |
| [agentsview](https://github.com/kenn-io/agentsview) | 6,439 | CC + Codex；Markdown 服务接口，支持子会话 | MIT，Go 后端。完整管理工具，可外部试用 |
| [claude-code-history-viewer](https://github.com/jhlee0409/claude-code-history-viewer) | 2,316 | CC + Codex；界面 Markdown 导出，内容类型过滤 | MIT，Rust/Tauri + TypeScript，与 SAG 接近，优先研究内置复用 |
| [SpecStory](https://github.com/specstoryai/getspecstory) | 1,374 | CC + Codex；批量历史转 Markdown、自动保存 | Apache-2.0，Go。优先验证外部批量导出接入 SAG |
| [claude-code-log](https://github.com/daaain/claude-code-log) | 1,262 | CC 的批量 Markdown 与监听；Codex 单会话 beta 支持 | MIT，Python；CC 归档候选，Codex 批量不能视为已支持 |
| [coding_agent_session_search](https://github.com/Dicklesworthstone/coding_agent_session_search) | 1,212 | CC + Codex；`cass export --format markdown` | Rust，但许可证附 OpenAI/Anthropic Rider，不能按普通 MIT 复用 |

源码依据：

- history-viewer 的 [Markdown 输出](https://github.com/jhlee0409/claude-code-history-viewer/blob/main/src/services/export/markdownExporter.ts)、[界面调用](https://github.com/jhlee0409/claude-code-history-viewer/blob/main/src/hooks/useExport.ts)、[Codex 解析](https://github.com/jhlee0409/claude-code-history-viewer/blob/main/src-tauri/src/providers/codex.rs)。Codex 已处理 `.jsonl.zst` 及压缩副本去重。其无界面导出目前只有 HTML/JSON，不能把界面 Markdown 导出称为现成 CLI。
- agentsview 的 [Markdown 服务实现](https://github.com/kenn-io/agentsview/blob/main/internal/server/export_markdown.go)、[接口测试](https://github.com/kenn-io/agentsview/blob/main/frontend/src/lib/api/client-markdown-export.test.ts)。接口为 `/api/v1/sessions/<id>/md`，子会话可用 `depth=all`。
- SpecStory 的 [CLI 说明](https://github.com/specstoryai/getspecstory/blob/dev/specstory-cli/README.md)：`specstory sync` 导出当前项目历史，默认输出 `.specstory/history/`；支持自定义输出和关闭云同步。其 [Codex 来源发现](https://github.com/specstoryai/getspecstory/blob/dev/specstory-cli/pkg/providers/codexcli/provider.go) 仍按 `.jsonl` 枚举，压缩历史覆盖需要补充验证，不能宣称能完整导出所有 Codex 历史。
- claude-code-log 的 [说明](https://github.com/daaain/claude-code-log/blob/main/README.md) 明确 Codex 为 beta 单会话导出，支持 `--format md`、`watch` 和层级过滤。
- cass 的 [许可证原文](https://github.com/Dicklesworthstone/coding_agent_session_search/blob/main/LICENSE) 含附加限制。

当前建议：若目标是尽快批量生成知识库，先验证 SpecStory；若目标是在 SAG 内提供选择、预览与导入界面，优先拆解 history-viewer 的来源解析和 Markdown 导出。尚未运行上述工具或读取个人聊天，最新 Codex 分段、压缩、附件完整性仍需以脱敏样本验收。

## 首轮结论（下方高 Star 候选补查后调整）

有现成方案，可先通过外部工具生成 Markdown，再接入 SAG 已有的文件夹导入。适合产品内置的解析代码也有参考，但本次三个主要候选都存在需要处理的限制，暂不建议直接选择一个整体嵌入。

- **优先参考 unabridged 的对话清洗和 Markdown 输出**：同时支持两者，MIT 许可证，纯 Node.js 模块，不需要模型 API。默认导出用户与助手对话，图片可独立保存。但项目很新，Codex 同一会话的多文件合并需要补充验证与实现。
- **批量和增量归档参考 ace**：具有现成命令和程序接口。但当前 Codex 解析存在重复消息、指令混入，以及压缩文件遗漏问题，必须修正后才能作为可靠的数据源。
- **ai-chat-exporter 的界面和 Codex 多文件处理值得研究**：已处理分段、压缩和附件。但仓库未发现许可证，暂不能将其源码作为可直接复用的依赖。

以上建议是基于下列源码检查作出的判断。

## 候选比较

| 项目 | 已确认能力 | 主要限制 | 适用方式 |
| --- | --- | --- | --- |
| [unabridged](https://github.com/saieeshb/unabridged) | Claude Code、Codex；Markdown/HTML/JSON；默认纯对话，可选工具调用及推理摘要；保存内嵌图片；本地执行；MIT | 当前主要导出单个会话；无内置增量批量同步；多段 Codex 会话合并未实现；新项目，真实格式覆盖率待测 | 对话清洗和导出参考，或少量会话手动导出 |
| [ace](https://github.com/mjaverto/ace) | 两者批量导出；Markdown 元数据；增量更新；CLI 和库接口；定时运行；MIT | 不支持 `.jsonl.zst`；Codex 解析存在重复及指令混入；默认保留工具日志；附带 Node 原生依赖 | 批量归档与调度参考，修复解析后再接入 |
| [ai-chat-exporter](https://github.com/rc2barrington/ai-chat-exporter) | 本地界面选取、批量导出；两者及 OpenCode；附件；Codex 分段合并、压缩和副本去重 | 没有查到许可证；没有明确的通用库接口或增量同步功能；每次导出新文件 | 可先外部试用，内置复用需先明确许可证 |

## 1. unabridged

固定源码版本：`05f9d9f846ab521e00d5983004cbada9555ab992`。

- JavaScript ES 模块；`package.json` 标明 MIT，Node.js ≥18，未声明运行时 npm 依赖。读取压缩 Codex 记录需要 Node.js ≥22.15。包声明 `private: true`，不能默认当作已发布 npm 库使用。[依赖声明](https://github.com/saieeshb/unabridged/blob/05f9d9f846ab521e00d5983004cbada9555ab992/package.json)
- 可直接从检出的代码执行 CLI，例如 `node /path/to/unabridged/bin/unabridged.mjs <session-id> --from codex -f md -o /path/to/output`。`--list --all --from all` 可列出两者各项目的会话；这里的 `--all` 用于列表范围，不能理解为批量导出全部会话。[CLI](https://github.com/saieeshb/unabridged/blob/05f9d9f846ab521e00d5983004cbada9555ab992/scripts/export-chat.mjs)
- `exportSession()` 可作为源码级接口；默认 detail 为 conversation。Markdown 图片写入相邻的 `<name>_files/`，可选 `--redact` 脱敏。[导出代码](https://github.com/saieeshb/unabridged/blob/05f9d9f846ab521e00d5983004cbada9555ab992/lib/export.mjs)
- Codex 遍历支持 `.jsonl` 和 `.jsonl.zst`，包含 archived_sessions。但索引按文件创建记录、以会话 ID 建 Map；加载仅接收其中一个文件。没有看到同一会话多段 rollout 的汇总步骤。因此压缩支持不等于完整长会话支持。[Codex 解析](https://github.com/saieeshb/unabridged/blob/05f9d9f846ab521e00d5983004cbada9555ab992/lib/codex.mjs)
- GitHub 元数据：2026-11-05 创建，最近 push 为同日，调查时 1 star、6 次提交。只能说明项目较新，不能以 star 数判断正确性。[仓库元数据](https://api.github.com/repos/saieeshb/unabridged)

## 2. ace

固定源码版本：`1cd7138a2038d17ad3c000d606b3a57bbe9daf14`。

- TypeScript；npm 包名 `@mjaverto/ace`，仓库版本 0.1.3；MIT；Node.js ≥20。依赖中包含 `better-sqlite3`，SAG 内置整个包会增加 Node 运行时和原生依赖打包工作。[依赖声明](https://github.com/mjaverto/ace/blob/1cd7138a2038d17ad3c000d606b3a57bbe9daf14/package.json)
- `ace render` 批量增量导出；支持 mtime 或 index 两种判断方式、输出目录、JSON 结果；`ace render-one` 可处理一个输入。`install launchd|systemd|cron` 安排定期执行，没有查到实时 watch 命令。[说明](https://github.com/mjaverto/ace/blob/1cd7138a2038d17ad3c000d606b3a57bbe9daf14/README.md)
- 公共接口包括 `runRender`、`claudeSource`、`codexSource`、`Registry`，适合在有 Node 环境的程序中调用。[库接口](https://github.com/mjaverto/ace/blob/1cd7138a2038d17ad3c000d606b3a57bbe9daf14/src/index.ts)
- 已确认的 Codex 解析问题：匹配规则仅接受 `.jsonl`；`response_item.message` 未限定角色且把所有非 user 归为 Assistant，可能混入 system/developer 指令；同时输出 `response_item.message` 和 `event_msg.user_message`，源文件同时保存这两类事件时会重复用户消息。默认输出工具调用，工具输入/输出各截断至 4000 字节，不能等同于纯对话导出。[Codex 解析](https://github.com/mjaverto/ace/blob/1cd7138a2038d17ad3c000d606b3a57bbe9daf14/src/sources/codex.ts)
- GitHub 元数据：2026-05-03 创建，最近 push 为 2026-09-27，调查时 2 stars。[仓库元数据](https://api.github.com/repos/mjaverto/ace)

## 3. ai-chat-exporter

固定源码版本：`db8566fc158d6d4441c17c1fc9cbb921835f2824`。

- JavaScript/React 本地应用，当前仓库版本 1.2.18。源码 `package.json` 没有 license 字段，仓库树没有 LICENSE，GitHub license 为 null。此处结论仅为“未找到许可证”。[依赖声明](https://github.com/rc2barrington/ai-chat-exporter/blob/db8566fc158d6d4441c17c1fc9cbb921835f2824/package.json)、[仓库元数据](https://api.github.com/repos/rc2barrington/ai-chat-exporter)
- 本地应用要求 Node.js ≥22.15，提供选取、搜索和批量导出；可保存附件或只输出文本。再次导出时避免覆盖，生成新的文件名，因此不能直接作为稳定增量归档。[使用说明](https://github.com/rc2barrington/ai-chat-exporter/blob/db8566fc158d6d4441c17c1fc9cbb921835f2824/README.md)
- 按会话 ID 收集 Codex 多个 rollout，去除压缩与未压缩副本，逐段读入同一个解析器；`.jsonl.zst` 通过 Node 内置 zstd 解压，读取采用逐行方式。[会话发现与合并](https://github.com/rc2barrington/ai-chat-exporter/blob/db8566fc158d6d4441c17c1fc9cbb921835f2824/server/localSessions.js)、[压缩与逐行读取](https://github.com/rc2barrington/ai-chat-exporter/blob/db8566fc158d6d4441c17c1fc9cbb921835f2824/server/lineFiles.js)
- Codex 只解析主消息流，排除镜像事件和开发者指令；邻接的助手文本、推理和工具调用归并成一个回复。[Codex 解析](https://github.com/rc2barrington/ai-chat-exporter/blob/db8566fc158d6d4441c17c1fc9cbb921835f2824/src/parsers/codexJsonl.js)
- GitHub 元数据：2026-09-14 创建，最近 push 为 2026-11-08，调查时 0 stars。[仓库元数据](https://api.github.com/repos/rc2barrington/ai-chat-exporter)

## 其他检索结果

- [jinghan23/codex-export](https://github.com/jinghan23/codex-export)：Python，针对 Codex CLI/Desktop，缺少 Claude Code 支持，GitHub 未识别许可证，不作为统一实现首选。
- [simonw/claude-code-transcripts](https://github.com/simonw/claude-code-transcripts)：Python、Apache-2.0，主要生成 HTML，目标与两种来源统一生成 Markdown 不同。
- [grapeot/ai_session_export](https://github.com/grapeot/ai_session_export)：Python、多源增量 Markdown，功能方向吻合，但仓库未识别许可证；本次未继续源码验证，不列入推荐依赖。

## 后续验证范围

确定内置方案前，应以脱敏样本验证：Claude Code 和 Codex 各自的单段与长会话；归档压缩记录；同一会话多个 rollout；重复事件；分支与子代理；中文、代码块、图片；继续聊天后同一 Markdown 更新而不产生重复文档。先默认保存用户问题、助手回答与来源元数据，工具日志作为可选项。


## SAG 当前接入条件

已核对本仓库代码：

- `core/src/text.rs` 的 `MARKDOWN_EXTENSIONS` 支持 `.md`、`.markdown`，当前文件类型列表没有 `.jsonl`。直接添加聊天日志文件夹不能获得经过整理的对话正文。
- `core/src/indexer.rs` 的 `add_root` 注册目录并安排初次扫描，`event_requests` 在开启监听后处理后续变化。导出 Markdown 的目录可以复用现有索引流程。
- `core/src/service.rs` 的 `save_document` 已能保存文档并安排更新索引。内置转换不需要另建检索系统，但仍需增加来源发现、转换与导入入口。

### 建议范围

最快验证路径：用现成工具将选中的聊天导出为 Markdown，把导出目录加入 SAG，再验证检索效果。补查后，首次验证优先评估 SpecStory 的项目历史批量导出，界面内置优先参考 claude-code-history-viewer；unabridged 保留为小型转换器候选。

内置首版建议：在文件导入中提供“导入聊天”，选择 Claude Code 或 Codex、选择会话、预览正文、保存到知识库目录。默认保留问题、回答、代码块和可解析的图片；工具日志与推理内容按需导出。转换过程不需要调用模型；如将来增加知识摘要，应单独定义，不能替代原文。

同一来源与会话 ID 使用稳定文件名，保留来源、项目、时间和会话 ID。首版先做手动批量导入与重复导入更新，持续自动同步可以后续评估。若采用第三方源码，保留其许可证与版权说明。

内置建议基于 SAG 当前为 Rust 后端的结构：参考许可明确的解析规则，在后端完成小范围适配，可避免给用户新增 Node 安装要求；这属于实现取舍，尚未开发或验证。

### 成功标准

1. 两种来源均能从选择会话到生成 Markdown，再在 SAG 搜索到问题和回答。
2. 同一会话重复导入不增加重复文档，继续聊天后能更新既有文档。
3. 内部指令和镜像事件不混入默认正文；代码块保留；图片不可用时明确显示状态。
4. Codex 分段与压缩记录合并后，不遗漏已保存的用户与助手消息。
5. 原始聊天文件不变；转换失败显示具体文件和原因。

本次仅形成研究结论，未修改功能、界面或公开支持范围，因此没有改动用户文档或运行文档站构建。
