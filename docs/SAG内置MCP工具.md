# SAG 内置 MCP 工具

SAG 内置 MCP 提供六个只读工具。客户端通过 MCP 协议发现和调用工具，连接位于同一个 Rust 进程内，不开放 HTTP 端口，不启动额外程序。工具定义的唯一来源是 `src/lib/sag-tools.json`，前端目录和 Rust MCP 共用这份定义。

模型调用中的名称带有 `sag2_` 前缀，避免与其他服务重名；界面显示为 `SAG · search`、`SAG · list`、`SAG · read`、`SAG · grep`、`SAG · getTime`、`SAG · calculate`。

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

### getTime

不接受参数，读取本机时钟，返回 `local_time`、`utc_time`、`timezone_offset` 和 `unix_ms`。本地时间与 UTC 均为带时区的 RFC 3339 格式。查询时间时应实际调用，不能复用旧的查询结果。

### calculate

接受必填字符串 `expression`，如 `{"expression":"sqrt(283298439.39)"}`。支持以下表达式，最多 2000 字节，最多嵌套 64 层；不接受代码、变量赋值或自定义函数。

| 运算 | 写法 |
| --- | --- |
| 四则运算、取余、括号 | `+ - * / % ( )` |
| 乘方、平方根 | `x^y`、`x**y`、`pow(x,y)`、`sqrt(x)` |
| 科学计数法、常量 | `1.2e3`、`5E-2`、`pi`、`e`（常量也支持 `pi()`、`e()`） |
| 绝对值、取整、符号 | `abs(x)`、`floor(x)`、`ceil(x)`、`trunc(x)` / `int(x)`、`sign(x)` |
| 四舍五入 | `round(x)`、`round(x,n)`；n 为 0～28，中点向远离零的方向取整 |
| 最大值、最小值 | `max(x,y,...)`、`min(x,y,...)`，至少一个参数 |
| 对数、指数 | `ln(x)`、`log10(x)` / `log(x)`、`log(base,x)`、`exp(x)` |
| 三角函数、角度转换 | `sin(x)`、`cos(x)`、`tan(x)`，参数为弧度；`radians(x)`、`degrees(x)` |

乘方从右向左结合，优先级高于一元负号：`2^3^2 = 512`，`-2^2 = -4`。函数名不区分大小写。

使用十进制定长计算，`0.1 + 0.2` 返回 `"0.3"`，结果以字符串提供，避免转成浮点数时再次丢失精度。支持最多 28 位小数；科学函数为近似值，实际精度取决于函数及输入，输出位数不代表准确位数。运算可能按可用精度舍入，除不尽的小数不代表无限精度。除零、负数平方根、无效对数、超出数值范围或无效表达式会返回明确错误。

计算器参考 [Strands calculator](https://github.com/strands-agents/tools/blob/main/src/strands_tools/calculator.py) 先限制表达式语法再求值的方式，以及 [fasteval](https://github.com/likebike/fasteval) 的常用函数接口；实际运算使用 `rust_decimal` 的 `maths` 功能，不将普通十进制计算转换为二进制浮点数。

这两个工具不访问知识库，不受文件查询范围限制，不生成文件引用，沿用按模型启用和关闭的设置。需要时间或数值运算时，系统提示模型先调用对应工具，无需为纯计算和时间查询检索文件。

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

- 六个工具都不会写入、移动或删除原文件。越过已添加目录或本轮范围的路径请求会失败，符号链接不能扩大访问范围。
- read、grep、search 统一使用本轮 source_id、文件版本哈希和原文位置。相同文件版本、相同原文范围去重并复用引用编号。
- 每个工具默认独立启用，可按模型关闭。
- SAG 与外部 MCP 共用每轮调用次数和回答上下文预算。search 可按预算省略片段，并明确返回 omitted_sources；其他工具结果过大时保留完整调用记录，并明确告诉模型缩小请求，避免静默截断或无效引用。
- 工具参数、返回结果和失败原因显示在对话调用卡片中，默认折叠并随历史保存。
