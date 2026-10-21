# 飞牛版验证记录

版本：0.1.9。验证日期：2026-10-21。

## 版本号统一

非飞牛版的 package.json、Tauri 配置及 Rust 应用版本均为 0.1.9。飞牛版的前端、后端、锁文件、FPK manifest 与安装说明同步为 0.1.9，沿用现有功能和修复。此前 0.2.x 仅保留为迁移测试历史编号。

x86 与 ARM 安装包重新构建，并核对 FPK manifest、两个 Linux ELF 程序的架构、前端版本和 SHA-256。中文与英文安装说明及站内搜索均已显示 0.1.9。按用户要求删除旧安装包及对应校验文件，只保留当前两个架构的安装包和校验文件。

用户确认后，已尝试在设备安装 0.1.9。飞牛返回“已安装相同或更高版本的 SAG Wiki”，拒绝覆盖 0.2.6；卸载窗口没有显示保留数据选项，未执行卸载。设备仍运行 0.2.6，现有配置、索引和文件保持不变。

## 扫描 PDF OCR 实机验证

在已安装的 0.2.6 上验证，无需新增设置或升级。测试文件 `output/pdf/SAG-扫描OCR验证-20261021.pdf` 为一页图片 PDF，只有一张图像，文字层提取结果为空；220 DPI、轻微旋转，内容为虚构的中文档案复核表。大小 284954 字节，SHA-256 为 `7a638c0160f05d0bad9376708117165f98e73737ff06dd8300b63bab3b35b960`。

通过飞牛文件管理上传到已启用 PDF 转换的 File 目录后，自动执行 OCR、生成 Markdown 并索引；17:45:34 完成，索引文件数由 36 增至 37。下载实际生成的 1138 字节 Markdown，档案编号、地点、负责人、复核日期、卷宗数、缺页册数及补齐日期共 7 项字段全部一致，唯一标记 `QINGHE-SCAN-4729` 保留。重新下载原 PDF，校验值不变且仍无文字层。

快速搜索中文地点可找到该文件；搜索唯一标记返回 1 个文件，8 ms 本地匹配。问答输入“青禾档案复核记录中，缺少目录页的卷宗有几册？谁负责补齐，截止日期是什么？请引用资料。”，实际调用 `SAG · search`，3.9 秒完成检索；回答正确给出 3 册、林青、2026 年 10 月 29 日。点击引用可打开 OCR 原文并核对表格。未把参考答案写入问题或会话。

证据：`.local/qa/fnos-ocr-verification.json`、`.local/qa/fnos-scanned-ocr.md`、`.local/qa/fnos-ocr-search-success.png`、`.local/qa/fnos-ocr-qa-success.png`、`.local/qa/fnos-ocr-citation.png`。本次覆盖清晰的中文印刷扫描页、数字和表格，未覆盖手写、严重模糊、多栏或多页扫描件。

中英文使用说明、开发验证说明与 OCR 问答截图已同步。文档构建通过；浏览器验证四个页面的新增内容、截图加载（1280 像素宽）和中英文站内搜索收录结果。应用代码未改动，版本仍为 0.2.6。

## 0.2.6 PDF 实机测试与启动修复

已通过飞牛文件管理将 `SAG-PDF-转换验证-20261021.pdf` 上传至 `/vol1/1000/File/`。文件为 1 页文本 PDF，617 字节，标记 `pdfnativeconversionmarker`；本地渲染和文本提取通过，SHA-256 为 `10ead3583e5ab55ffa381df449d3759d9050370bc2aeddc0cd66c2b7e59d3a4d`。

实机 Docling 安装在下载 uv 后、启动 Python 安装前失败，界面仅显示“无法启动 Docling 组件”。本地已复现：应用账号的 HOME 指向不存在目录时，PTY 默认进入该目录而启动失败。为组件命令指定自身数据目录作为工作目录，并保留完整错误链及可执行文件路径。新增两项回归，修改前均失败，修改后 22 项转换测试通过。0.2.6 已在实机安装，重试后成功启动 uv，安装 Python 与 140 个依赖；原启动错误未再出现。

后续模型下载报 `ConnectError: [Errno 101] Network is unreachable`，NAS 无法连接 Hugging Face。已在本机下载 Docling 要求的官方 layout、layout ONNX 和 tableformer 仓库完整快照，按官方 Git blob / LFS SHA-256 验证全部文件，再通过飞牛文件管理上传。OCR 文件按 NAS 实际安装的 RapidOCR 清单下载，并核对清单内的 SHA-256。重试后，安装程序读取本地模型并显示“Docling 已准备就绪”。下载清单与脚本位于 `.local/qa/`，没有改写组件就绪标志。

在 File 的索引设置中启用 PDF，保留 Markdown 与文本选项。保存后自动转换测试 PDF，生成 `.sag-converted/SAG-PDF-转换验证-20261021.pdf.md`，索引于 14:10:53 完成，文件数从 35 增至 36。快速搜索 `pdfnativeconversionmarker` 返回 1 个文件，耗时 6 ms；预览显示识别出的正文。深入搜索也将该文件排在首位并生成有引用的总结。通过飞牛文件管理下载生成的 143 字节 Markdown，确认包含原文件链接与完整验证文本；重新下载原 PDF，SHA-256 与上传前完全一致。实机截图：`.local/qa/fnos-pdf-conversion-success.png`。此次首先验证文本 PDF，后续扫描 PDF 验证见上节；Office 格式仅有此前 Linux 转换验证。

对应中文与英文使用、开发页面已同步，文档站构建通过。浏览器核对两种语言的正文与导航链接，站内搜索 `Hugging Face` 均返回更新后的模型安装与实机验证说明。

## 0.2.5 飞牛设置

授权目录改为带图标、文件夹名称和可读路径的列表，并增加统一间距、边框和长路径换行。独立网页原先直接调用桌面专用 `openAppSetting`，实机复现 `Host bridge is not available outside iframe or app runtime`。现改为打开当前 NAS 的飞牛桌面，并显示应用中心中的设置路径；桌面内继续使用官方原生设置接口。数据与隐私中的同一入口同步修复。

平台回归在修改前失败，修改后 7 项前端测试通过，覆盖独立网页不调用宿主接口、桌面原生调用和错误透传。Svelte 检查 0 错误、0 警告。本地浏览器已通过目录列表、14 px 行内边距和无溢出检查，并确认按钮打开模拟飞牛桌面且原页面不报错。真实飞牛桌面已确认原生入口能打开 SAG Wiki 应用设置。中文和英文文档的页面、截图、安装包版本和搜索检查通过。x86 与 ARM 安装包已生成；0.2.6 已在实机安装。授权目录列表与独立页面的“打开飞牛桌面”入口已通过实机验证。实机截图：`.local/qa/fnos-real-settings-0.2.6.png`。

## 问答自动检索修复

已安装的 0.2.4 保持不变，通过“问答设置”的系统提示词即时应用修复，源码默认提示词同步更新。问题来自问答模型对通用知识问题跳过检索；内置 MCP 连接与文件工具正常。

实机复现：`结婚会怎么样` 连续两次直接回答，工具调用数为 0；明确要求查找测试文档时，`list` 和 `read` 正常返回验证标记。只修改系统提示词后，同一问题调用 `search`，返回本地资料及引用。未改动密钥、授权范围和其他工具设置。另一个普通问法也成功调用 `search`（4.1 秒）；问候和明确不检索的翻译请求直接回答，工具调用数均为 0。最终规则在新页面复测原问题，`search` 完成耗时 3.2 秒，回答显示本地引用。实机截图：`.local/qa/fnos-qa-search-fixed.png`。47 项问答代理测试通过，中文和英文文档构建、正文、导航链接及站内搜索检查通过。

## 0.2.4 界面调整

侧栏标题上移 36 px，收起按钮与标题同排，折叠状态移除窗口按钮预留空白。已检查展开和折叠布局，Svelte 检查及生产构建通过。功能代码沿用 0.2.3 的验证结果。0.2.4 已在实机升级并打开。标题相对侧栏顶部为 18.75 px，原为 54.75 px；收起按钮处于标题同行，授权目录与会话仍保留。实机截图：`.local/qa/fnos-real-sidebar-header.png`。

## 本次修复

- 实机复现：添加目录弹窗缺少布局样式，标题和关闭按钮错位、内容无内边距。恢复布局、颜色变量及内容滚动；本地浏览器检查标题对齐、24px 内边距与底部按钮可见。
- 实机复现：选择后读取授权列表、添加目录均显示“请求来源与应用地址不一致”。原实现直接比较网关 Host 与浏览器 Origin；改为浏览器 Fetch Metadata 同源校验，并为旧客户端规范化域名和默认端口。
- 回归：新增 3 项来源校验测试；测试网关改写 Host 后，通过浏览器完成授权、添加目录和关键词检索。
- x86 与 ARM 安装包均验证 manifest、布局样式、两个 Linux 程序及 SHA-256。

- 实机诊断确认 `trim.file.convertPath` 返回路径数组，与文档 `{status,result}` 对象不同。0.2.3 在后端统一两种格式；新回归覆盖两种响应，浏览器按实机数组格式完成授权、目录显示和添加。临时诊断输出已移除。

## 已完成

| 项目 | 结果 |
| --- | --- |
| Svelte 与 TypeScript | 0 错误、0 警告；生产构建通过 |
| Rust 工作区 | 310 项通过；3 项明确标记为忽略的测试未运行 |
| 前端测试 | 授权回调、来源校验、用户存储隔离通过 |
| 浏览器流程 | 授权、添加目录、索引、搜索预览、编辑、只读拒绝、跨用户拒绝通过 |
| 转换界面 | 安装弹窗、格式选择、并行数设置通过 |
| MCP 界面 | stdio JSON 导入、启动命令和参数显示通过 |
| Linux 运行 | Debian 12 ARM64 容器运行真实 Rust 服务；HTTP 服务为应用账户，MCP 为登录用户 |
| stdio MCP | 初始化、工具列表、实际工具调用、参数和环境变量、保存后读取凭据通过 |
| 用户隔离 | 拒绝 root、应用账户、无效 UID 和非应用对端；跨用户文件和应用密钥目录不可读 |
| 进程清理 | 断开连接后主进程和子进程均结束；应用停止后两个 Socket 均清理 |
| Docling 安装 | 从官方源安装 Python 3.12、Docling 2.140.0、格式组件与模型，无需 C 编译器 |
| 实际文件转换 | PDF、DOCX、XLSX、PPTX 自动生成 Markdown、索引和检索；原始字节保持不变；纯标题 PDF 可检索 |
| 转换权限 | 新文件的当前用户访问权限、原文件权限撤销、只读复用、写入拒绝、删除权限及符号链接检查 |
| 文档 | 中文和英文各 4 个页面；转换与 MCP 截图、页面内容和站内搜索通过 |

浏览器测试使用真实 SAG 服务、临时文件和模拟的飞牛网关。Linux 测试使用真实 Linux 用户、真实子进程和真实 Docling；飞牛授权 API 仍为测试接口。没有调用付费模型。

## 复现

```sh
npm run build
npm test
cargo test --workspace
cargo build -p sag-desktop-fnos
npm run test:e2e
npm --prefix docs-site run build
# 启动文档站后：
node tests/fnos-docs.mjs
```

Linux 集成测试：

```sh
docker build -f tests/Dockerfile.fnos -t sag-fnos-test .
docker run --rm \
  -v "$PWD:/source:ro" \
  -v "$PWD/target/aarch64-unknown-linux-gnu/release:/binaries:ro" \
  sag-fnos-test python3 /source/tests/fnos-linux.py --docling
```

必须使用与容器架构匹配的 Linux 程序。`--docling` 会联网下载数 GB 的组件与模型。省略该参数可只验证生命周期和 stdio MCP。

## 安装包

- `release/sag-desktop-fnos-0.1.9-x86.fpk`
- `release/sag-desktop-fnos-0.1.9-arm.fpk`
- 对应 `.sha256` 校验文件

使用官方 fnpack 1.2.3 打包；包含 HTTP 服务与用户进程服务两个 Linux 程序。旧安装包已清理。

## 实机状态

0.2.3 已在 fnOS 1.2.0701（Intel N150）升级并打开，实机通过：

- 目录弹窗布局、原生选择器确认和目录名称显示。
- 添加当前用户已授权的 File 目录，并持久显示于侧边栏。
- 在该目录新建并保存 `SAG-飞牛验证-20261021.md`。
- 自动索引后检索 `fnosnativeverification20261021`，返回 1 个文件，显示 5 ms 本地匹配及正确原文。
- 读取与写入使用真实网关用户身份和原生权限接口。跨用户、撤销权限及只读拒绝仍由本地/Linux 测试覆盖，未在这台设备重新逐项验证。

测试 Markdown 和 PDF 保留供用户复查。后续用户已配置模型，深入检索与内置 MCP 文件工具已在实机运行。文本 PDF 和扫描 PDF 的转换、索引、搜索及扫描件问答已在 0.2.6 实机通过；Office 转换与 stdio MCP 已通过此前 Linux 集成测试，尚未在这台 NAS 运行。

诊断过程确认：0.2.0 的来源校验在真实网关上误拒绝；0.2.1 修复后发现路径 API 返回数组；0.2.2 仅用于读取该响应；0.2.3 已移除临时诊断并兼容实际格式。


保留 Docling 原有全部格式选项，本次实际转换覆盖 PDF、DOCX、XLSX、PPTX；其余格式未逐种实测。

## 界面截图

- `.local/qa/fnos-real-folder-picker.png`（实机）
- `.local/qa/fnos-real-search.png`（实机）
- `docs-site/public/images/fnos-folder-picker.png`
- `docs-site/public/images/fnos-search.png`
- `docs-site/public/images/fnos-settings.png`
- `docs-site/public/images/fnos-docling-formats.png`
- `docs-site/public/images/fnos-mcp-stdio.png`
