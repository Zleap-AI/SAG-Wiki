# 更新日志 / Changelog

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

- 新增网页剪藏、Mermaid 图表和文档目录。
- 内置解析 Office、文本型 PDF 和电子书。
- 改善文件管理、问答过程展示和索引体验。
- 新增中英文帮助文档。

## 0.1.9

- 支持本地 Jev 和无需 API Key 的连接方式。
- 改善文档转换、索引和密钥保存。
- 修复 Windows CLI 的后台启动和搜索路径问题。
- 更换更新签名密钥，旧版本需手动升级一次至 0.1.9。

## 0.1.8

- 修复 Windows 文档转换问题。
- 文档转换完成后即可搜索。
- 改善文件路径、失败定位和文件引用操作。
