# 更新日志 / Changelog

## 0.3.6

- **文件分享**：支持分享 Markdown 文件或文件夹，自动打包正文引用的本地图片并保留目录结构。确认后上传并生成分享码，可复制分享码和接收指引。
- **接收分享**：添加文件夹窗口新增“接收分享”标签页，文件页空白处右键也可接收。支持从剪贴板识别分享码，确认保存位置后下载并加入知识库；默认接收到文档目录下的 `sag-wiki/share`，可更改位置或关闭剪贴板识别，同名文件夹自动编号。
- **CLI 分享与下载**：新增 `sag share` 和 `sag download` 命令，无需启动桌面端或配置模型；支持指定下载位置，并同步补充 Agent Skill 使用说明。
- **索引详情显示**：文件设置新增“显示索引详情”开关，默认关闭；开启并保存后显示文件页的索引状态与详情入口。关闭后仍可通过索引历史查看进度和失败原因。
- **索引状态操作**：侧栏状态卡片标题旁提供暂停、继续和关闭按钮。处理期间关闭卡片不影响后台索引，同一轮任务刷新时保持隐藏。

### English

- **File sharing**: Share a Markdown file or folder with referenced local images and its directory structure. Confirm the upload to generate a share code, then copy the code and receiving instructions.
- **Receiving shared files**: Add a Receive share tab to the add-folder dialog and a receiving action to the file-area context menu. Recognize share codes from the clipboard, then download and add the files to the knowledge base after the destination is confirmed. Save by default under `Documents/sag-wiki/share`, with a configurable destination, optional clipboard detection and numbered folders for name conflicts.
- **CLI sharing and downloads**: Add `sag share` and `sag download`, with no desktop app or model configuration required. Support a custom download destination and include usage instructions in the Agent Skill.
- **Index details display**: Add a Show index details switch in File settings, off by default. Enable and save it to show indexing status and the details entry point on the Files page. Index history remains available for progress and failure details when the switch is off.
- **Index status controls**: Add pause, resume and dismiss buttons beside the sidebar status title. Dismissing the card keeps background indexing running and the card hidden during the same task.

## 0.3.5

- **大文档阅读**：侧边栏、搜索和引用中的大 Markdown 默认先进入阅读界面，按阅读位置显示正文并加载本地图片，减少打开时的长时间卡顿；支持全文目录、查找和复制完整源码，点击“编辑文档”后再进入完整编辑器。

### English

- **Large document reading**: Open large Markdown files from the sidebar, search and citations in a reading view that displays nearby content and loads local images as needed, reducing long opening stalls. Keep full-document outline, find and source copying, and enter the full editor only after selecting Edit document.

## 0.3.4

- **Windows 远程服务**：修复打开远程服务设置后，后台状态检查反复弹出命令行窗口的问题。

### English

- **Windows remote services**: Prevent background status checks from repeatedly opening a command window while Remote services settings is open.

## 0.3.3

- **文档处理进度**：精简侧栏状态，默认显示当前任务的文件进度和文档名称；详情展示搜索可用状态、解析方式、PDF 分段及页码，处理结束后不再保留当前文档名称，并可关闭状态卡片。
- **失败文件重试**：详情列出失败文件及原因，仅重试失败文件并保留成功索引；修复错过任务开始或完成通知时，界面可能一直显示重试中的问题。
- **MinerU 表格兼容**：保留图片表格、标题和脚注，跳过空占位项，支持含空格的图片路径；缺少有效表格内容或图片时仍尝试下一解析方式。
- **大 PDF 解析**：同一文档的分段回退到 Docling 时复用已加载模型，减少重复加载；保留取消处理、资源释放和成功分段缓存。

### English

- **Document processing progress**: Simplify the sidebar to show the current task's file progress and document name. Expand details for search availability, the parser, PDF parts and page ranges. Clear the current document name when processing ends and allow the finished status card to be dismissed.
- **Failed-file retry**: List failed files and their causes in details, retry only those files and preserve successful indexes. Fix the interface remaining stuck in retry mode when task-start or completion notifications are missed.
- **MinerU table compatibility**: Preserve image-only tables, captions and footnotes, skip empty placeholders and support image paths containing spaces. Continue to the next applicable parser when meaningful table content or image assets are missing.
- **Large PDF parsing**: Reuse loaded Docling models across fallback parts of the same document to reduce repeated loading, while retaining cancellation, resource release and successful-part caching.

## 0.3.2

- 修复转换文档中的上下标丢失，以及以上下标标签开头的公式行被错误显示为代码块的问题。（[PR #142](https://github.com/Zleap-AI/SAG-Wiki/pull/142)）
- 孤立的上下标片段保留为文本，同时正确渲染带底数的前置上下标和转义下划线公式。
- 修复 MinerU 分段解析结果重复包裹公式定界符导致的显示错误，并兼容不同的公式类型和正文格式。

### English

- Preserve superscripts and subscripts in converted documents, and prevent formula lines starting with script tags from appearing as code blocks. ([PR #142](https://github.com/Zleap-AI/SAG-Wiki/pull/142))
- Keep isolated script fragments literal while correctly rendering leading scripts followed by a base and formulas containing escaped underscores.
- Fix formula rendering errors caused by duplicated delimiters in segmented MinerU results, and support alternate equation types and content fields.

## 0.3.1

- **Docling 安装与转换**：统一处理 Windows 文档组件的资源、模型目录及转换任务输入输出路径，修复 `additional.dat` 和 `tm_config.json` 的读取错误。保留已下载并校验通过的安装包，安装失败后重试复用缓存；缓存损坏或安装包变化时重新下载。
- **快速入门**：首次启动时独立显示教程，避免后台加载、启动错误或快捷搜索导致教程被跳过。
- **Windows 窗口**：重复打开程序时恢复已有主窗口，避免创建重复窗口。
- **退出应用**：Windows 从托盘或应用菜单退出时，先停止独立后台服务并阻止自动重启，再退出客户端，修复退出时偶发的“访问被拒绝”错误。macOS 保留原有行为，退出客户端后后台继续运行，CLI 和 MCP 可继续使用；关闭窗口时隐藏应用。

### 历史文档补充

详细操作见[文件夹与索引](/zh/docs/folders)和[偏好与快捷键](/zh/docs/preferences)，安装包见[下载安装](/zh/docs/installation)。

### English

- **Docling installation and conversion**: Handle Windows document component resource paths, model directories and conversion input/output paths consistently. Fix errors reading `additional.dat` and `tm_config.json`. Cache completed, verified downloads and reuse them after installation failures. Download again when the cache is damaged or the package changes.
- **Quick start**: Show the first-launch tutorial independently of background loading, startup errors and quick search.
- **Windows app**: Restore the existing main window when reopening the app, avoiding duplicate windows.
- **App exit**: On Windows, quitting from the tray or app menu stops the standalone service before exiting the client and prevents automatic restarts during shutdown. Fix an intermittent “Access is denied” error during shutdown. macOS retains its existing behavior: the service keeps running after the client exits, so CLI and MCP remain available. Closing the window hides the app.

### Historical documentation details

See [Folders and indexing](/en/docs/folders) and [Preferences and shortcuts](/en/docs/preferences) for details, and [Installation](/en/docs/installation) for installers.

## 0.3.0

- **聊天记录导入**：支持将 Claude Code 和 Codex 本地聊天记录转换为 Markdown，默认保存到 `文档/sag-wiki/agent-history`，按 `claude code` 和 `codex` 分子文件夹，也可更改保存位置。
- **聊天预览与清洗**：支持搜索、项目筛选、分页和跨页勾选，优化 Codex 列表读取速度。过滤系统提示词、环境信息、内部通知和自动注入的 Skill 正文；开启“展示工具调用过程”后，预览和导入同时保留工具调用与结果。重复导入跳过未变化内容，更新时保留手动修改的文件。
- **笔记导入**：支持 Notion HTML 导出包、Evernote ENEX、Google Keep Takeout 和 macOS Apple Notes，转换为 Markdown 并保留可转换的格式、内部链接与附件，完成后加入知识库。
- **数据导入入口**：设置中的“数据导入”和添加文件夹窗口统一提供聊天与笔记导入入口；文件设置改用“索引文件类型”，区分文件索引和数据导入。

### 历史文档补充

详细操作见[导入数据](/zh/docs/data-import)，安装包见 [0.3.0 正式版](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.3.0)。

### English

- **Chat history imports**: Convert local Claude Code and Codex chats to Markdown. Save by default under `Documents/sag-wiki/agent-history`, in separate `claude code` and `codex` subfolders, or choose another location.
- **Chat previews and cleanup**: Search, filter by project, browse pages and keep selections across pages, with faster Codex listing. Exclude system prompts, environment context, internal notifications and injected Skill content. Enable “Show tool calls and results” to include them in both previews and imports. Repeated imports skip unchanged content and preserve manually edited files.
- **Note imports**: Import Notion HTML exports, Evernote ENEX, Google Keep Takeout and macOS Apple Notes as Markdown, retaining supported formatting, internal links and attachments, then add them to the knowledge base.
- **Data import entry points**: Access chat and note imports from Data import in Settings or the add-folder dialog. “File types to index” distinguishes indexing preferences from data imports.

### Historical documentation details

See [Import data](/en/docs/data-import) for instructions and the [0.3.0 release](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.3.0) for installers.

## 0.2.18

- **转换结果存放位置**：可选择应用数据目录、资料文件夹或自定义目录，切换时迁移已有结果，无需重新识别。阅读转换结果时可直接打开原文件。
- **文件与解析设置**：Docling 和 MinerU 的配置入口统一到文件设置，可选择优先解析方式，并调整设置分组。
- **macOS 更新重启**：应用内更新后等待旧进程退出，再从原安装位置重新启动。

### 历史文档补充

- **转换结果存放位置**：可选择应用数据目录、资料文件夹或自定义目录，迁移已有结果无需重新识别，并可直接打开原文件。
- **文件与解析设置**：统一 Docling 和 MinerU 配置入口，可选择优先解析方式，并调整设置分组。
- **macOS 更新重启**：等待旧进程退出后，从原安装位置重新启动。

### English

- **Conversion storage**: Save converted files in the app data directory, library folders, or a custom directory. Move existing results without repeating OCR, and open the original file from the reader.
- **File and parser settings**: Configure Docling and MinerU in File settings, choose the preferred parser, and use the revised settings groups.
- **macOS update restart**: After an in-app update, wait for the old process to exit before reopening the app from its original location.

### Historical documentation details

- **Conversion storage**: Choose the app data directory, library folders, or a custom directory. Move existing results without repeating OCR and open the original file from the reader.
- **File and parser settings**: Configure Docling and MinerU in one place, choose the preferred parser, and use the revised settings groups.
- **macOS update restart**: Wait for the old process to exit before reopening the app from its original location.

## 0.2.17

- **窄屏布局**：改善手机网页和窄桌面窗口中的侧栏、设置、搜索和问答。
- **问答模型与搜索总结**：支持从服务商获取模型列表、选择多个问答模型，并接入 OpenAI Decisions。搜索总结可单独选择模型并查看用量。
- **文档解析**：Docling 组件可从资源站安装，MinerU 可配置自定义接口。文件转换默认并行数提高到 4，已保存的设置继续保留。
- **系统代理**：Windows 和 macOS 上的模型请求可使用手动配置的系统代理，修改后无需重启。

### 历史文档补充

- **窄屏布局**：改善手机网页和窄桌面窗口中的侧栏、设置、搜索和问答。
- **问答模型与搜索总结**：支持获取模型列表和接入 OpenAI Decisions，搜索总结可单独选择模型并查看用量。
- **文档解析**：Docling 可从资源站安装，MinerU 可配置自定义接口。文件转换默认并行数提高到 4，保留已保存的设置。
- **系统代理**：Windows 和 macOS 上的模型请求可使用手动配置的系统代理。

### English

- **Narrow layouts**: Improve the sidebar, settings, search, and chat on mobile browsers and in narrow desktop windows.
- **Chat models and search summaries**: Fetch model lists from providers, select multiple chat models, and use OpenAI Decisions. Choose a separate model for search summaries and view its usage.
- **Document parsing**: Install Docling from the resource site and configure a custom MinerU endpoint. File conversion defaults to four concurrent tasks and preserves saved settings.
- **System proxy**: Model requests on Windows and macOS support manually configured system proxies. Changes take effect without restarting.

### Historical documentation details

- **Narrow layouts**: Improve the sidebar, settings, search, and chat on mobile browsers and in narrow desktop windows.
- **Chat models and search summaries**: Fetch model lists and use OpenAI Decisions. Choose a separate model for search summaries and view its usage.
- **Document parsing**: Install Docling from the resource site and configure a custom MinerU endpoint. File conversion defaults to four concurrent tasks and preserves saved settings.
- **System proxy**: Model requests on Windows and macOS support manually configured system proxies.

## 0.2.16

- **数学公式**：问答和文档阅读支持显示 TeX 公式，离线也可用。
- **搜索排除**：CLI、MCP 和 HTTP API 支持按相对路径排除文件和目录。
- **PDF 索引**：修复部分 PDF 无法读取页数时的索引失败，可重新索引受影响文件。

### 历史文档补充

- **数学公式**：问答和文档阅读支持离线显示 TeX 公式。
- **搜索排除**：CLI、MCP 和 HTTP API 可按相对路径排除文件和目录。
- **PDF 索引**：修复部分 PDF 无法读取页数时的索引失败。

### English

- **Math formulas**: Render TeX formulas in answers and document reading views, including offline.
- **Search exclusions**: Exclude files and folders by relative path in CLI, MCP, and HTTP API searches.
- **PDF indexing**: Fix indexing for PDFs whose page count could not be read. Reindex affected files to retry.

### Historical documentation details

- **Math formulas**: Render TeX formulas offline in answers and document reading views.
- **Search exclusions**: Exclude files and folders by relative path in CLI, MCP, and HTTP API searches.
- **PDF indexing**: Fix indexing for PDFs whose page count could not be read.

## 0.2.13

- **升级缓存与索引统计**：启动或升级后的检查复用未变化的索引和兼容的转换结果；索引详情区分已检查、已复用、已更新和失败，避免将检查文件数误认为重新构建数。内容、解析设置或必要迁移变化时，仍会更新受影响文档。
- **Office 仅提取文字**：新增 DOCX/PPTX 纯文字解析选项，跳过图片和内嵌对象且不执行图片 OCR；设置变化后重新检查相关文档。
- **远程服务**：提供本机和局域网网页入口，支持文件浏览、Markdown 编辑、搜索和问答。
- **CLI 随桌面端升级**：修复受管的旧版 `sag` 未同步更新的问题，替换前会备份校验不一致的旧文件。

- **MCP 接入**：通过 API 服务连接外部 MCP 客户端，支持搜索知识库和浏览文件夹；可从设置页复制连接配置，也可用 CLI 导出。
- **API Key 管理**：新建的 Key 可在桌面设置中再次复制，旧 Key 仍可使用。

### 历史文档补充

- **升级缓存与索引统计**：启动或升级后的检查复用未变化的索引和兼容的转换结果；索引详情区分已检查、已复用、已更新和失败，避免将检查文件数误认为重新构建数。内容、解析设置或必要迁移变化时，仍会更新受影响文档。
- **Office 仅提取文字**：新增 DOCX/PPTX 纯文字解析选项，跳过图片和内嵌对象，不识别图片文字。修改设置后重新检查相关文档，详见[文件夹与索引](/zh/docs/folders)。
- **远程服务**：可在本机或局域网浏览器中浏览文件、编辑 Markdown、搜索和问答。Linux 服务器只装独立 CLI 即可运行网页服务，无需安装桌面端。网页服务与 API 可分别开关，详见[网页服务](/zh/docs/remote-access)。
- **CLI 随桌面端升级**：修复受管的旧版 `sag` 未同步更新的问题，替换前会备份校验不一致的旧文件。

- **MCP 接入**：外部 MCP 客户端可搜索知识库、浏览文件夹；可在桌面设置中复制连接配置，也可用 CLI 导出。
- **API Key 管理**：可在桌面设置中再次复制新建的 Key。

### English

- **Upgrade cache reuse and indexing summaries**: Startup and upgrade checks reuse unchanged indexes and compatible conversion results. Indexing details distinguish checked, reused, updated and failed files, so a check is not mistaken for a rebuild. Content changes, parsing settings and required migrations can still update affected documents.
- **Office text only**: Add an opt-in DOCX/PPTX parsing mode that skips images and embedded objects without image OCR. Changed settings recheck affected documents.
- **Remote services**: Open the workspace from a browser on the host or LAN to browse files, edit Markdown, search and ask questions.
- **CLI updates with the desktop app**: Fix an older managed `sag` being left behind. The app backs up an old executable if its checksum no longer matches.

- **MCP access**: Connect external MCP clients through the API service to search the knowledge base and browse folders. Copy the configuration from desktop settings or export it with the CLI.
- **API key management**: Copy newly created keys again from desktop settings. Existing keys continue to work.

### Historical documentation details

- **Upgrade cache reuse and indexing summaries**: Startup and upgrade checks reuse unchanged indexes and compatible conversion results. Indexing details distinguish checked, reused, updated and failed files, so a check is not mistaken for a rebuild. Content changes, parsing settings and required migrations can still update affected documents.
- **Office text only**: Add an opt-in DOCX/PPTX parsing mode that skips images and embedded objects without image OCR. Changed settings recheck affected documents. See [Folders and indexing](/en/docs/folders).
- **Remote services**: Browse files, edit Markdown, search and ask questions in a browser on the host or LAN. A Linux server can run the web service with just the standalone CLI, without installing the desktop app. The web app and API can be enabled independently. See [Web service](/en/docs/remote-access).
- **CLI updates with the desktop app**: Fix an older managed `sag` being left behind. The app backs up an old executable if its checksum no longer matches.

- **MCP access**: External MCP clients can search the knowledge base and browse folders. Copy the configuration from desktop settings or export it with the CLI.
- **API key management**: Copy newly created keys again from desktop settings.

## 0.2.12

- **基础文字先可搜索**：PDF 先发布可提取的文字，混合或扫描 PDF 在后台继续 OCR 和表格增强；文件列表明确显示可搜索范围及增强状态。纯文字 PDF 保持快速完成。
- **大 PDF 不再挡住新文件**：基础索引和增强使用独立工作线程，本地内置解析支持的 Office、RTF 等文件可先完成文字索引；云解析沿用原配置顺序。增强失败保留已有可用文字，并支持只重试失败文件。
- **减少 PDF 重复加载模型**：同一 PDF 的多个分段复用 Docling 进程和模型，文档完成、失败或取消后释放；保持现有 CPU 线程策略、OCR 和表格识别配置。修复并行扫描误删或覆盖新增强结果的问题。
- **更可靠的索引进度与恢复**：分别显示基础索引、后台增强和 PDF 分段进度，减少状态闪动；未变化的失败文件复用失败记录，文件、组件或相关预算变化后重新尝试。
- **改善 CLI 与问答兼容性**：`sag add` 支持用 `--timeout` 调整等待时间，超时明确提示后台仍在索引；决策模型批次因问题条数超限被拒绝时自动拆批重试。修复数学区间被误判为引用、金额内容被数学表达式过滤的问题；切换决策模型提供商时重置模型选择。
- **改善组件安装与分发**：调整文档组件安装内存限制，保留失败诊断，正确识别 Windows 解析进程在启动阶段超过内存上限的情况；继续精简 QPDF 分发内容。

桌面端与随包 CLI 版本统一为 0.2.12。基础文字可搜索不代表扫描内容和复杂表格已全部完成，请以界面状态及原文核对完整性。

### 历史文档补充

- **分阶段索引**：PDF 可提取的文字先进入搜索，OCR 和表格识别在后台继续，大 PDF 处理期间仍可索引其他文件。
- **PDF 分段处理**：同一份 PDF 的多个分段复用 Docling 模型，并显示处理进度。
- **失败重试**：可单独重试索引失败的文件，保留已可搜索的文字。

### English

- **Search basic text sooner**: publish usable PDF text before background OCR and table enhancement for mixed or scanned PDFs. File-list states identify search availability and enhancement progress. Text-only PDFs retain fast completion.
- **Keep new files moving**: independent basic-indexing and enhancement workers let supported local Office, RTF and other built-in formats become searchable while a large PDF is enhanced. Cloud parsing retains its configured order. Enhancement failures retain usable text, with selective failed-file retry.
- **Reuse PDF models**: share one Docling process and its models across a PDF's segments, releasing resources on completion, failure or cancellation. Keep CPU thread policy and OCR/table settings unchanged. Fix concurrent scans deleting or overwriting newly enhanced indexes.
- **Clearer progress and recovery**: show separate basic, enhancement and PDF-segment progress with more stable updates. Reuse unchanged failure records and retry when files, components or relevant budgets change.
- **CLI and answer compatibility**: `sag add --timeout` controls the wait and explains when indexing continues after timeout. Split decision-model batches rejected for exceeding question-count limits. Avoid treating mathematical intervals as citations or stripping prices as mathematical expressions, and reset the model selection when switching decision-model providers.
- **Component installation and distribution**: adjust document-component installation memory limits while retaining diagnostics, correctly report Windows parser memory-limit failures during startup, and further reduce the QPDF distribution payload.

The desktop app and bundled CLI both use 0.2.12. Searchable basic text does not imply complete scanned-content or complex-table coverage; check the displayed state and original document.

### Historical documentation details

- **Progressive indexing**: Search available PDF text while OCR and table recognition continue in the background. Other files can be indexed while a large PDF is processed.
- **PDF segments**: Reuse Docling models across segments of the same PDF and show conversion progress.
- **Retry failed files**: Retry indexing only affected files while keeping available text searchable.

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

### 历史文档补充

- **文档折叠**：Markdown 支持多级阅读，可直接切换卡片与导图，保留原文供编辑。
- **后台处理与缓存**：关闭面板后继续处理，支持查看进度、主动停止和复用已完成结果。
- **长文档索引**：改进超长表格行、Windows 换行和数字内容的处理，保留完整原文及对应位置。

### English

- **Document folding**: Markdown offers Full, Brief, Key, Takeaway, Cards and Map tabs while keeping the source editable.
- **Background processing and caching**: processing continues when the fold panel closes, with progress, explicit cancellation and reuse of completed results.
- **Long-document indexing**: improve handling of long table rows, Windows line endings and numeric content while preserving complete source text and locations.

The desktop app and bundled CLI both use 0.2.7.

### Historical documentation details

- **Document folding**: Markdown supports multiple reading levels and direct switching between cards and a map while keeping the source editable.
- **Background processing and caching**: processing continues when the panel closes, with progress, explicit cancellation and reuse of completed results.
- **Long-document indexing**: improve handling of long table rows, Windows line endings and numeric content while preserving complete source text and locations.

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

### 历史文档补充

- **快捷搜索**：新增全局快捷键（macOS ⌘⇧Space，Windows Ctrl+Shift+Space）唤起的搜索弹窗，可选择结果并在主窗口打开原文，关闭后返回原应用；设置中可启用、关闭或改键。
- **跨语言检索**：问答在同一次检索中同时提供中英文等义表述，中文提问默认用中文作答，精确文字与代码不翻译。
- **工具调用**：每轮工具调用次数默认不限制，仍可设为 1–100。
- **索引**：文本型 PDF、Office、RTF、CSV、EPUB 等内置文档默认纳入索引；转换内存按物理内存计算，文本准备预算提高到 512 MiB。
- **接入与显示**：CLI 设置提供本机 Skill 接入指令；搜索预览、快捷搜索和引用原文渲染 HTML 表格、对齐方式与本地图片，本地图片支持 SVG。

桌面端与随包 CLI 版本统一为 0.2.6。

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

### Historical documentation details

- **Quick Search**: a global shortcut (⌘⇧Space on macOS, Ctrl+Shift+Space on Windows) opens a search popup that lists local matches and opens the passage in the main window, then returns to the previous app. Settings can enable, disable or rebind it.
- **Cross-language retrieval**: Q&A includes equivalent Chinese and English wording in the same search and answers Chinese questions in Chinese; exact text and code stay untranslated.
- **Tool calls**: calls per turn are unlimited by default and can still be capped at 1–100.
- **Indexing**: text-based PDF, Office, RTF, CSV and EPUB files are indexed by default. Conversion memory follows physical memory, and text preparation allows 512 MiB.
- **Setup and display**: CLI settings provide a local Skill setup instruction; previews, Quick Search and citations render HTML tables, alignment and local images, including SVG.

The desktop app and bundled CLI both use 0.2.6.

## 0.2.5

- **搜索总结**：有结果且回答模型可用时默认展示自动总结，支持从原文返回已有总结。
- **原文定位**：Markdown 按所选片段的来源位置高亮段落，语义结果不含搜索原词时也能定位；切换文件后滚动到所选片段。
- **HTML 高亮修复**：使用段落轮廓标记，保留原文的文字和背景色，避免白底白字等对比度问题。
- **定时更新检查**：运行期间每隔 8 小时检查新版，发现更新后可由用户选择下载与安装。
- **开发窗口刷新修复**：文档、缓存及扩展构建目录的变化不再触发桌面开发窗口刷新。
- **macOS 签名与公证**：发布流程为应用、包内 CLI 和 DMG 接入 Developer ID 签名、Apple 公证及票据装订，并增加最终安装包校验。

桌面端与随包 CLI 版本统一为 0.2.5，沿用现行更新签名通道；本次更新不涉及数据库迁移。

### 历史文档补充

- **搜索总结**：有结果且回答模型可用时默认展示自动总结，支持从原文返回已有总结。
- **原文定位**：Markdown 按所选片段的来源位置高亮段落，切换文件后滚动到所选片段。
- **HTML 高亮修复**：使用段落轮廓标记，保留原文文字和背景色。
- **定时更新检查**：运行期间每隔 8 小时检查新版，下载与安装由用户选择。
- **macOS 签名与公证**：发布流程为应用、包内 CLI 和 DMG 接入签名、公证与票据装订。

桌面端与随包 CLI 版本统一为 0.2.5。

### English

- **Search summaries**: open the automatic summary by default when results and an answer model are available, and return to an existing summary from its source.
- **Source navigation**: highlight Markdown paragraphs using the selected passage's source position, including semantic results without the original search words. Scroll to the selected passage when switching files.
- **HTML highlight fix**: outline the selected paragraph while preserving source text and background colors, preventing white-on-white text and other contrast problems.
- **Periodic update checks**: check for updates every eight hours while the app is running, with downloads and installation controlled by the user.
- **Development reload fix**: changes to documentation, caches and extension build output no longer reload the desktop development window.
- **macOS signing and notarization**: add Developer ID signing, Apple notarization, stapled tickets and final-package verification to the release workflow for the app, bundled CLI and DMG.

The desktop app and bundled CLI both use 0.2.5, retaining the current update-signing channel. This update requires no database migration.

### Historical documentation details

- **Search summaries**: open the automatic summary by default when results and an answer model are available.
- **Source navigation**: highlight Markdown paragraphs from the selected passage's source position and scroll to it when switching files.
- **HTML highlight fix**: outline the selected paragraph while keeping its text and background colors.
- **Periodic update checks**: check for updates every eight hours, with downloads and installation controlled by the user.
- **macOS signing and notarization**: sign, notarize and staple the app, bundled CLI and DMG in the release workflow.

The desktop app and bundled CLI both use 0.2.5.

## 0.2.4

- **CLI 连接反馈**：测试成功后默认收起详情，支持展开查看；在设置分类间切换时保留最近结果和展开状态。
- **连接状态更新**：再次测试清除旧结果并防止重复请求；关闭 CLI 时清除缓存状态。成功徽标停留后淡出，适配明暗主题与减少动态效果偏好。
- **模型设置**：LLM 与 Jev 的“测试连接”按钮统一放在卡片右下角。
- **Windows 窗口控制**：将最小化、最大化／还原和关闭按钮恢复到左侧栏顶部，移除独立顶部标题栏；侧栏收起时仍可操作，macOS 继续使用原生窗口控件。

桌面端与随包 CLI 版本统一为 0.2.4，沿用现行更新签名通道；本次更新不涉及数据库迁移。

### 历史文档补充

- **CLI 连接反馈**：测试成功后默认收起详情，支持展开查看。
- **连接状态更新**：再次测试清除旧结果并防止重复请求，关闭 CLI 时清除缓存状态。
- **模型设置**：LLM 与 Jev 的“测试连接”按钮统一放在卡片右下角。
- **Windows 窗口控制**：最小化、最大化／还原和关闭按钮恢复到左侧栏顶部。

桌面端与随包 CLI 版本统一为 0.2.4。

### English

- **CLI connection feedback**: collapse successful connection details by default, with an option to expand them. Preserve the latest result and disclosure state when switching settings categories.
- **Connection state updates**: clear stale results before retesting, prevent duplicate requests and clear cached feedback when CLI access is disabled. The success badge fades away, with light/dark theme and reduced-motion support.
- **Model settings**: align the LLM and Jev connection-test buttons at the bottom right of their cards.
- **Windows window controls**: restore minimize, maximize/restore and close to the top of the sidebar, removing the separate title bar. Keep controls available with the sidebar collapsed; macOS retains its native window controls.

The desktop app and bundled CLI both use 0.2.4, retaining the current update-signing channel. This update requires no database migration.

### Historical documentation details

- **CLI connection feedback**: collapse successful connection details by default, with an option to expand them.
- **Connection state updates**: clear stale results before retesting and prevent duplicate requests; clear cached feedback when CLI access is disabled.
- **Model settings**: align the LLM and Jev connection-test buttons at the bottom right of their cards.
- **Windows window controls**: restore minimize, maximize/restore and close to the top of the sidebar.

The desktop app and bundled CLI both use 0.2.4.

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

### English

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

### 历史文档补充

- 会话与文件夹共用侧栏，支持按日期浏览及搜索会话。
- 索引历史按文件夹汇总，提供更清晰的失败原因与文件详情。
- 问答工具调用归入研究步骤，优化生成中的输入与引用原文侧栏。
- 修复取消后的事件处理、历史刷新覆盖新回答及空知识库引用提示。

桌面端与随包 CLI 统一为 0.2.1，沿用现行更新签名通道。

### English

- Share the sidebar between conversations and folders, with date grouping and conversation search.
- Group indexing history by folder and show clearer failure reasons and file details.
- Group tool calls into research steps and improve input during generation and the citation source panel.
- Fix event handling after cancellation, history refreshes overwriting new answers and empty-library mention hints.

The desktop app and bundled CLI both use 0.2.1 and retain the current update-signing channel.

## 0.2.0

- 新增网页剪藏、Mermaid 图表和文档目录。
- 内置解析 Office、文本型 PDF 和电子书。
- 改善文件管理、问答过程展示和索引体验。
- 新增中英文帮助文档。

### 历史文档补充

2026-10-23 · [发布说明](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.2.0)

- 新增网页剪藏，支持预览后保存为本地 Markdown。
- 新增 Mermaid 图表和文档目录。
- Office、文本型 PDF 和电子书支持内置解析。
- 改善文件浏览与新建文档，支持按文件夹设置索引。
- 问答处理过程自动折叠，并显示用时。
- 改善索引进度、失败提示和文档转换。
- 改善首次启动、连接恢复和 MCP 服务设置。
- 新增中英文帮助文档。

### English

2026-10-23 · [Release notes](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.2.0)

- Adds web clipping with a preview before saving local Markdown.
- Adds Mermaid diagrams and a document outline.
- Adds built-in parsing for Office files, text-based PDFs and ebooks.
- Improves file browsing, document creation and per-folder indexing settings.
- Collapses answer processing details and displays elapsed time.
- Improves indexing progress, failure notices and document conversion.
- Improves first launch, connection recovery and MCP server settings.
- Adds Chinese and English documentation.

## 0.1.9

- 支持本地 Jev 和无需 API Key 的连接方式。
- 改善文档转换、索引和密钥保存。
- 修复 Windows CLI 的后台启动和搜索路径问题。
- 更换更新签名密钥，旧版本需手动升级一次至 0.1.9。

### 历史文档补充

2026-10-22 · [发布说明](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.9)

- 支持本地 Jev，以及无需 API Key 的连接方式。
- 改善索引、文档转换和密钥保存。
- 修复 Windows CLI 的后台启动和搜索路径问题。
- 更换更新签名密钥。

### English

2026-10-22 · [Release notes](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.9)

- Supports local Jev services and connections without an API key.
- Improves indexing, document conversion and credential storage.
- Fixes Windows CLI background startup and search-result paths.
- Changes the update-signing key.

## 0.1.8

- 修复 Windows 文档转换问题。
- 文档转换完成后即可搜索。
- 改善文件路径、失败定位和文件引用操作。

### 历史文档补充

2026-10-20 · [发布说明](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.8)

### English

2026-10-20 · [Release notes](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.8)

- Fixes Windows document conversion.
- Makes documents searchable as soon as conversion finishes.
- Improves file paths, failure details and file mentions.

## 0.1.7

2026-10-20 · [发布说明](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.7)

- 支持浏览未索引文件和空文件夹。
- 新增“全部”“已索引”“未索引”筛选。
- 改善 Windows 安装与卸载。

### English

2026-10-20 · [Release notes](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.7)

- Supports browsing unindexed files and empty folders.
- Adds All, Indexed and Not indexed filters.
- Improves Windows installation and removal.

## 0.1.6

2026-10-20 · [发布说明](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.6)

- 新增应用内更新和多语言 Windows 安装器。
- 支持自定义 Jev 地址和 CLI 文件类型设置。
- 增加文件夹移除确认，完善 Agent 接入说明。

### English

2026-10-20 · [Release notes](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.6)

- Adds in-app updates and a multilingual Windows installer.
- Supports custom Jev endpoints and CLI file-type settings.
- Adds folder-removal confirmation and improves Agent setup guidance.

## 0.1.5

2026-10-19 · [发布说明](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.5)

- 提供 macOS 和 Windows 安装包，完成 macOS 签名与公证。
- 补充 CLI 安装和 Agent 接入文档。

### English

2026-10-19 · [Release notes](https://github.com/Zleap-AI/SAG-Wiki/releases/tag/v0.1.5)

- Provides macOS and Windows installers, with macOS signing and notarization.
- Adds CLI installation and Agent setup documentation.

## 0.1.0

2026-10-14 – 2026-10-18

- 支持本地文件管理、Markdown 编辑和文件变化监听。
- 提供快速搜索、SAG2 深入检索和本地文档关联图。
- 支持多模型问答、原文引用和会话历史。
- 提供独立 CLI、MCP 工具和共享后台服务。
- 支持文件夹范围、忽略规则和模型配置。

### English

2026-10-14 – 2026-10-18

- Supports local file management, Markdown editing and file watching.
- Provides quick search, SAG2 retrieval and a local document relationship graph.
- Supports multiple answer models, source citations and conversation history.
- Provides a standalone CLI, MCP tools and a shared background service.
- Supports folder scopes, ignore rules and model configuration.
