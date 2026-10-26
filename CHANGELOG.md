# 更新日志 / Changelog

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
