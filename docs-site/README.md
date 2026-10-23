# SAG Wiki 文档站 / Documentation site

文档站位于 SAG Wiki 仓库的 `docs-site/`，与桌面端、CLI 一起维护。使用 Next.js、Fumadocs、React 和 Tailwind CSS，独立运行，不会启动或修改桌面应用。

The documentation site lives in `docs-site/` within the SAG Wiki repository. Maintain it alongside the desktop app and CLI. It uses Next.js, Fumadocs, React and Tailwind CSS and runs independently from the desktop app.

## 本地运行 / Local preview

在仓库根目录运行 / From the repository root:

```sh
npm --prefix docs-site ci
npm --prefix docs-site run dev -- --hostname 127.0.0.1 --port 3219
```

- 中文：http://127.0.0.1:3219/zh/docs
- English: http://127.0.0.1:3219/en/docs

访问根地址时按浏览器首选语言选择中文或英文；其他语言或未提供语言时默认英文。带有 `/zh/` 或 `/en/` 的地址保持指定语言。

The root URL selects Chinese or English from the browser's preferred languages. Other languages or a missing language preference default to English. URLs with `/zh/` or `/en/` retain that language.

## 构建与验证 / Build and verify

```sh
npm --prefix docs-site run types:check
npm --prefix docs-site run build
npm --prefix docs-site run start -- --hostname 127.0.0.1 --port 3219
```

## 内容维护 / Content maintenance

- `content/docs/*.mdx`：中文页面 / Chinese pages.
- `content/docs/*.en.mdx`：对应英文页面，保持相同 slug / Matching English pages with the same slugs.
- `content/docs/meta.json`、`meta.en.json`：双语导航 / Navigation in both languages.
- `public/images/`：正式产品截图及品牌图标 / Product screenshots and brand assets.
- `package-lock.json`：提交到仓库，使用 `npm ci` 安装锁定依赖 / Commit this lockfile and install with `npm ci`.

功能变化时同步更新中英文内容与相关截图。维护规则见仓库根目录的 [AGENTS.md](../AGENTS.md)。

Update both languages and affected screenshots when product behavior changes. See [AGENTS.md](../AGENTS.md) for the maintenance rules.

`node_modules/`、`.next/`、`.source/`、`qa/` 及 TypeScript 构建缓存仅在本地使用，不提交。站点本身没有独立 Git 仓库，也没有配置单独的 Git 远程。

Dependencies, build outputs, generated sources, QA artifacts and TypeScript caches are local only. The site has no nested Git repository or separate Git remotes.

当前仅提供本地预览，未部署。安装链接指向已发布的 0.1.9。

This is a local preview, not a deployed site. Downloads point to released 0.1.9 installers.
