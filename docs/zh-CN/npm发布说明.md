# 发布 @zleap-ai/sag

[English](../en/npm-publishing.md) · [文档目录](README.md)

发布后的安装命令：

```sh
npm install -g @zleap-ai/sag
sag
```

也可使用 `pnpm add -g @zleap-ai/sag` 或 `bun add -g @zleap-ai/sag`。首次运行引导配置 JEV，之后使用 `sag add` 和 `sag "搜索内容"`。运行命令仍为 `sag`。

## 准备发布包

在 SAG-Desktop 项目目录执行：

```sh
node scripts/package-npm-cli.mjs
```

脚本构建当前平台的独立 CLI，生成：

- `releases/npm/sag/`：可发布目录。
- `releases/npm/zleap-ai-sag-0.1.0.tgz`：npm 安装包；文件名随 CLI 版本变化。

npm 包只包含启动入口、预编译程序、说明和包信息；不包含桌面端、项目源码、知识库或用户密钥。安装时没有额外下载和安装脚本，因此不依赖 GitHub 下载地址，也不需要用户安装 Rust。npm 全局安装负责注册 `sag` 命令。

源码中的两个 `package.json` 保持 `private: true`，防止误发开发目录。请发布生成的 `.tgz`，无需修改桌面项目的 `package.json`。

## 登录和发布

需要拥有 npm 账号 `zleap-ai`，或加入 npm 的 `zleap-ai` 组织并获得发布权限。npm 账号与 JEV API 密钥相互独立；用户的模型密钥不放进 npm 包。

```sh
npm login
npm whoami
npm publish ./releases/npm/zleap-ai-sag-0.1.0.tgz --dry-run --access public
```

确认包名、版本、文件列表和支持平台后，正式发布：

```sh
npm publish ./releases/npm/zleap-ai-sag-0.1.0.tgz --access public
```

按 npm 提示完成两步验证。每次发布新版本前更新 `cli/Cargo.toml` 的版本并重新打包；npm 不允许覆盖已经发布的同名同版本。

支持的平台以生成后的 `package.json` 和 README 为准，未包含的平台不会在安装时临时编译。

## 汇总多个平台

每个平台构建后，脚本会保存对应程序到 `releases/cli-native/<版本>/<平台>-<架构>/`。可设置 `SAG_TARGET` 使用已配置的 Rust 交叉编译目标。支持收集 macOS、Linux、Windows 的 x64 和 arm64 构建；尚未构建的平台不会列入生成包。

为多个系统发布同一个版本时，在对应环境构建同一提交、同一版本，把各自 `releases/cli-native/<版本>/` 下的目录汇总到发布机器，再运行打包脚本。检查最终包的支持平台后只发布一次。

参考：[npm 公开包发布说明](https://docs.npmjs.com/creating-and-publishing-scoped-public-packages/)、[npm 发布验证要求](https://docs.npmjs.com/requiring-2fa-for-package-publishing-and-settings-modification/)。
