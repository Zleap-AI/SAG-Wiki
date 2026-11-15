# Claude Code History Viewer 复用评估与 SAG 聊天导入方案

核对日期：2026-11-15。固定上游版本：[67979b401e936ed503c941a3f3294012f51d730e](https://github.com/jhlee0409/claude-code-history-viewer/tree/67979b401e936ed503c941a3f3294012f51d730e)，v1.32.2。本机只读检出 `/tmp/sag-history-viewer.BzJdtQ`。

范围：上游源码核查、构造样本运行其 Markdown 原函数、对照 SAG 当前代码提出接入方案。本次未实现产品功能，未安装运行上游应用，未读取个人聊天记录。上游 GitHub push 时间与提交时间不同，本报告以固定提交为准。

## 结论

可以拆取许可明确的解析代码，但不能原样把整套导出接入 SAG。最有价值的是 Codex 事件转换、压缩记录读取、Claude 记录类型和跨文件续接。原 Markdown 导出服务于可读日志，包含截断、占位符及过滤遗漏，不是完整的知识库导入器。

MIT 允许复制和修改，需保留版权与许可。建议沿用本仓库已有第三方记录方式，新增 `third-party/claude-code-history-viewer/LICENSE` 与来源说明，列固定提交和修改范围。[上游许可](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/LICENSE)。这些是实施时需要新增的文件，本次未复制到产品源码。

## 可以复用的代码

| 上游位置 | 复用价值 | 接入时必须调整 |
| --- | --- | --- |
| `src-tauri/src/providers/codex.rs`：`is_rollout_jsonl`、`is_discoverable_rollout`、`read_rollout_bytes` | 识别普通/压缩记录，过滤压缩副本 | 保留读取规则；用流式读取取代整文件映射/整块解压，明确失败结果 |
| 同文件 `convert_codex_item`、`convert_codex_content_array`、`try_merge_tool_result_into_previous` | 文本、图片、工具事件标准化；工具结果关联 | 保留原始 role，默认排除 developer/system；补消息来源识别、新记录格式与多段合并 |
| `src-tauri/src/models/message.rs`：`RawLogEntry`、`MessageContent` | Claude 的字段名称、别名与原始记录结构 | 只取导入所需字段；不复制全部统计、计费及 UI 数据结构 |
| `src-tauri/src/commands/session/load.rs`：`parse_line_to_message`、`parse_line_simd` | Claude JSONL 解析、isMeta 等规则 | 抽出纯解析函数；不能复制整份 4623 行文件或静默丢弃解析错误 |
| `src-tauri/src/commands/session/chain.rs` | 根据 logicalParentUuid 查找 Claude 跨文件续接 | 补按父消息位置截取与重叠去重，防止分支包含后续无关消息 |
| `src/services/export/contentExtractor.ts`、`markdownExporter.ts` | 内容块分类、Markdown 编排；都是无界面的转换逻辑 | 移除 store 类型依赖；补统一过滤、图片落盘，取消默认截断与计费信息 |
| `src-tauri/src/export.rs`：`extract_blocks` 等 | 内容分类已有 Rust 实现，可以减少从 TS 重写的工作 | 当前输出为 HTML/JSON；抽取规则后另写小型 Markdown renderer，不复制 HTML、CLI 与路径查找部分 |
| 上述文件中的测试、`src/test/export/` | 标准事件样本及已知兼容行为 | 按 SAG 导入结果改成验收，不把上游有损输出当成正确目标 |

关键复用来源：[Claude 原始模型](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/models/message.rs)、[Claude 读取](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/load.rs)、[续接处理](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/chain.rs)、[内容提取 Rust 版本](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/export.rs)、[Markdown 输出](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/markdownExporter.ts)。

不能作为独立依赖直接引入整个应用：Codex 文件还依赖项目模型、搜索、标题数据库与重命名命令；Claude 读取依赖 Tauri、缓存、分页、rayon、simd-json。SAG 使用 Svelte，上游 React/Zustand 的 `useExport` 和界面不能直接复用。

## SAG 的最小接入方案

### 用户流程

在文件页现有“新建文档”“添加文件夹”旁增加“导入聊天”：

1. 选择 Claude Code 或 Codex，自动发现本机目录；允许指定已复制的记录目录。
2. 列出项目和会话，显示标题、时间，可多选；正文按需读取。
3. 选择会话时显示实际将保存的 Markdown。默认只保留问题、回答和可保存的附件；工具日志、思考过程为可选项。
4. 选择已有知识库目录并导入；显示成功、跳过、失败及具体原因，完成后可以直接打开文档。

第一版做手动批量导入与再次导入更新。自动监听、定时同步和新建独立聊天管理页不在第一版范围。

### 代码落点

建议在 `core/src/chat_import.rs` 集中实现来源发现、解析、清洗及 Markdown 生成，Claude/Codex 实现可分别放其子模块。外部只提供列出会话、预览、批量导入三个操作。预览和落盘使用同一份清洗逻辑，不能由前后端分别生成。

最小消息模型保留来源、会话 ID、消息 ID、父消息 ID、角色、时间、内容块以及文件位置。必须在清洗前保存来源字段；否则被转成 assistant 的 developer 消息无法可靠排除。文本转 Markdown 在 Rust 后端完成，不需要给用户增加 Node/Python 环境。

- `protocol/src/lib.rs`：增加三个操作及结果结构。
- `core/src/service.rs`：接入操作、保存文档、汇报逐项结果，复用已有索引队列。
- `src-tauri/src/lib.rs`：桌面调用入口。
- `src/components/LibraryWorkspace.svelte`、新建导入对话框：只做选择与预览。
- `src/lib/types.ts` 与中英文语言文件：类型和界面文案。

第一版按本机桌面功能交付，不把读取 `.claude`/`.codex` 的能力自动开放给远程 Web 请求。现有远程 API 使用同一个 protocol，实施时需确认新操作的准入范围。

### 现有能力怎样复用

`Store::save_document` 已有文件名校验、写入与修改冲突检查；`service::save_document` 已有索引更新队列，`indexer::add_root` 已有目录注册。导入器生成普通 `.md` 后可以直接复用这些流程，不新增检索索引，不把 `.jsonl` 加入普通文件类型。

现有 `conversions` 表假设一个已索引源文件对应转换文件，而聊天源位于知识库之外、会话可能跨多个文件，不适合直接套用。重复导入需要最少的来源记录：来源根目录、工具、会话 ID、输出路径、源内容指纹、上次输出哈希；优先存入已有 SQLite。输出文件用稳定会话标识命名，标题放正文，不因会话改名产生第二份文件。

再次导入：源没变则跳过；源有变化且本地 Markdown 未被编辑则更新；Markdown 被用户编辑则提示冲突，不覆盖。失败按会话记录，不吞掉错误。只有进行中的 JSONL 最后一行确实未写完时可报告“记录尚未写完”，不能把所有坏行都静默跳过。

附件保存为与 Markdown 相邻的目录和相对链接；缺失图片明确标记。默认保留问题、回答的原始文本和代码块；工具日志默认不写入，若选择保留则完整导出或明确标注截断，不能套用固定 500 字符。正文中的来源与会话信息保持简洁，内部同步字段可放 frontmatter；SAG 已会跳过 frontmatter 进行正文分块。

### 依赖与成本

SAG 已直接依赖 `serde_json`、`chrono`、`walkdir`、`dirs`、`base64`、`sha2`、`tempfile` 等。`zstd 0.13.3` 已在 `core/Cargo.lock` 中作为间接依赖，直接调用时仍需在 Cargo.toml 显式声明。不必为了复用上游增加 simd-json、memmap2 或整套 Tauri 插件；先用现有 JSON 解析与缓冲逐行读取。长会话解析在后台任务执行，避免阻塞界面。

## 清洗、截断和压缩的实际能力

## 已有清洗与去重

| 能力 | 实际行为 | 精确源码 |
| --- | --- | --- |
| 过滤内部消息 | 导出排除 `system`、`summary`、`progress`、`queue-operation`、`file-history-snapshot`；默认排除 `isSidechain` 子代理记录 | [contentExtractor.ts:231–237](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L231-L237) |
| Claude 元信息过滤 | 读取时跳过 `isMeta` 记录；summary 在一般正文读取中排除 | [load.rs:1880–1891](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/load.rs#L1880-L1891) |
| Codex 镜像事件去重 | 保留 `response_item` 正文；忽略 `event_msg.user_message/agent_message`，避免同一句导出两次 | [codex.rs:436–462](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L436-L462) |
| 工具结果归并 | `try_merge_tool_result_into_previous()` 将工具结果归入前条合适消息 | [codex.rs:445](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L445) |
| 内容类型开关 | text、thinking、toolCalls 可筛选；media/search 始终保留 | [contentExtractor.ts:248–263](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L248-L263) |
| 避免仅导出分页窗口 | export hook 有 `resolveMessages()`，可先取得完整会话再导出 | [useExport.ts:23–66](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/hooks/useExport.ts#L23-L66) |

这是结构与事件级过滤。没有发现按内容相似度合并重复讨论、删除失败方案、纠正已过时答案或提炼决策的语义清洗。

## 需要修正的清洗边界

1. **Codex 指令正文可能保留**。`convert_codex_item()` 把 role 为 user 之外的 `message` 统一标成 assistant，同时保留原 role。导出 `isExportable()` 只检查 `type`，没有检查原始 role，因此 developer/system 可被当成助手内容输出。搜索函数另有 role 过滤，不能把搜索行为视为导出行为。[codex.rs:1278–1295](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L1278-L1295)、[codex.rs:630–632](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L630-L632)。
2. **环境信息筛选仅用于标题**。`is_codex_auto_injected_user_text()` 识别 `<environment_context>`，调用位置在首次用户消息作为 session summary 的选取中；正文转换没有使用该筛选。因此不能声称正文已排除环境信息、AGENTS 等注入内容。[codex.rs:1164–1178](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L1164-L1178)、[codex.rs:1245–1253](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L1245-L1253)。
3. **工具结果过滤不完全**。Markdown 第 76 行只对 `msg.type === "assistant"` 应用内容过滤。Claude 的 `tool_result` 经常位于 user 消息，即使关闭 toolCalls 仍可能出现。`commands` 参数虽然存在，但过滤函数没有相应分支。[markdownExporter.ts:75–78](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/markdownExporter.ts#L75-L78)、[contentExtractor.ts:240–263](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L240-L263)。
4. **图片、文档不会随 Markdown 导出**。图片成为 `[Image]`；文档成为标题或 `[Document]`，不是复制附件并建立相对链接。[contentExtractor.ts:156–166](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L156-L166)。
5. **错误记录会跳过**。Codex 单行 JSON 解析失败直接 `continue`；一般 Claude 读取用 `.ok()?`。知识库导入应显示不完整或失败状态，避免把部分结果当成完整会话。[codex.rs:404–407](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L404-L407)、[load.rs:1881](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/load.rs#L1881)。

## 三种“压缩”的区别

### 磁盘压缩：支持读取，无内容摘要

Codex 读取 `.jsonl.zst` 使用 `zstd::decode_all`，恢复原始字节；同目录存在对应 `.jsonl` 时不重复发现 `.zst`。[codex.rs:82–145](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L82-L145)。

它解决文件格式兼容，不减少导出正文。解压使用整个内存缓冲，不是流式解压。

### 上下文压缩：读取已有事件，不执行压缩

Codex `compacted` 事件转换成 `Conversation compacted` 系统消息，只记录 replacement_history 的条目数，未提炼内容或恢复被替换的历史；这些系统标记又被 Markdown 导出排除。[codex.rs:1646–1673](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L1646-L1673)。

Claude 会话跨文件续接有实际支持：根据 `compact_boundary.logicalParentUuid` 向前找原文件，按旧到新读取，保留原始记录。它重建已有会话，未运行新的摘要生成。[chain.rs:1–13](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/chain.rs#L1-L13)、[chain.rs:209–261](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/chain.rs#L209-L261)、[load.rs:2011–2037](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/load.rs#L2011-L2037)。

### 导出截断：已有固定字符限制，会损失内容

- `summarizeInput()` 对工具输入每个字符串只保留前 120 字符，只输出字符串/布尔/数字字段；对象和数组不展开。这是字段抽取与截断，函数名不能理解为 AI 摘要。
- tool_result 和 MCP 结果最多 500 字符；代码执行 stdout 300、stderr 200；网页搜索最多列出前 5 个结果名称/地址。
- 用户和助手的普通 text 正文没有这类长度截断。

[contentExtractor.ts:16–31](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L16-L31)、[contentExtractor.ts:81–139](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L81-L139)、[contentExtractor.ts:189–205](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src/services/export/contentExtractor.ts#L189-L205)。

## 同一任务中的函数实测补充

主代理已在本机将上游 `exportToMarkdown` 和 `extractBlocks` 转译后，使用人工构造数据调用原函数，确认以下结果：system 类型排除；assistant 的 tool_use 能被过滤；user 类型里的 tool_result 仍输出；type=assistant、role=developer 仍输出；commands=false 仍可输出 command-name 标签；图片只输出占位；普通 1 万字符正文不截断；`password=...` 文本未脱敏。以上是人工样本的函数级实测，尚未验证真实会话端到端导入。

## 新版记录及跨文件完整性补充

- **Codex `thread_item` 未处理**：`parse_rollout_file()` 的顶层分支只有 session_meta、turn_context、response_item、event_msg、compacted，其余 `_ => {}`。因此输入若使用 thread_item 保存消息，此版本会忽略。这里确认的是解析器行为，尚未核验当前各 Codex 版本如何写入该记录。[codex.rs:416–534](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L416-L534)。
- **Codex 同一会话多段文件未合并**：load_sessions 对每个 rollout 生成独立记录，session_id 使用文件路径；load_messages 只读传入的一个路径。`.jsonl` 与同目录同名 `.zst` 去重已有，但没有看到按实际会话 ID 收集多段并串联的行为。[codex.rs:316–378](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/providers/codex.rs#L316-L378)。
- **Claude 跨文件只连接，不做消息 UUID 去重或分支截取**：链解析器的 visited 集合防止文件循环；`load_session_messages()` 对各文件的结果执行 `messages.extend(file_messages)`，没有 seen-message-ID 集合。`logicalParentUuid` 用于查找先前文件，不用于截取先前文件到对应消息。对于正常无重叠自动续接可用；回放重叠和从中途分支的情况需额外验证。不能将“支持 continuation 链”描述成“支持完整分支去重”。[chain.rs:209–246](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/chain.rs#L209-L246)、[load.rs:2015–2024](https://github.com/jhlee0409/claude-code-history-viewer/blob/67979b401e936ed503c941a3f3294012f51d730e/src-tauri/src/commands/session/load.rs#L2015-L2024)。


## 内容摘要的建议

上游没有可直接拿来用的知识摘要流程。SAG 已有 `core/src/document_fold.rs` 的多层阅读生成，聊天导入为 Markdown 后可以沿用“折叠”来阅读简述、要点与卡片；这不会默认生成一篇独立知识笔记，也不会自动排除聊天中后来被推翻的结论。

如果后续要“提炼知识”，单独提供用户触发的整理动作：基于清洗后的问答生成结论与步骤，附原聊天来源；保留完整问答，不覆盖成摘要。导入和规则清洗不调用模型。摘要生成会使用用户配置的模型，属于另一个步骤。

## 验收标准

- CC 和 Codex 各有普通、继续对话、分支、多文件与压缩记录样本；消息顺序完整且不重复。
- 不混入 developer/system、环境和 AGENTS 等来源明确的内部指令；用户正常正文中的类似文字不能一律正则删除。
- 无论外层角色如何，工具调用和工具结果均遵从同一开关；预览与保存一致。
- 中文、长文本与代码块保真，附件可打开或明确显示缺失。
- 重复导入不增重复文件，源更新可更新，用户修改产生冲突提示。
- 解析失败、未知记录、缺段可见；不能把不完整结果无提示地标为成功。
- 从界面选取、预览、保存到 SAG 搜索命中与引用原文完成端到端检查。

实施时同步 `docs-site/content/docs/` 中英文件操作文档、截图，并执行文档站构建。本次为研究文档，未修改公开功能说明，未运行产品或文档站构建。
