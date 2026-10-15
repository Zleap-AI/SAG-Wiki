# SAG 内置 MCP 工具

更新日期：2026-10-15。

SAG 内置 MCP 提供四个只读工具。客户端通过 MCP 协议发现和调用工具，连接位于同一个 Rust 进程内，不开放 HTTP 端口，不启动额外程序。工具定义的唯一来源是 `src/lib/sag-tools.json`，前端目录和 Rust MCP 共用这份定义。

模型调用中的名称带有 `sag2_` 前缀，避免与其他服务重名；界面显示为 `SAG · search`、`SAG · list`、`SAG · read`、`SAG · grep`。

## 参数

所有 offset 从 0 开始。path 使用绝对路径，只能缩小用户选择的文件范围；省略时使用当前范围。显式引用的文件也在本轮允许范围内。

### list

列出文件和目录，不读取正文。

| 参数 | 类型 | 必填 | 默认值 | 含义 |
| --- | --- | --- | --- | --- |
| path | string | 否 | 当前范围 | 限定目录，也可指定单个文件 |
| pattern | string | 否 | 不筛选 | 匹配相对路径，如 `**/*.rs`、`**/README*` |
| recursive | boolean | 否 | true | 包含子目录 |
| offset | integer | 否 | 0 | 跳过的条目数 |
| length | integer | 否 | 2000 | 最多返回的条目数，必须大于 0 |

返回 `items`（path、type、size、modified_ms）、`offset`、`has_more`、`next_offset`。顺序为稳定的按路径深度优先遍历。遵守隐藏文件显示设置和排除目录设置，不跟随符号链接递归。

### read

读取指定文件的 UTF-8 原文，保留换行。沿用客户端文本解码规则，UTF-8 BOM 不作为正文字符。

| 参数 | 类型 | 必填 | 默认值 | 含义 |
| --- | --- | --- | --- | --- |
| path | string | 是 | — | 文件绝对路径 |
| offset | integer | 否 | 0 | 起始 Unicode 字符位置 |
| length | integer | 否 | 2000 | 最多读取的 Unicode 字符数，必须大于 0 |

字符按 Unicode 码点计算，不按 UTF-8 字节计算。返回 path、offset、length、total_characters、version、has_more、next_offset，以及带 source_id 的原文来源。offset 等于文件长度时返回空内容，超过文件长度时报错。

### grep

按字面文字匹配本地文本，不进行分词、语义扩展或 Jev 排序。

| 参数 | 类型 | 必填 | 默认值 | 含义 |
| --- | --- | --- | --- | --- |
| query | string | 是 | — | 非空的精确匹配文字，不解释为正则表达式 |
| path | string | 否 | 当前范围 | 限定文件或目录 |
| pattern | string | 否 | 不筛选 | 按相对路径筛选文件 |
| case_sensitive | boolean | 否 | false | 区分大小写 |
| context_lines | integer | 否 | 2 | 命中前后各包含的行数 |
| offset | integer | 否 | 0 | 跳过的命中条目数 |
| length | integer | 否 | 50 | 最多返回的命中条目数，必须大于 0 |

返回 matches（path、line、offset、length、context_offset、source_id）、原文 sources 和分页信息。同一行出现多次匹配时，每次匹配各计一个条目。文件顺序稳定，文件内按原文位置排序。

### search

使用现有 SAG2 流程：BM25 召回、词项扩展、Jev 判断、联合排序。

| 参数 | 类型 | 必填 | 默认值 | 含义 |
| --- | --- | --- | --- | --- |
| query | string | 是 | — | 自然语言问题或缺失的信息，1–2000 个字符 |
| path | string | 否 | 当前范围 | 限定文件或目录 |
| top_k | integer | 否 | 检索设置，默认 10 | 最多返回的片段数，范围 1–100，仍受候选数量约束 |

返回 query、按相关性排序的 sources、locations（path、offset、length、source_id）。locations 使用与 read 一致的字符单位，可以直接用于读取命中位置的上下文。候选数量、扩展轮数等仍来自检索设置。

## 工具配合

| 当前信息 | 调用方式 |
| --- | --- |
| 已知文件路径 | 直接 read |
| 只有文件名或扩展名 | list 的 pattern 筛选后 read |
| 有函数名、关键词或报错文字 | grep，必要时 read 命中上下文 |
| 自然语言问题或跨文件查找 | search，必要时 read 补充原文 |
| 返回 has_more=true | 使用 next_offset 继续读取 |
| 证据足够 | 停止调用并依据原文回答 |

这些规则写入 MCP 说明和工具定义。模型不必按固定顺序调用，也不必使用全部工具。

## 范围、引用和预算

- 四个工具都不会写入、移动或删除原文件。越过已添加目录或本轮范围的路径请求会失败，符号链接不能扩大访问范围。
- read、grep、search 统一使用本轮 source_id、文件版本哈希和原文位置。相同文件版本、相同原文范围去重并复用引用编号。
- 每个工具默认独立启用，可按模型关闭；旧版 SAG2 总开关继续控制整个内置工具组，旧配置默认启用四个工具。
- SAG 与外部 MCP 共用每轮调用次数和回答上下文预算。search 可按预算省略片段，并明确返回 omitted_sources；其他工具结果过大时保留完整调用记录，并明确告诉模型缩小请求，避免静默截断或无效引用。
- 工具参数、返回结果和失败原因显示在对话调用卡片中，默认折叠并随历史保存。

## 验证

使用临时目录与本地模拟模型验证 MCP 发现、工具开关、分页、通配符、精确匹配、中文字符和 CRLF 位置、越界路径、符号链接、取消、组合调用和上下文预算。模型测试不会读取用户的原始文档，也不需要真实 API 密钥。具体执行结果见《更新记录》。
