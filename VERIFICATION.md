# 飞牛版验证记录

版本：0.1.9。验证日期：2026-10-22。

## 2026-10-22 电脑远程接入

新增独立的 Streamable HTTP MCP 入口（15679 端口），电脑无需 SSH 即可调用现有只读文件工具。访问密钥绑定飞牛用户，仅保存 SHA-256 摘要；网页生成与撤销密钥，远程请求不接受客户端指定 UID。查询前后检查原生权限、CLI 开关和密钥状态。Codex 配置通过界面复制，密钥不会出现在页面代码示例或截图中。

374 项 Rust 测试通过，3 项原有测试跳过；前端 7 项测试通过，Svelte 检查无错误或警告。新增测试覆盖真实 MCP 客户端握手、原生文件读取、共享 Jev 搜索、用户隔离、伪造 UID、写入拒绝、密钥轮换、CLI 关闭与查询期间撤销密钥。Linux 包内程序通过真实 TCP MCP 握手、读取、原生 ACL 和撤销验证，原有 CLI、stdio MCP、生命周期测试保持通过。

实机再次重装时发现飞牛把保留的 `remote-access.json` 归属改为 root，导致新服务读取失败。Linux 用相同的 root 所有、0600 权限配置复现 `Permission denied`；启动脚本现恢复该配置文件的应用账户归属与 0600 权限，保留原有密钥摘要。修复后的完整 Linux 生命周期、远程 MCP 和 CLI 验证通过。

完整回归发现原有 20 进程并发用例在共享服务的启动扫描完成前发出查询，部分请求报告索引未就绪。测试准备阶段现先添加目录并确认索引就绪，再验证 20 个请求均到达模型后才返回结果；没有修改 CLI 运行逻辑。完整回归随后通过。

0.1.9 在 fnOS 1.2.0701 测试机卸载重装后，恢复原 File 目录授权，37 个文件索引 ready。电脑通过已有 Tailscale 网络直接连接 MCP，完成 initialize、tools/list、search 和 read；扫描 PDF 的 OCR 文本可检索，原文包含 3 册、林青等预期字段。本机 Codex 配置已加入 sag_fnos，Codex CLI 确认配置可解析；当前会话的工具列表需在新会话加载，未另行启动代理模型会话。未采用公网端口转发或手动 SSH。

最终安装后再次确认原密钥仍可连接；轮换密钥后旧密钥返回 401，新密钥通过远程搜索、OCR 原文读取与 Codex 配置检查。重新打开网页后连接地址保留、远程状态为“已启用”；实际执行 `sag check --json` 返回退出码 0、37 个文件且索引就绪。

证据保存在 `.local/qa/remote-final-verification.json`、`.local/qa/remote-final-search.json`、`.local/qa/remote-final-read.json`、`.local/qa/fnos-remote-final.png`、`.local/remote-workspace-final.log`、`.local/remote-reinstall-green.log`。访问密钥只保存在本机私有配置及忽略的验证目录，不提交到仓库。x86、ARM FPK 保持版本 0.1.9；安装包架构、执行权限、服务端口与 SHA-256 均已检查。

中英文 CLI、安装与开发文档以及设置截图已同步。文档构建、六个相关页面、双语搜索、图片加载均通过；新安装包及校验文件上传后重新下载，四个文件的 SHA-256 与本地一致。

## 2026-10-22 接入 Agent 入口修复

实机复现右上角显示“飞牛设置”，点击打开飞牛设置页。与原版对照后确认迁移时改动了按钮文案与目标页，补齐 CLI 时未恢复。本次仅将按钮文字、无障碍名称和提示恢复为“接入 Agent”，点击目标恢复为 CLI；飞牛设置仍在设置菜单中。

前端构建及 Svelte 检查通过。Linux 真实服务页面与 fnOS 1.2.0701 实机均验证右上角文案和点击目标。按用户要求保持 0.1.9，在测试机卸载重装当前包；恢复原有 File 授权并重新索引后，实际执行 `sag check --json` 返回退出码 0、`index_ready: true`、37 个文件及 `ready` 状态。未改动 CLI、搜索和转换逻辑。

x86、ARM 安装包均核对 manifest 版本、三个程序的 Linux ELF 架构、执行权限及 SHA-256；GitLab 上传后下载核验一致。证据：`.local/qa/agent-entry-before.json`、`.local/qa/agent-entry-local-after.json`、`.local/qa/agent-entry-nas-after.json`、`docs-site/public/images/fnos-agent-entry.png`。本次发布使用 `v0.1.9-fnos-cli.1` 标记，应用版本仍为 0.1.9。 中英文 CLI、安装文档与入口截图已同步；文档构建、四个相关页面、下载链接、图片加载与双语搜索验证通过。

## 2026-10-22 CLI 补齐

复用原版 CLI 的命令、匹配参数、输出格式、退出码、配置和 Skill 安装逻辑；新增飞牛 Unix Socket 连接，以内核提供的进程 UID 关联该用户的网页数据。只开放 CLI 所需操作，搜索前后复查原生权限，`--types` 写入前检查 `.sagignore` 权限。CLI 不使用 root 或应用账户。

网页“设置 → CLI”可即时开关接入，并通过用户进程运行包内 `sag`，显示实际输出和退出码。只解析 sag 参数，不通过 Shell 执行。SSH 安装入口使用受管理的启动器，随应用升级更新。随包 Skill 使用飞牛安装、授权和共享服务说明。

本地测试：371 项 Rust 测试通过，3 项原有测试跳过；前端 7 项测试通过，Svelte 检查无错误或警告。ARM Linux 集成测试通过原版命令、共享索引、权限撤回、跨用户拒绝、root/应用账户拒绝、服务启停、全部安装目标及用户启动器。网页实际执行固定字符串搜索、索引检查和 CLI 开关验证通过。用户反馈的卡片贴边问题复现为 1 像素间距；采用现有 settings-fields 容器后为 18 像素，执行按钮位于底部栏上方，无横向溢出；安装说明和常用命令默认折叠。x86 容器启动测试未通过，原因未查明；不计为验证通过。

FPK 包含 `sag-desktop-fnos`、`sag-fnos-worker` 和 `sag` 三个 Linux 程序，x86 与 ARM 架构分别检查，包内程序与构建输出比对，版本保持 0.1.9。

用户确认接受应用数据可能清除后，通过飞牛应用中心卸载旧的 0.2.6，并按用户要求保持 0.1.9 重新安装。首次进入页面失败：管理员文件管理确认保留的 users/1000 目录仍为 root 所有，而应用服务使用独立账户，初始化无法设置私有目录权限。启动时恢复应用私有目录归属后，页面和用户服务正常打开。临时尝试通过文件管理替换程序时，解压后的程序丢失执行权限；最终安装均使用完整 FPK，包内三个程序的 0755 权限与构建内容已核对。

实机进一步确认普通用户的 /home/admin 尚不存在，网页执行安装命令无法写入用户终端目录。现仅在用户执行 install 命令且其系统分配的主目录不存在时创建该目录，归属当前用户、权限 0700；已有主目录不修改。Linux 集成测试覆盖缺失主目录及已有目录权限保持，全部通过。

最终 0.1.9 在 fnOS 1.2.0701（Intel N150）完成卸载重装。恢复原有 File 授权并重新索引后，37 个文件均为 ready。实机通过 status、list、check、固定字符串匹配、正则计数、无匹配退出码、CLI 禁用拒绝、config --enable、service restart；语义搜索返回扫描 PDF 的 OCR 原文、档案编号和缺页信息。固定字符串及正则匹配沿用原版忽略规则；PDF/Office 派生内容通过语义搜索检索。

实机完成 CLI 与 Codex、Claude Code、OpenCode 三个 Skill 的安装和卸载，所有命令退出码为 0；最后重新安装 CLI，保留 /home/admin/.local/bin/sag 入口。真实 SSH 登录会话未单独测试，网页运行的是同一用户身份下的实际 Linux CLI；Linux 集成测试另已执行安装后的启动器。

界面修正了 CLI 卡片间距、折叠说明、命令修改开关后的状态同步，以及飞牛文件菜单的名称。证据：`.local/qa/fnos-cli-final-installs.json`、`.local/qa/fnos-cli-real-tests.json`、`.local/qa/fnos-cli-final-installed.png`、`.local/qa/fnos-cli-packages.json`。中英文 CLI、使用和开发文档同步，真实设备截图已加入；文档构建、六个页面、图片加载及中英文 CLI 搜索结果均通过。安装包只保留当前 x86、ARM 两个 FPK 及校验文件。

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
