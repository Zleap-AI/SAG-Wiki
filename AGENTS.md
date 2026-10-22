# SAG Wiki 文档维护

- 文档站与桌面端共用本仓库，站点源码位于 `docs-site/`。修改功能、界面操作、CLI 参数、安装方式或支持范围时，同一任务中同步更新 `docs-site/content/docs/` 对应的中文与英文页面；以实际代码和已验证行为为准。
- 界面变化涉及文档截图时，同步更新 `docs-site/public/images/` 的演示截图，隐藏个人信息和凭据。下载链接只指向已发布版本。
- 文档更新后运行 `npm --prefix docs-site run build`，检查受影响页面、链接和搜索结果；完成时说明文档同步结果。发布按用户当次指令执行。
- 文档站源码、内容、正式图片和依赖锁文件随本仓库同步到 GitHub 与 GitLab。`node_modules/`、`.next/`、`.source/`、`qa/` 等本地产物不提交。
