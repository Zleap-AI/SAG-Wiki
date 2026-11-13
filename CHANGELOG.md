# 更新日志 / Changelog

## 0.2.12

- **基础文字先可搜索**：PDF 先发布可提取的文字，混合或扫描 PDF 在后台继续 OCR 和表格增强；文件列表明确显示可搜索范围及增强状态。纯文字 PDF 保持快速完成。
- **大 PDF 不再挡住新文件**：基础索引和增强使用独立工作线程，本地内置解析支持的 Office、RTF 等文件可先完成文字索引；云解析沿用原配置顺序。增强失败保留已有可用文字，并支持只重试失败文件。
- **减少 PDF 重复加载模型**：同一 PDF 的多个分段复用 Docling 进程和模型，文档完成、失败或取消后释放；保持现有 CPU 线程策略、OCR 和表格识别配置。修复并行扫描误删或覆盖新增强结果的问题。
- **更可靠的索引进度与恢复**：分别显示基础索引、后台增强和 PDF 分段进度，减少状态闪动；未变化的失败文件复用失败记录，文件、组件或相关预算变化后重新尝试。
- **改善 CLI 与问答兼容性**：`sag add` 支持用 `--timeout` 调整等待时间，超时明确提示后台仍在索引；决策模型批次因问题条数超限被拒绝时自动拆批重试。修复数学区间被误判为引用、金额内容被数学表达式过滤的问题；切换决策模型提供商时重置模型选择。
- **改善组件安装与分发**：调整文档组件安装内存限制，保留失败诊断，正确识别 Windows 解析进程在启动阶段超过内存上限的情况；继续精简 QPDF 分发内容。

桌面端与随包 CLI 版本统一为 0.2.12。基础文字可搜索不代表扫描内容和复杂表格已全部完成，请以界面状态及原文核对完整性。

### English

- **Search basic text sooner**: publish usable PDF text before background OCR and table enhancement for mixed or scanned PDFs. File-list states identify search availability and enhancement progress. Text-only PDFs retain fast completion.
- **Keep new files moving**: independent basic-indexing and enhancement workers let supported local Office, RTF and other built-in formats become searchable while a large PDF is enhanced. Cloud parsing retains its configured order. Enhancement failures retain usable text, with selective failed-file retry.
- **Reuse PDF models**: share one Docling process and its models across a PDF's segments, releasing resources on completion, failure or cancellation. Keep CPU thread policy and OCR/table settings unchanged. Fix concurrent scans deleting or overwriting newly enhanced indexes.
- **Clearer progress and recovery**: show separate basic, enhancement and PDF-segment progress with more stable updates. Reuse unchanged failure records and retry when files, components or relevant budgets change.
- **CLI and answer compatibility**: `sag add --timeout` controls the wait and explains when indexing continues after timeout. Split decision-model batches rejected for exceeding question-count limits. Avoid treating mathematical intervals as citations or stripping prices as mathematical expressions, and reset the model selection when switching decision-model providers.
- **Component installation and distribution**: adjust document-component installation memory limits while retaining diagnostics, correctly report Windows parser memory-limit failures during startup, and further reduce the QPDF distribution payload.

The desktop app and bundled CLI both use 0.2.12. Searchable basic text does not imply complete scanned-content or complex-table coverage; check the displayed state and original document.

## 0.2.11

- **长 PDF 分段转换与恢复**：按最多 50 页分段，每段独立尝试 MinerU、适用的内置解析器和已安装的 Docling。失败只降级当前分段，成功分段缓存复用，全部完成后才合并入库；停止或失败不会发布残缺正文。
- **减少长文档等待**：使用随包 QPDF 拆页，避免克隆并逐页删除整份 PDF；明确没有文字层的扫描分段尽快交给 OCR。恢复云端任务时从首次提交计算等待期限，避免重试反复等待同一过期任务。
- **保留 MinerU 文档图片**：ZIP 结果中的图片保存到本地，阅读器通过相对链接显示；检索保留描述和图注，过滤图片资源地址。兼容引用式图片、嵌套或转义的图片描述以及带空格的图片路径。
- **改善打包与 Windows 兼容性**：桌面端和独立 CLI 携带 QPDF，修复迁移安装目录后依赖失效、npm 包遗漏 QPDF、Windows 临时文件占用和规范化路径处理问题。
- **精简 QPDF 打包内容**：移除重复动态库、未使用工具和 Windows 开发文件，保留运行所需依赖与许可证，减少桌面端和独立 CLI 的分发体积。
- **区分失败文件重试进度**：索引详情明确显示正在重试失败文件，避免与正常转换进度混淆。

桌面端与随包 CLI 版本统一为 0.2.11。

### English

- **Resumable long-PDF conversion**: process PDFs in parts of at most 50 pages, independently trying MinerU, the applicable built-in parser, and installed Docling. Only failed parts fall back; successful parts are cached. Merge and index the complete document only after every part succeeds, without publishing partial output after failure or cancellation.
- **Less waiting on long documents**: use bundled QPDF for page selection instead of cloning and deleting pages from the whole PDF. Hand scanned parts with no text layer off to OCR promptly. Resumed cloud tasks keep their original submission deadline instead of restarting the wait for an expired task.
- **Preserve MinerU document images**: save ZIP result images locally for the reader to display through relative links. Search retains descriptions and captions while filtering image resource addresses. Support reference images, nested or escaped image descriptions, and image paths containing spaces.
- **Packaging and Windows compatibility**: bundle QPDF with the desktop app and standalone CLI. Fix dependencies that broke after installation relocation, missing QPDF in npm packages, open temporary file handles on Windows, and canonical path handling.
- **Smaller QPDF payload**: remove duplicate shared libraries, unused utilities, and Windows development files while retaining runtime dependencies and licenses, reducing distribution size for the desktop app and standalone CLI.
- **Distinct failed-file retry progress**: index details distinguish retries of failed files from normal conversion progress.

The desktop app and bundled CLI both use 0.2.11.

## 0.2.10

- **新增 HTTP API**：本机和局域网程序可通过 API 进行快速或深入检索、浏览文件夹，并按完整路径指定一个或多个搜索目录。API 与桌面端、CLI 共用知识库、索引和决策模型配置，结果按原文片段返回。
- **API 服务设置与 CLI 命令**：桌面端新增 **设置 → API 服务**，可开关服务、查看本机与局域网地址、管理 Key 和开机自启；独立 CLI 新增 `sag api start/stop/status`、`sag api key create/list/revoke` 和 `sag api autostart` 命令。
- **独立 CLI 支持 Linux**：npm 包新增 Linux ARM64 / x86_64，已在 Ubuntu 24.04 和 Debian 12 上验证，无需桌面端即可运行搜索和 API 服务。
- **支持 Responses API**：问答模型地址填写完整的 `/v1/responses` 接口即可启用 Responses 流式回答，支持多轮工具调用携带推理状态与调用标识，思考强度使用 `reasoning.effort`。仅填写 `/v1` 仍使用原来的 Chat Completions 接口。
- **超长 PDF 不再因页眉页脚转换失败**：超过 200 页的 PDF 拆分解析后，MinerU 返回的页眉、页脚和页码会被过滤，代码块保留语言、标题和脚注，图表保留说明和脚注；遇到无法识别的内容会提示类型和原始页码，而不是笼统失败。
- **索引时显示当前使用的解析方式**：转换过程中，索引状态栏和索引详情会显示 MinerU、anydoc 或 Docling。
- **改善长对话的摘要重试**：上下文压缩时，摘要请求在流式响应中断或超时后按配置重试，必要时缩小当前分块；已完成的摘要分块保留，界面显示重试进度。
- **修复转换文档的 CLI 检索**：PDF、CSV 等转换后的文档此前不计入原文目录的 `sag check` 结果，`-F` / `-E` 也搜不到；现在按源目录正确计数和匹配。
- **修复 Windows 匹配结果路径**：命令行匹配结果统一使用 `/` 分隔符，不再显示反斜杠。
- **Jev 更名为决策模型**：设置页与文档统一使用“决策模型”名称；切换到 Zleap 时自动填入 `Zev-1`。
- **检索结果按相关性排序**：深入检索在本地按相关性分数排序返回，不再进行 DPP 多样性重排。
- **改善符号链接目录的索引**：知识库根目录变为符号链接后，仍按原有路径识别已索引内容，已移除的目录会明确提示重新添加。

桌面端与随包 CLI 版本统一为 0.2.10。

### English

- **HTTP API**: programs on this computer or the local network can run fast or deep searches, browse folders and limit a search to one or more folders by full path. The API shares the knowledge base, index and decision-model settings with the desktop app and CLI, and returns source passages.
- **API service settings and CLI commands**: the desktop app adds **Settings → API service** to turn the service on or off, show local and LAN addresses, and manage keys and launch at startup. The standalone CLI adds `sag api start/stop/status`, `sag api key create/list/revoke` and `sag api autostart`.
- **Standalone CLI on Linux**: the npm package adds Linux ARM64 and x86_64, verified on Ubuntu 24.04 and Debian 12, so search and the API service run without the desktop app.
- **Responses API support**: entering a complete `/v1/responses` endpoint for an answer model enables streamed Responses answers, including reasoning state and call identifiers across tool rounds, with effort set through `reasoning.effort`. A plain `/v1` address keeps using Chat Completions.
- **Long PDFs no longer fail on running headers and footers**: after split parts are merged, MinerU's headers, footers and page numbers are dropped, code blocks keep their language, caption and footnote, and charts keep their description and footnote. Unrecognised content now names its type and original page instead of failing generically.
- **Active parser shown during conversion**: the index status bar and index details show MinerU, anydoc or Docling while files convert.
- **Reliable summary retries in long conversations**: when a summary request is interrupted mid-stream or times out, context compaction retries within the configured limits and shrinks the current chunk if needed, keeping chunks already summarised and showing retry progress.
- **CLI search over converted documents**: documents converted from PDF, CSV and similar formats were missing from `sag check` counts and `-F` / `-E` results under their source folder; they are now counted and matched there.
- **Windows match paths**: command-line match results use `/` separators instead of backslashes.
- **Jev renamed to decision model**: Settings and the documentation use "decision model" throughout; switching to Zleap fills in `Zev-1` automatically.
- **Results ordered by relevance**: deep search returns results sorted by local relevance score, without DPP diversity reranking.
- **Indexing symlinked folders**: when a knowledge-base root becomes a symbolic link, indexed content is still recognized under its original path, and removed folders ask to be added again.

The desktop app and bundled CLI both use 0.2.10.

## 0.2.9

- **修复 Windows 快捷搜索崩溃**：Windows 版此前未创建快捷搜索窗口，通过全局快捷键、托盘菜单或快捷键设置打开快捷搜索会导致应用退出；现已修复，缺少窗口时报错而不再崩溃。
- **改善 Windows 启动失败提示**：本地服务启动失败时显示完整原因（含系统错误码），不再只显示“无法检查 SAG 服务构建”；检查服务、配置 PATH 和目录权限时不再依赖 NUL 设备。
- **减少重启后的重复索引**：因未安装文档组件而无法转换的文件（如需要 OCR 的扫描 PDF）在组件安装或文件变化前不再逐次重新解析；未变化的 PDF 与 Office 文件按修改时间和大小跳过，不再每次完整读取计算哈希。手动重新索引仍会完整校验。
- **修复离线组件导入入口**：同时勾选文档与媒体格式时，下载失败后仍显示“导入离线组件”按钮。
- **Jev 默认使用 Zleap 决策模型**：首次启动默认选择 Zleap 开放平台的 `Zev-1`，设置页可直接打开控制台获取 API Key；已有用户的配置保持不变。
- **改善模型连接测试**：问答模型测试不再附带部分服务不支持的输出长度限制。
- **对话续问保持系统提示词在前**：引用检查、空回答检查和工具调用上限提醒后的再次请求均保持系统提示词位于开头。

桌面端与随包 CLI 版本统一为 0.2.9。

### English

- **Windows quick-search crash**: the Windows build did not create the quick-search window, so opening quick search from the global shortcut, the tray menu or the shortcut settings exited the app. Fixed; a missing window now reports an error instead of crashing.
- **Windows startup diagnostics**: when the local service fails to start, show the full cause including the OS error code instead of only "cannot check the SAG service build". Service checks, PATH configuration and directory permissions no longer depend on the NUL device.
- **Less re-indexing after restart**: files that could not be converted because the document component is missing (such as scanned PDFs needing OCR) are not parsed again until the component is installed or the file changes. Unchanged PDF and Office files are skipped by modification time and size instead of being read in full to hash on every scan. Manual re-index still verifies everything.
- **Offline component import entry**: the "Import offline component" button stays visible after a download failure when document and media formats are selected together.
- **Zleap decision model as the Jev default**: first launch selects `Zev-1` from the Zleap Open Platform, and Settings links to the console for an API key. Existing configurations are unchanged.
- **Model connection tests**: the chat-model test no longer sends an output-length limit that some services reject.
- **System prompt stays first in follow-ups**: the system prompt remains at the start of re-requests after citation checks, empty-answer checks and tool-call limit reminders.

The desktop app and bundled CLI both use 0.2.9.

## 0.2.8

- **新增 MinerU 文档解析**：支持 302.AI 和 MinerU 官方服务。配置密钥后优先使用云端解析，失败时尝试适用的本地解析器；302.AI 当前仅支持 PDF。启用云端解析会上传原文档。
- **改善长 PDF 处理**：支持按服务限制自动分段，成功后合并正文并保留原始页号；中断后可续查已有任务，减少重复提交。
- **开放运行参数设置**：可在设置页调整转换超时、内存预算、文件大小与页数限制，以及模型重试、搜索并发、缓存和文档折叠参数。保存后对新任务生效。
- **大文件夹后台索引与进度展示**：添加大文件夹后即可继续操作，文件监听准备与索引在后台完成；显示排队、扫描、转换和索引阶段，方便查看进度与失败原因。
- **提高索引监听稳定性**：修复根目录变化可能引发的监听异常，改善目录恢复、重新添加后的自动更新；移除文件夹时及时停止相关任务。
- **提高图片文字识别效率**：连续处理图片时复用已加载的 OCR 模型，减少重复加载；空闲后自动释放资源，并遵守设置中的超时和内存预算。
- **支持手动导入文档识别组件**：下载失败时，可导入完整离线文档组件或解压后的 uv 目录；完善手动下载地址及镜像配置提示。
- **修复 Windows 本地服务启动问题**：修复部分电脑因账户权限识别错误，导致无法保存连接记录、无法连接本地后台服务的问题。
- **改善文件阅读与切换体验**：点击侧栏文件直接切换内容，减少列表闪现；修复快速切换时旧内容覆盖新选择的问题，读取失败时保留当前阅读内容并提示错误。

桌面端与随包 CLI 版本统一为 0.2.8。

### English

- **MinerU document parsing**: support 302.AI and the official MinerU service. With a key configured, supported files use cloud parsing first and fall back to an applicable local parser on failure. 302.AI currently supports PDF only. Enabling cloud parsing uploads the original document.
- **Long PDF handling**: split PDFs according to service limits, merge successful results and retain original page numbers. After an interruption, check existing tasks to reduce duplicate submissions.
- **Runtime settings**: adjust conversion timeouts, memory budgets, file-size and page limits, model retries, search concurrency, caches and document-folding parameters in Settings. Saved changes apply to new tasks.
- **Background indexing and progress**: keep using the app after adding a large folder while file monitoring and indexing are prepared in the background. Show queueing, scanning, conversion and indexing stages, with progress and failure details.
- **File monitoring reliability**: fix monitoring errors triggered by root-directory changes and improve automatic updates after directories are restored or added again. Removing a folder promptly stops its related tasks.
- **Faster image text recognition**: reuse loaded OCR models across consecutive images, release idle resources automatically and respect the configured timeout and memory budgets.
- **Manual document-component import**: when downloads fail, import a complete offline document component or an extracted uv directory. Improve manual-download links and mirror-configuration guidance.
- **Windows local-service startup**: fix account-permission resolution on affected computers that prevented saving connection records and connecting to the local background service.
- **File reading and switching**: open sidebar files directly with less list flicker. Prevent older reads from replacing the latest selection; keep the current document visible and show an error when reading fails.

The desktop app and bundled CLI both use 0.2.8.

## 0.2.7

- **文档折叠**：Markdown 支持全文、简述、关键句、要点、卡片和导图六个阅读标签，保留原文供编辑。
- **后台处理与缓存**：关闭折叠面板后继续处理，支持查看进度、主动停止和复用已完成的结果。
- **长文档索引**：改进超长表格行、Windows 换行和数字内容的处理，保留完整原文及对应位置。

桌面端与随包 CLI 版本统一为 0.2.7。

### English

- **Document folding**: Markdown offers Full, Brief, Key, Takeaway, Cards and Map tabs while keeping the source editable.
- **Background processing and caching**: processing continues when the fold panel closes, with progress, explicit cancellation and reuse of completed results.
- **Long-document indexing**: improve handling of long table rows, Windows line endings and numeric content while preserving complete source text and locations.

The desktop app and bundled CLI both use 0.2.7.

## 0.2.6

- **快捷搜索**：新增独立的快捷搜索窗口，默认用 macOS 的 ⌘⇧Space、Windows 的 Ctrl+Shift+Space 在其他应用中唤起，也可从菜单栏或托盘打开。输入即在本地索引中搜索，最多列出 10 个文件；↑ ↓ 选择、Enter 在主窗口打开原文、⌘ / Ctrl+Enter 查看全部结果、Esc 关闭并返回原应用。设置中的“快捷键”页可启用、关闭或改键，搜索首页显示当前快捷键。
- **跨语言检索**：问答要求模型在同一次自然语言检索中同时给出中英文等义表述并保留原问题的条件与限定，中文提问默认用中文回答；精确文字、代码和文件路径不翻译。
- **工具调用默认不限制**：每轮工具调用上限的默认值由 10 次改为 0（不限制），仍可设为 1–100 限制 SAG 与 MCP 工具的总调用次数。
- **内置文档默认纳入索引**：文本型 PDF、Office、RTF、CSV、EPUB 等内置解析格式统一算作文本类型，默认索引且无需下载转换组件；`sag add --types` 可直接选择这些扩展名，是否转换沿用文件夹的文件类型设置，旧转换缓存继续复用。
- **本机 Skill 接入**：后台服务在同一端口提供 `/sag-wiki.md` 及其 `references/` 文档；CLI 设置的“其他”一栏改为复制 `set up: http://127.0.0.1:端口/sag-wiki.md` 指令，由 Agent 直接读取，不再下载 Skill 文件夹。
- **转换与索引内存预算**：Docling 转换按物理内存计算预算，全部任务合计为一半、单任务为四分之一，低内存机器尽量保留 4 GiB 的 OCR 运行空间，读取物理内存失败时给出明确错误；文本准备预算提高到 512 MiB 且单文件至少预留 64 MiB，长标题与段落密集的文档可以正常索引。
- **HTML 与本地图片**：搜索预览、快捷搜索和引用原文会渲染 HTML 表格（保留合并单元格与加粗）、段落和标题对齐、链接与图片尺寸；相对路径的本地图片支持 PNG、JPEG、GIF、WebP 和 SVG，编辑仍保留原始 HTML。
- **文档**：文档站补充快捷搜索、HTML 表格与本地图片、转换内存与索引预算、跨语言问答、MCP 添加服务流程和 Skill 接入说明，并更新中英文截图。

桌面端与随包 CLI 版本统一为 0.2.6，沿用现行更新签名通道；本次更新不涉及数据库迁移。

### English

- **Quick Search**: add a dedicated popup opened from other apps with ⌘⇧Space on macOS or Ctrl+Shift+Space on Windows, or from the menu bar and tray. Typing searches the local index and lists up to ten files; use ↑ ↓ to select, Enter to open the passage in the main window, ⌘ / Ctrl+Enter for all results and Esc to close and return to the previous app. Settings → Shortcuts can enable, disable or rebind the shortcut, and the search home page shows the current one.
- **Cross-language retrieval**: Q&A now asks the model to include equivalent Chinese and English wording in the same natural-language search while keeping the original constraints, and answers Chinese questions in Chinese. Exact text, code and file paths are not translated.
- **Unlimited tool calls by default**: the per-turn tool-call limit now defaults to 0 (unlimited) instead of 10; 1–100 still caps combined SAG and MCP calls.
- **Built-in documents indexed by default**: text-based PDF, Office, RTF, CSV and EPUB files count as text, are indexed without downloading a conversion component, and can be selected with `sag add --types`. Per-folder file-type settings still decide conversion, and existing conversion caches keep working.
- **Local Skill setup**: the background service now serves `/sag-wiki.md` and its `references/` on the same port. The "Other" row in CLI settings copies a `set up: http://127.0.0.1:PORT/sag-wiki.md` instruction for the agent to read, replacing the Skill folder download.
- **Conversion and indexing memory budgets**: Docling conversions budget half of physical memory in total and one quarter per job, keep 4 GiB for OCR on smaller machines and report a clear error when physical memory cannot be read. Text preparation now allows 512 MiB overall with at least 64 MiB per file, so long headings and paragraph-dense documents index reliably.
- **HTML and local images**: search previews, Quick Search and citations render HTML tables with merged cells and bold text, paragraph and heading alignment, links and image sizes. Local images load from relative paths, including PNG, JPEG, GIF, WebP and SVG, while editing keeps the original HTML.
- **Documentation**: the documentation site covers Quick Search, HTML tables and local images, conversion memory and indexing budgets, cross-language Q&A, adding MCP servers and Skill setup, with refreshed Chinese and English screenshots.

The desktop app and bundled CLI both use 0.2.6, retaining the current update-signing channel. This update requires no database migration.

## 0.2.5

- **搜索总结**：有结果且回答模型可用时默认展示自动总结，支持从原文返回已有总结。
- **原文定位**：Markdown 按所选片段的来源位置高亮段落，语义结果不含搜索原词时也能定位；切换文件后滚动到所选片段。
- **HTML 高亮修复**：使用段落轮廓标记，保留原文的文字和背景色，避免白底白字等对比度问题。
- **定时更新检查**：运行期间每隔 8 小时检查新版，发现更新后可由用户选择下载与安装。
- **开发窗口刷新修复**：文档、缓存及扩展构建目录的变化不再触发桌面开发窗口刷新。
- **macOS 签名与公证**：发布流程为应用、包内 CLI 和 DMG 接入 Developer ID 签名、Apple 公证及票据装订，并增加最终安装包校验。

桌面端与随包 CLI 版本统一为 0.2.5，沿用现行更新签名通道；本次更新不涉及数据库迁移。

### English

- **Search summaries**: open the automatic summary by default when results and an answer model are available, and return to an existing summary from its source.
- **Source navigation**: highlight Markdown paragraphs using the selected passage's source position, including semantic results without the original search words. Scroll to the selected passage when switching files.
- **HTML highlight fix**: outline the selected paragraph while preserving source text and background colors, preventing white-on-white text and other contrast problems.
- **Periodic update checks**: check for updates every eight hours while the app is running, with downloads and installation controlled by the user.
- **Development reload fix**: changes to documentation, caches and extension build output no longer reload the desktop development window.
- **macOS signing and notarization**: add Developer ID signing, Apple notarization, stapled tickets and final-package verification to the release workflow for the app, bundled CLI and DMG.

The desktop app and bundled CLI both use 0.2.5, retaining the current update-signing channel. This update requires no database migration.

## 0.2.4

- **CLI 连接反馈**：测试成功后默认收起详情，支持展开查看；在设置分类间切换时保留最近结果和展开状态。
- **连接状态更新**：再次测试清除旧结果并防止重复请求；关闭 CLI 时清除缓存状态。成功徽标停留后淡出，适配明暗主题与减少动态效果偏好。
- **模型设置**：LLM 与 Jev 的“测试连接”按钮统一放在卡片右下角。
- **Windows 窗口控制**：将最小化、最大化／还原和关闭按钮恢复到左侧栏顶部，移除独立顶部标题栏；侧栏收起时仍可操作，macOS 继续使用原生窗口控件。

桌面端与随包 CLI 版本统一为 0.2.4，沿用现行更新签名通道；本次更新不涉及数据库迁移。

### English

- **CLI connection feedback**: collapse successful connection details by default, with an option to expand them. Preserve the latest result and disclosure state when switching settings categories.
- **Connection state updates**: clear stale results before retesting, prevent duplicate requests and clear cached feedback when CLI access is disabled. The success badge fades away, with light/dark theme and reduced-motion support.
- **Model settings**: align the LLM and Jev connection-test buttons at the bottom right of their cards.
- **Windows window controls**: restore minimize, maximize/restore and close to the top of the sidebar, removing the separate title bar. Keep controls available with the sidebar collapsed; macOS retains its native window controls.

The desktop app and bundled CLI both use 0.2.4, retaining the current update-signing channel. This update requires no database migration.

## 0.2.3

- **文件夹侧栏**：恢复首版直接展示文件夹的布局，移除会话／文件夹切换，保留文件夹导入。
- **问答首页**：恢复会话列表、时间与历史搜索；会话内可返回列表，生成或取消中的回答受到保护。
- **布局修复**：收起侧栏后仍可浏览首页会话列表。
- **文档截图**：使用当前界面重新生成中英文桌面截图，同步 README 与文档站。

桌面端与随包 CLI 版本统一为 0.2.3。

### English

- **Folder sidebar**: restore the original folder-first layout, remove the conversation/folder switch and retain folder import.
- **Q&A home**: restore conversation history, timestamps and search. Return to the list from a conversation, with active generation and cancellation protected.
- **Layout fix**: keep conversation history visible when the sidebar is collapsed.
- **Documentation screenshots**: regenerate bilingual desktop screenshots from the current UI and synchronize the READMEs and documentation site.

The desktop app and bundled CLI both use 0.2.3.

## 0.2.2

相对 0.2.1 的更新 / Changes since 0.2.1.

### 新功能与改进

- **可核对笔记**：将已完成的回答保存为 Markdown，保留实际引用及保存时的原文；支持修改文件名、选择目录和记住保存位置。
- **引用状态**：来源变化后在本地核对引用，区分一致、位置更新、内容修改、缺失和未核对；支持查看词级对照、打开原文及筛选待核对笔记，无需调用模型。
- **搜索与阅读**：结果按文件归组，原文使用更大的阅读区域；新增相关片段切换、文内查找、专注阅读和可调整的列表宽度，并记住阅读位置。
- **大文档预览**：优先显示目标段落，分批渲染其余内容、按需加载图片，减少打开长文档与跳转引用时的等待。
- **双语文档**：刷新 README 和文档站的桌面截图，同步搜索、问答、索引与 MCP 操作说明。

### 升级说明

桌面端与随包 CLI 统一为 0.2.2，适用于 macOS 和 Windows，沿用现行更新签名通道。旧会话保持兼容；笔记保存在本地 Markdown 中，核对需要来源文件可访问并已加入知识库。

### Features and improvements

- **Verifiable notes**: save completed answers as Markdown with the citations actually used and their original passages. Edit the file name, choose a folder and remember the save location.
- **Citation status**: locally check sources for unchanged, moved, edited, missing or unverified passages. Compare changes word by word, open sources and filter notes needing attention, without calling a model.
- **Search and reading**: group results by file and give source text more reading space. Navigate related passages, find text within a document, enter focus mode and resize the result list, with reading positions preserved.
- **Large-document previews**: show the target passage first, render remaining content progressively and load images as needed to reduce waits when opening long documents or following citations.
- **Bilingual documentation**: refresh desktop screenshots in both READMEs and the documentation site, with updated search, Q&A, indexing and MCP instructions.

### Upgrading

The desktop app and bundled CLI both use 0.2.2 for macOS and Windows, retaining the current update-signing channel. Existing conversations remain compatible. Notes are local Markdown files; verification requires accessible source files registered in the knowledge base.

## 0.2.1

本次版本包含桌面端体验更新。

### 新功能与体验改进

- 合并问答会话与文件夹导航，支持按日期浏览会话、搜索标题和回答，并提供更紧凑的侧栏与新建会话入口。
- 优化索引状态与历史详情：按文件夹汇总最近索引、折叠历史记录，并用易读说明呈现失败原因和受影响文件。
- 改进问答过程展示与输入体验：工具调用折叠为研究步骤，回答生成时仍可编辑输入框，优化引用编号、进度和耗时展示。
- 重新设计引用原文侧栏，突出文件信息与命中段落，改善匹配段落切换和长路径显示。
- 收紧取消后的事件处理并修复会话历史刷新覆盖新回答的问题；优化空知识库及无匹配文件时的 `@` 引用提示。

### 发布与兼容性

- 桌面端和随包 CLI 统一为 0.2.1，沿用现行应用内更新签名通道和安装迁移要求。
- GitHub 发布范围仍为 macOS 和 Windows 桌面端；PR 执行两平台 CLI/服务 E2E，完整构建、单元测试、文档构建与安装器检查由桌面发布工作流执行。

## 0.2.0

相对 0.1.9 的桌面端与 CLI 更新。

### 新功能与体验改进

- 内置 anydoc 文档转换，文本型 PDF、Word、PowerPoint、Excel/CSV、OpenDocument 和 EPUB 无需另行下载转换组件。扫描件仍需按需安装的 OCR 组件；缺少组件时会提示索引问题。
- 新增网页剪藏扩展和桌面导出入口，可预览网页正文后保存为本地 Markdown，并将保存目录加入知识库。支持 Chrome / Edge；Safari 导出仅在桌面构建附带已签名、公证的扩展安装包时可用。
- 新增文档标题目录、当前文档问答、首次保存时命名与选择目录，并改善转换后文档的打开、编辑和文件列表展示。
- 文档编辑器、引用原文、搜索预览和回答支持本地 Mermaid 图表；问答处理过程可自动折叠并显示用时。
- 长会话超出上下文预算时，使用当前问答模型生成并保存摘要，保留完整历史。每轮工具调用仍默认 10 次，新增设置为 0 取消次数限制，SAG 与 MCP 工具共用该上限。
- 改善 MCP 工具绑定：添加服务和导入 JSON 使用独立弹窗，明确显示待测试、已连接及当前模型启用状态，新增移除确认与撤销。
- 新增带操作截图的中英文文档站，涵盖文档编辑、网页剪藏、检索、问答、CLI 与 MCP 接入。

### Bug 修复

- 改善未配置模型时的首次使用：默认使用本地快速搜索，跳过需要回答模型的自动摘要，并为语义检索与问答提供配置提示。
- 后台服务启动连接失败时显示可操作错误并重连，恢复后清除临时错误；事件消费者落后时保持事件流连接，避免后续进度和问答事件中断。
- 索引时跳过不可读子目录，继续处理其余文件；目录中的 `.sagignore` 无法读取时停止该范围的扫描，避免忽略规则失效。
- 检索跳过已删除、已变化或不可读的原文，以及超过 Jev 单项输入预算的候选，避免单份文档导致整个查询失败；搜索数量扣除当前页跳过的失效项。
- 修复 Windows 转换输出的 UTF-8 读取及 uv 目录联接处理，并调整 8 GiB 设备的转换内存预算，避免原有 2 GiB 限额过早终止转换。
- 转换组件安装显示实际进度，模型下载源不可达时及时报错；“其他格式”默认不预选，按所选格式安装文档/OCR 或音视频组件。
- 内置转换缓存纳入 anydoc 版本，后续索引时更新旧缓存；原文未变时保留失败重试前的有效结果，避免覆盖用户编辑过的转换文档，并防止原文变化后继续使用旧结果。
- 外部链接改用系统浏览器打开，避免打断应用内会话；手动上滚阅读回答后，生成完成不再将阅读位置拉回底部。
- 已关闭的相同索引失败提示不再重复出现，新的失败仍会提醒，并改善 Windows 索引路径显示。

### 发布与兼容性

- 桌面端和随包 CLI 统一为 0.2.0，沿用 0.1.9 的更新签名通道。0.1.8 及更早版本的签名迁移要求不变。
- GitHub 发布范围为 macOS 和 Windows 桌面端。PR 执行两平台 CLI/服务 E2E；完整构建、单元测试、文档构建与安装器检查由桌面发布工作流执行。

## 0.1.9

- 支持本地 Jev 的基础地址及完整接口地址，修复重复拼接 `/systemone` 导致连接失败的问题。
- Jev 新增“无需鉴权”选项，桌面与 CLI 均可接入无 API Key 的本地服务，旧配置保持 API Key 鉴权。
- 保留跨供应商密钥保护，修复无效配置保存可能删除原 Jev 密钥的问题。
- 新增集中签名的 macOS/Windows 发布流水线；更新签名密钥迁移后，旧版本用户需手动安装一次 0.1.9，之后恢复应用内更新。
- 修复 Windows CLI 启动后台服务后输出管道不结束，以及子目录搜索结果路径显示不完整的问题。
- 改善文件索引的资源占用、转换组件安装与进度显示。
- 优化主题选择、侧栏设置入口与窗口控制布局。

## 0.1.8

- 修复 Windows 文档转换组件安装卡住及部分 PDF 转换失败的问题。
- 文件转换完成后即可检索，索引进度与图表同步刷新。
- 优化 Windows 路径显示、失败文件定位及 @ 文件引用交互。
- 其他产品优化与问题修复。
