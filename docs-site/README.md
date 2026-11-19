# SAG Wiki 文档站 / Documentation site

文档站位于 SAG Wiki 仓库的 `docs-site/`，与桌面端、CLI 一起提交和同步到 GitHub、GitLab。使用 Next.js、Fumadocs、React 和 Tailwind CSS，独立运行，不会启动或修改桌面应用。

The documentation site lives in `docs-site/` within the SAG Wiki repository. Commit and synchronize it with the desktop app and CLI to GitHub and GitLab. It uses Next.js, Fumadocs, React and Tailwind CSS and runs independently from the desktop app.

## 本地运行 / Local preview

在仓库根目录运行 / From the repository root:

```sh
npm --prefix docs-site ci
npm --prefix docs-site run dev -- --hostname 127.0.0.1 --port 3219
```

- 简体中文：http://127.0.0.1:3219/zh/docs
- English: http://127.0.0.1:3219/en/docs
- 繁體中文：http://127.0.0.1:3219/zh-TW/docs
- 日本語：http://127.0.0.1:3219/ja/docs
- 한국어：http://127.0.0.1:3219/ko/docs
- Español: http://127.0.0.1:3219/es/docs

访问根地址时，站点根据请求的 `Accept-Language` 匹配文档语言。没有匹配项时进入简体中文首页。请求缺少此标头或只接受任意语言（`*`）时进入英文首页。带有受支持语言前缀的地址按该语言查找页面。

The root URL matches the request's `Accept-Language` header against the supported locales. If none match, it opens the Simplified Chinese home. A missing header or a wildcard-only preference (`*`) opens the English home. A URL with a supported locale prefix looks up the page in that locale.

直接访问缺少繁体中文、日语、韩语或西班牙语译文的页面时，站点优先跳转到英文版，其次是简体中文版。在语言菜单中切换到缺少当前页面译文的语言时，会打开该语言的文档首页。搜索只收录所选语言的页面，导航也会标出可参阅的英文资料。

A direct link to an untranslated Traditional Chinese, Japanese, Korean, or Spanish page opens the English version if available, with Simplified Chinese as the next fallback. Switching languages from a page without a translation opens the selected language’s documentation home. Search indexes pages in the selected language, and navigation also includes labeled links to English references.

## 构建与验证 / Build and verify

`docs-site` 可单独复制到 Docker 构建目录，无需上层仓库、根目录更新日志或 Git：

You can copy `docs-site` into its own Docker build context. Building it requires no parent repository, root changelog or Git:

```sh
cd docs-site
npm ci
npm run build
npm run start -- --hostname 0.0.0.0 --port 3000
```

从完整仓库运行 / From the full repository:

```sh
npm --prefix docs-site run types:check
npm --prefix docs-site run build
npm --prefix docs-site run start -- --hostname 127.0.0.1 --port 3219
```

## 内容维护 / Content maintenance

- `content/docs/<slug>.mdx`：简体中文页面 / Simplified Chinese pages.
- `content/docs/<slug>.<locale>.mdx`：其他语言的对应页面，使用相同的 slug，语言后缀为 `en`、`zh-TW`、`ja`、`ko` 或 `es` / Translations with the same slug and an `en`, `zh-TW`, `ja`, `ko`, or `es` suffix.
- `content/docs/meta.json`、`content/docs/meta.<locale>.json`：分别维护简体中文和其他语言的导航 / Navigation for Simplified Chinese and each translated locale, respectively.
- `public/images/`：正式产品截图及品牌图标 / Product screenshots and brand assets.
- `package-lock.json`：提交到仓库，使用 `npm ci` 安装锁定依赖 / Commit this lockfile and install with `npm ci`.

功能变化时同步更新中英文内容与相关截图。维护规则见仓库根目录的 [AGENTS.md](../AGENTS.md)。

Update both languages and affected screenshots when product behavior changes. See [AGENTS.md](../AGENTS.md) for the maintenance rules.

`node_modules/`、`.next/`、`.source/`、`qa/` 及 TypeScript 构建缓存仅在本地使用，不提交。站点本身没有独立 Git 仓库，也没有配置单独的 Git 远程。

Dependencies, build outputs, generated sources, QA artifacts and TypeScript caches are local only. The site has no nested Git repository or separate Git remotes.

本地预览不代表线上部署；仓库不预设文档托管平台。安装链接指向已发布的桌面版本。

A local preview is not a deployment. This repository does not prescribe a hosting platform. Downloads point to published desktop installers.

## Release downloads / 版本下载信息

README 使用 GitHub 最新正式版入口。安装页的 `ReleaseDownloads` 组件在浏览器中读取公开的 GitHub Latest Release API，显示版本、macOS/Windows 安装包链接和实际大小（十进制 MB，一位小数）。版本及附件经过校验；超时、限流或附件不完整时显示提示，用户可通过页面上的最新正式版链接下载。发布新版本后，重新打开安装页即可获取新信息，无需修改配置或提交文档 PR。

README links to the latest stable GitHub Release. The installation page's `ReleaseDownloads` component reads the public Latest Release API in the browser and displays the version, macOS/Windows installers and actual sizes (decimal MB, one decimal place). It validates the release and assets. If requests time out, are rate-limited, or return incomplete assets, the page points readers to its latest-release link. Reopening the page after a release retrieves current downloads without a configuration update or documentation PR.

## Changelog generation and deployment verification / 更新日志生成与部署验证

版本记录只编辑根目录 `CHANGELOG.md`；每版包含中文及 `### English` 英文说明。旧文档页独有的历史版本已迁回此文件。运行以下命令后提交根日志及两个生成的 MDX 文件：

Edit only the root `CHANGELOG.md`, with Chinese and `### English` entries per version. Historical entries previously unique to the site have been migrated into that source. Generate and commit both MDX outputs with the root changelog:

```sh
npm --prefix docs-site run changelog:generate
npm --prefix docs-site run changelog:lint
npm --prefix docs-site run build
```

PR CI 对所有 PR 强制执行 lint，不依赖是否触发桌面发布。独立构建直接使用已提交的双语 MDX 页面，不访问上层目录。需要源码提交校验信息的发布构建，在完整仓库中先运行 `node scripts/documentation-release.mjs build`，再运行 `npm --prefix docs-site run build`；桌面发布 CI 已包含这两步。前一步校验日志并生成不入库的 `public/docs-version.json`，记录源码提交、日志哈希、版本列表和双语内容哈希。部署必须使用同一次构建的产物。

PR CI lints every PR independently of desktop-release eligibility. A standalone build uses the committed bilingual MDX pages without accessing parent directories. For a release build with source provenance, run `node scripts/documentation-release.mjs build` from the full repository before `npm --prefix docs-site run build`; desktop release CI includes both steps. The first step validates the changelogs and emits an untracked `public/docs-version.json` containing the source commit, changelog digest, versions and bilingual content digests. Deploy the outputs from that same build.

在实际部署完成步骤中，检出预期提交后执行以下命令；线上版本信息及两个实际页面的内容标记均须一致。旧缓存、部分部署、错误提交或未收录的发布版本会使命令失败。校验不要求文档最高版本等于最新 Release，允许提前编写下一版本。

After deployment, check out the expected source commit and run the following command. It checks the online manifest and both actual page markers, rejecting stale caches, partial deployments, wrong commits and missing releases. It allows documentation to include a future version rather than requiring its highest version to equal the latest release.

```sh
npm --prefix docs-site run verify:deployment -- --url https://your-docs-host.example --expected-sha <full-commit-sha> --release v0.3.5
```

`--release` 可省略；提交及内容一致性始终检查。此命令在完整仓库中运行，部署产物须包含上述发布构建生成的 `docs-version.json`。此仓库没有实际部署任务，本命令供部署完成步骤调用，不会部署站点或发布桌面新版本。

`--release` is optional; source and content consistency are always checked. Run this command from the full repository, and deploy the `docs-version.json` produced by the release preparation step above. No deployment job is configured in this repository. This command is a post-deployment hook, not a site deployment or a desktop release.
