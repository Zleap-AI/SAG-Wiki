> fnOS development now lives in [Zleap-AI/SAG-Wiki · fnos/develop](https://github.com/Zleap-AI/SAG-Wiki/tree/fnos/develop). GitHub builds x86-64 FPKs only; look for `fnos-` tags in [Releases](https://github.com/Zleap-AI/SAG-Wiki/releases). Windows/macOS development stays on `main`. See [CONTRIBUTING.md](CONTRIBUTING.md) for release and synchronization rules.

# SAG Desktop for fnOS

独立的飞牛 OS 原生应用项目。沿用 SAG Desktop 的 Svelte 界面和 Rust 内核，通过飞牛统一网关运行，提供原生目录授权、索引、搜索、引用问答和 Markdown 编辑。

当前源码版本为 **0.1.13**。平台通过安装包名称区分。此前 0.2.x 为迁移测试编号，已有修复与功能保留。0.1.13 同步了桌面端 `main` 分支（GitHub Zleap-AI/SAG-Wiki）的修复与功能：内置 PDF、Office、EPUB 转换，本地 Jev 无鉴权接入，检索跳过已变更或不可读文件，不可读子目录不再中断索引，以及组件安装进度、模型源连通性和安装日志。

`main` 更新后由 GitLab CI 测试、构建并发布 x86-64 / ARM64 FPK。[最新发布](https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos/-/releases/permalink/latest)包含安装包、SHA-256 和源码提交信息。发布标记包含应用版本、提交号和流水线 ID；FPK 内的应用版本保持源码版本。

## 使用

要求 fnOS **1.2.0401+**；移动端相关能力要求飞牛 App **1.34.0+**。

下载对应架构的安装包，在飞牛应用中心手动安装。从飞牛桌面打开 SAG Wiki，点击“添加文件夹 → 选择并授权文件夹”，授权后添加索引。

| 架构 | 最新自动构建 FPK | SHA-256 |
| --- | --- | --- |
| x86-64 | [下载 FPK](https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos/-/releases/permalink/latest/downloads/sag-desktop-fnos-x86.fpk) | [校验文件](https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos/-/releases/permalink/latest/downloads/sag-desktop-fnos-x86.fpk.sha256) |
| ARM64 | [下载 FPK](https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos/-/releases/permalink/latest/downloads/sag-desktop-fnos-arm.fpk) | [校验文件](https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos/-/releases/permalink/latest/downloads/sag-desktop-fnos-arm.fpk.sha256) |

仓库与附件为私有内容，下载需要对应 GitLab 访问权限。旧的 [0.1.9 指引修订发布](https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos/-/releases/v0.1.9-fnos-agent-guide)仍保留。已装 0.2.x 迁移测试版的设备会因飞牛版本检查而拒绝直接降级；请保留现有应用数据。

快速搜索不需要模型密钥。SAG2 检索需配置 Jev；问答需配置问答模型。

## 原生能力

- 官方 `@trimjs/web-app`：文件夹选择与授权、系统文件管理器、应用设置、宿主主题。
- `trim.file.getUserAccessibleFolders`：获取当前用户授权目录。
- `trim.file.checkUserACL`：读取、索引、搜索、写入和删除转换文件前检查当前用户权限。
- `trim.file.convertPath`：授权列表显示可读路径。
- 统一网关 `X-Trim-Userid`：按用户隔离 SQLite 索引、模型密钥、会话和草稿。
- 独立浏览器通过飞牛授权页面返回应用，校验一次性 state。

网页服务通过 Unix Socket 接入飞牛网关；远程 MCP 使用独立的 15679 端口，需要当前用户生成的访问密钥，仅提供只读知识库工具。局域网或 VPN 可直接访问，公网接入需使用 HTTPS 反向代理。系统 API 令牌只在后端读取，不持久化、不发送给前端。新文件按父目录写入权限检查；符号链接按真实路径检查授权范围。

## 范围

保留 Markdown、文本、RTF、代码、配置文件、SAG2 检索、引用问答和编辑能力。PDF、Word、PowerPoint、Excel/CSV、OpenDocument、EPUB 由内置 anydoc 转换器在应用进程内转换，无需下载组件；图片识别、扫描件 OCR、音视频转写及 iWork 等其他格式通过 Docling 组件按需转换。MCP 支持远程 HTTP 与本地 stdio；CLI 保留原版搜索、匹配、配置、索引管理、服务管理和 Agent Skill 安装命令，并支持 `sag config --auth none` 接入无鉴权的本地 Jev。应用升级、启停和桌面入口由飞牛管理。

内置格式添加文件夹后自动转换。在“设置 → 文件设置 → 其他格式”勾选需要组件的格式时会提示安装 Docling，并可查看安装进度、各包下载状态和安装日志；安装前先检查模型服务可达性，无法连接时立即报错并提示 `HF_ENDPOINT` 镜像。扫描件在未安装组件时记为索引问题，不会自动下载。组件按飞牛用户单独保存，转换在 NAS 上执行，原文件保留，Markdown 输出到原目录内的 `.sag-converted/`。转换输出为当前用户设置文件访问权限；读取转换文件时同时检查原文件权限；只读目录可复用已有转换结果，创建、覆盖、删除分别检查权限。

在“设置 → 问答设置 → 工具绑定”添加或导入 stdio 配置，填写 NAS 上的启动命令、参数和环境变量，再测试连接并绑定工具。可执行文件和对应运行环境需要已在 NAS 安装。命令以当前飞牛登录用户的 UID 和用户组运行；每个用户有独立工作目录，只接收显式配置的环境变量。

生命周期脚本以 root 启动内部进程服务，并把 HTTP 服务切换到应用账户。内部服务启动时恢复应用私有数据的账户归属，兼容飞牛卸载后保留但改为 root 所有的数据目录。内部服务只接受应用账户通过私有 Unix Socket 发来的请求，拒绝使用 root 或应用账户执行 MCP；连接关闭和应用停止时清理整个子进程组。飞牛 API 令牌不会继承到 MCP 或 Docling 子进程。

已在 fnOS 1.2.0701 安装并打开。0.2.1 修复目录弹窗布局和网关来源校验，0.2.3 兼容实机直接返回的路径数组与文档中的路径响应格式。该设备已验证原生选择器、目录显示和添加、Markdown 创建与保存、自动索引和快速搜索。PDF/Office 与 stdio MCP 的验证记录见 VERIFICATION.md。

## CLI

安装包包含原生 `sag` 命令。在 NAS 的 SSH 终端执行：

```sh
/var/apps/sag-desktop-fnos/target/bin/sag install cli
# 重新打开终端
sag status
sag check /vol1/1000/File --json
sag -F "关键词" /vol1/1000/File --json
```

网页的“设置 → CLI”可开关接入并运行实际命令。CLI 使用系统提供的登录 UID，共用该用户网页中的索引与 Jev 设置；索引和检索执行飞牛原生权限检查。`sag service stop/restart` 仅管理该用户的共享服务；下一次操作会重新启动。更多参数见 `docs-site/content/docs/cli.zh.mdx`。

也可在网页中执行 `sag install cli`。当前账号的主目录尚未创建时，安装流程会创建该目录并归属当前用户，已有目录权限保持不变。

## 开发

### 自动构建与发布

`.gitlab-ci.yml` 使用现有 `docker` 标签 Runner：

- `main` push 或在 **Build → Pipelines → New pipeline** 选择 `main` 手动触发：检查、双架构构建、发布。
- Merge Request 或手动选择其他分支：执行检查和双架构构建，供评审下载安装包，不发布 Release。
- 自动创建的发布标签不触发新流水线，避免循环。

`checks` 执行前端检查/测试、以非 root 用户运行 Rust workspace 测试，并执行发布脚本测试。`build-fpk` 按 x86 / arm 矩阵构建，核对包内版本、架构、执行权限及二进制；x86 额外运行真实 worker 的 Linux 发布集成测试。任一检查或架构失败都不会执行发布。

`publish-fpk` 使用 GitLab 自动提供的临时 `CI_JOB_TOKEN`，无需配置个人 Token。验证两个架构及 sidecar 后上传 Generic Packages，并逐个下载校验，再创建 Release。包版本/标签形如 `0.1.13-main.<commit8>.<pipeline-id>` / `v0.1.13-main.<commit8>.<pipeline-id>`，重试不会覆盖已有不同内容。流水线 artifacts 保留 30 天；Release 下载指向 Package Registry，不依赖临时 artifacts。`build-info.json` 和 `audit-*.json` 记录源码提交、流水线及二进制哈希。

默认使用 Runner 常用的 `python:3.11` 构建环境，首次安装固定版本的 Rust 并缓存工具链。可通过项目变量 `FPK_BUILD_IMAGE` / `FPK_PUBLISH_IMAGE` 切换为运维提供的私有构建/发布镜像。Rust 组件使用 RsProxy 国内分发镜像，版本固定且由 rustup 校验组件哈希；可用 `RUSTUP_DIST_SERVER` 覆盖下载源。工具链固定为 rustup 1.29.2、Rust 1.99.1、Node 24.13.0、cargo-zigbuild 0.24.0、Zig 0.17.0、官方 fnpack 1.2.3；rustup、Node 与 fnpack 下载有固定 SHA-256 校验。构建工具缓存由检查和架构构建共享，Rust 依赖及编译缓存按任务隔离。CI 构建检查不等于 fnOS 实机验收，ARM64 安装运行仍需相应设备验证。

前端依赖默认使用 npmmirror，可用 `NPM_CONFIG_REGISTRY` 覆盖；`npm ci` 仍校验 lockfile 中的完整性哈希。安装步骤有 10 分钟总超时及进程日志，避免网络故障时持续静默等待。

系统包及 Python 构建工具默认从阿里云公共镜像下载，可用 `DEBIAN_MIRROR` / `PIP_INDEX_URL` 覆盖。APT 保留仓库签名与包哈希验证，下载有请求超时和重试。

Rust crate 依赖使用 RsProxy sparse 镜像，Cargo 按 `Cargo.lock` 校验版本与哈希。可用 `CARGO_REGISTRY_MIRROR` 切换内部 sparse 源，设为空则使用 crates.io；该配置写入 CI 专用的 `CARGO_HOME`。

Release 下载链接使用 `FPK_PUBLIC_API_V4_URL` 指定的对外 HTTPS 地址，Runner 上传仍使用 GitLab 内网 API。项目 CI 变量 `CARGO_NET_RETRY=10` 用于容忍下载源偶发 DNS 失败。

私有构建镜像的预装定义见 `scripts/ci/Dockerfile`：

```sh
docker build --platform linux/amd64 -f scripts/ci/Dockerfile scripts/ci -t <私有仓库>/sag-wiki-fnos-ci:rust-1.99.1-v1
```

该镜像预装完整工具链；仓库和 Runner 拉取权限就绪后再切换，当前流水线可使用默认环境运行。

### 本地开发

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

也可传入预编译的 Linux 程序：`npm run package:fnos -- --arch x86 --binary /path/to/sag-desktop-fnos`。该程序旁必须同时提供 `sag-fnos-worker` 和启用 `fnos` 功能编译的 `sag`。脚本检查三个 ELF 的架构，输出 FPK 和 SHA-256。

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
| `cli/` | 原版命令与飞牛连接适配 |
| `backend/` | Unix Socket 服务、权限和用户隔离 |
| `core/`、`client/`、`protocol/` | SAG 内核源码副本 |
| `packaging/` | 飞牛入口、权限、启动与停止脚本 |
| `docs-site/` | 中英文使用和开发文档 |
| `release/` | 本地安装包，不纳入 Git；正式包发布到 GitLab 发布附件 |

源码基于本机 `SAG-Desktop` 的当前工作区复制，基线 HEAD 为 `cecff6f2d72d4eea6c5def538aa1d865f4532297`，包括复制时的未提交修改。原项目未被本项目修改。

依据：[飞牛完整开发指引](https://developer.fnnas.com/llms-full.txt)。许可：MIT；第三方声明见 `licenses/`。
