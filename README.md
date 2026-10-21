# SAG Desktop for fnOS

独立的飞牛 OS 原生应用项目。沿用 SAG Desktop 的 Svelte 界面和 Rust 内核，通过飞牛统一网关运行，提供原生目录授权、索引、搜索、引用问答和 Markdown 编辑。

飞牛版与非飞牛版使用同一产品版本号，当前为 **0.1.9**。平台通过安装包名称区分，不再单独递增飞牛版本。此前 0.2.x 为迁移测试编号，已有修复与功能保留。

## 使用

要求 fnOS **1.2.0401+**；移动端相关能力要求飞牛 App **1.34.0+**。

下载对应架构的安装包，在飞牛应用中心手动安装。从飞牛桌面打开 SAG Wiki，点击“添加文件夹 → 选择并授权文件夹”，授权后添加索引。

| 架构 | 0.1.9 安装包 | SHA-256 |
| --- | --- | --- |
| x86-64 | [下载 FPK](https://gitlab.zexrtq.com/api/v4/projects/134/packages/generic/sag-wiki-fnos/0.1.9/sag-desktop-fnos-0.1.9-x86.fpk) | [校验文件](https://gitlab.zexrtq.com/api/v4/projects/134/packages/generic/sag-wiki-fnos/0.1.9/sag-desktop-fnos-0.1.9-x86.fpk.sha256) |
| ARM64 | [下载 FPK](https://gitlab.zexrtq.com/api/v4/projects/134/packages/generic/sag-wiki-fnos/0.1.9/sag-desktop-fnos-0.1.9-arm.fpk) | [校验文件](https://gitlab.zexrtq.com/api/v4/projects/134/packages/generic/sag-wiki-fnos/0.1.9/sag-desktop-fnos-0.1.9-arm.fpk.sha256) |

仓库与附件为私有内容，下载需要对应 GitLab 访问权限。已装 0.2.x 迁移测试版的设备会因飞牛版本检查而拒绝直接覆盖为 0.1.9；请保留现有应用数据。

快速搜索不需要模型密钥。SAG2 检索需配置 Jev；问答需配置问答模型。

## 原生能力

- 官方 `@trimjs/web-app`：文件夹选择与授权、系统文件管理器、应用设置、宿主主题。
- `trim.file.getUserAccessibleFolders`：获取当前用户授权目录。
- `trim.file.checkUserACL`：读取、索引、搜索、写入和删除转换文件前检查当前用户权限。
- `trim.file.convertPath`：授权列表显示可读路径。
- 统一网关 `X-Trim-Userid`：按用户隔离 SQLite 索引、模型密钥、会话和草稿。
- 独立浏览器通过飞牛授权页面返回应用，校验一次性 state。

服务仅监听 Unix Socket，不开放独立 TCP 端口。系统 API 令牌只在后端读取，不持久化、不发送给前端。新文件按父目录写入权限检查；符号链接按真实路径检查授权范围。

## 范围

保留 Markdown、文本、RTF、代码、配置文件、SAG2 检索、引用问答和编辑能力。PDF、Office 等格式通过 Docling 自动转换；MCP 支持远程 HTTP 与本地 stdio。应用升级、启停和桌面入口由飞牛管理。

在“设置 → 文件设置 → 其他格式”安装 Docling，选择格式和转换并行数。组件按飞牛用户单独保存，转换在 NAS 上执行，原文件保留，Markdown 输出到原目录内的 `.sag-converted/`。转换输出为当前用户设置文件访问权限；读取转换文件时同时检查原文件权限；只读目录可复用已有转换结果，创建、覆盖、删除分别检查权限。

在“设置 → 问答设置 → 工具绑定”添加或导入 stdio 配置，填写 NAS 上的启动命令、参数和环境变量，再测试连接并绑定工具。可执行文件和对应运行环境需要已在 NAS 安装。命令以当前飞牛登录用户的 UID 和用户组运行；每个用户有独立工作目录，只接收显式配置的环境变量。

生命周期脚本以 root 启动内部 MCP 进程服务，并把 HTTP 服务切换到应用账户。内部服务只接受应用账户通过私有 Unix Socket 发来的请求，拒绝使用 root 或应用账户执行 MCP；连接关闭和应用停止时清理整个子进程组。飞牛 API 令牌不会继承到 MCP 或 Docling 子进程。

已在 fnOS 1.2.0701 安装并打开。0.2.1 修复目录弹窗布局和网关来源校验，0.2.3 兼容实机直接返回的路径数组与文档中的路径响应格式。该设备已验证原生选择器、目录显示和添加、Markdown 创建与保存、自动索引和快速搜索。PDF/Office 与 stdio MCP 的验证记录见 VERIFICATION.md。

## 开发

```sh
npm ci
npm run build
cargo test -p sag-desktop-fnos
cargo test -p sag-core --test fnos_permissions
```

打包需要 Rust、Zig、cargo-zigbuild 和官方 fnpack：

```sh
rustup target add x86_64-unknown-linux-gnu aarch64-unknown-linux-gnu
cargo install cargo-zigbuild
export FNPACK=/path/to/fnpack
npm run package:fnos -- --arch x86
npm run package:fnos -- --arch arm
```

也可传入预编译的 Linux 程序：`npm run package:fnos -- --arch x86 --binary /path/to/sag-desktop-fnos`。该程序旁必须同时提供 `sag-fnos-worker`。脚本检查两个 ELF 的架构，输出 FPK 和 SHA-256。

浏览器完整流程测试：

```sh
cargo build -p sag-desktop-fnos
npx playwright install chromium
npm run test:e2e
```

测试只访问临时目录与本地模拟的飞牛 API，不使用真实模型密钥。可设置 `PLAYWRIGHT_CHROMIUM_EXECUTABLE` 使用已安装的 Chrome。

Linux 集成测试位于 `tests/fnos-linux.py`，必须在一次性 root 容器中运行，见 `tests/Dockerfile.fnos`。加 `--docling` 会真实下载组件与模型，测试 PDF、Word、Excel、PowerPoint 转换。无需付费模型密钥。

## 目录

| 目录 | 内容 |
| --- | --- |
| `src/` | Svelte 界面与飞牛 SDK |
| `backend/` | Unix Socket 服务、权限和用户隔离 |
| `core/`、`client/`、`protocol/` | SAG 内核源码副本 |
| `packaging/` | 飞牛入口、权限、启动与停止脚本 |
| `docs-site/` | 中英文使用和开发文档 |
| `release/` | 本地安装包，不纳入 Git；正式包发布到 GitLab 发布附件 |

源码基于本机 `SAG-Desktop` 的当前工作区复制，基线 HEAD 为 `cecff6f2d72d4eea6c5def538aa1d865f4532297`，包括复制时的未提交修改。原项目未被本项目修改。

依据：[飞牛完整开发指引](https://developer.fnnas.com/llms-full.txt)。许可：MIT；第三方声明见 `licenses/`。
