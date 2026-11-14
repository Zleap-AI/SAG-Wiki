# DOCX/PPTX 仅文字模式：实现与性能验收

基线：GitHub main `7b51a7fb642411402d821119118a0faf4244a8de`；实施分支 `feat/desktop-office-text-only`。结束前重新 fetch GitHub main，提交号未变化。内网 GitLab origin 本次不可用，使用 GitHub 基线已经用户确认。

## 行为与边界

- 设置 → 文件设置新增 **Office 仅提取文字**，默认关闭；仅作用于 DOCX/PPTX，旧 DOC/PPT 和宏格式保持原路径。
- 应用层通过 Seek/File 读取 ZIP 目录，以路径和 Content Types 白名单筛选必要文字 XML，在被排除资源的 ZIP 成员打开、读取、解压之前跳过图片、OLE、附件、音视频、缩略图等；图片伪装为 `.xml` 时仍按 MIME 排除。
- 保留文字 XML 的名称和关系、移除图片/OLE 引用，构造内存中的受限包，再交给已有 AnyDoc 0.2.4 解析。正文、表格、文本框及受支持备注沿用原引擎，无自建文字布局解析器。
- 不修改 AnyDoc，不升级依赖、不引入 Java/LibreOffice。直接使用锁文件中已有的 quick-xml 0.41.0；core/cli 锁文件仅增加 sag-core 对它的直接依赖边。
- 开启时普通云解析和 Docling 都不满足资源限制，不能用作回退；即使启用了云解析，此模式也在本地执行。没有合格回退时明确报错。非标准主文档位置、DTD、内联二进制 XML 等结构会失败，不会放宽读取策略。
- 成功与失败缓存均区分模式；设置保存会重新检查索引，只有相关 DOCX/PPTX 的转换身份改变，PDF 等格式的身份不改变。原文件保持不变。
- 保留输入、单 XML、累计解压、节点/深度、输出、超时与取消限制。当前默认输入 512 MiB，单 XML/累计文字解压受默认 max_decoded_text_mib=32 MiB 限制；不是通过提高资源上限实现。AnyDoc 工作线程超时后丢弃结果，沿用现有线程隔离方式，不代表强制终止线程。
- 源文件指纹仍分块读取原压缩包；不声称系统物理 I/O 完全不经过图片压缩字节，也不声称进程峰值内存等于文字大小。

## 功能验证

- `core/tests/office_text_only.rs`：6 项通过，覆盖两格式大图片超限→开启成功、损坏压缩图片/OLE/伪装 XML 图片仍可提取正文、默认关闭和格式范围、取消/限额不覆盖旧输出、已有真实小型格式夹具文字逐行保留、设置重启持久化、失败回退阻断/关闭后重试、普通/分阶段两条索引路径在启用云设置后仍本地检索成功。
- 损坏资源测试先直接读取条目并断言解压失败，再断言受限解析成功，防止使用无效的测试诱饵。AnyDoc 既有 DOCX/PPTX 格式夹具分别包含脚注/尾注和 PPT 备注关系。
- `anydoc::office_text::tests`：2 项通过，覆盖路径/MIME 过滤及保留文本框/表格同时去掉图片/OLE 引用、拒绝 DTD/内联二进制。
- 既有 `progressive_index` 34 项、`conversion_failures` 1 项通过。
- 前端 file-types/runtime-settings/progressive-index：19 项通过；Svelte 检查 0 错误、0 警告。CLI `cargo check --locked --offline` 通过；已有 Rust 未使用代码警告保留。
- 双语 README、folders 文档与中英文默认关闭截图已同步。文档站 `npm --prefix docs-site run build` 通过；本地生产预览检查中英文页面正文、Office 搜索标题及图片链接返回成功，截图已人工查看。

## 测量方法

用户当前没有可提供的真实问题文件，本节全部为**合成样本**，不代表技术报告的实际速度或兼容性。

- Apple M4 Pro，24 GiB 内存，macOS 26.2（25C56）；Release 构建，同一个最终二进制，分别关闭/开启新开关。关闭时走未改变的既有 AnyDoc 路径，因此这是原路径与新模式对比，并非两个历史安装包对比。
- 每种格式生成两张真实 BMP 结构的纯色图片，解压后总量约 48 MiB 或 140 MiB；正文只有固定检索词。BMP 像素高度可压缩，压缩文件远小于解压资源，专门复现输入未超限但内部资源超限。两幅图内容相同但关系来源不同，原引擎仍分别计入资源预算。
- 每组交替顺序进行 3 轮，使用全新目录/SQLite，关闭监听与云解析，不安装 Docling。记录 `Store.scan` 墙钟时间以及独立进程 `/usr/bin/time -l` 的峰值 RSS（字节）。计时包含文件哈希、转换及索引，进程 RSS 还包含 Store 初始化；不包含生成样本或 GUI，未清空 OS 文件缓存。
- 测量期间不运行本任务构建/测试；正常 OS 负载不受控。如此短的正文，几十毫秒级时差主要受固定开销和系统波动影响，不应外推为真实文件的提速比例。
- 48 MiB 组前后均成功，可比较时间和内存；140 MiB 组关闭时触发 max_asset_total_bytes，开启后成功，只比较可用性与峰值，**失败耗时不是等价完成耗时**。
- 复现：`cargo build --manifest-path core/Cargo.toml --locked --offline --release --example office_text_baseline`，随后 `python3 scripts/measure-office-text.py core/target/release/examples/office_text_baseline qa/office-text-perf-<unique-run>`。使用全新输出目录。原始输入、逐轮 time 日志、JSON 和元信息保存在忽略的 qa 目录，不提交本地产物。

## 最终测量结果

最终 Release 二进制 SHA-256：`9ee802f6a7e216dc8ab99f140563f13176d9ba862a9193e9b0c5ba8d1cb39d9e`。原始记录：`qa/office-text-perf-final/`。以下均为三轮中位数，内存单位 MiB（2²⁰ 字节）。

| 合成样本 | 原路径耗时 | 文字模式耗时 | 原路径峰值 RSS | 文字模式峰值 RSS | 结果 |
| --- | ---: | ---: | ---: | ---: | --- |
| docx-48MiB.docx | 61.41 ms | 51.71 ms | 137.66 | 51.67 | 均成功，检索词命中 |
| pptx-48MiB.pptx | 60.30 ms | 46.43 ms | 137.67 | 51.81 | 均成功，检索词命中 |
| docx-140MiB.docx | 67.37 ms | 45.75 ms | 481.86 | 51.70 | 原路径资源超限失败；开启后检索命中 |
| pptx-140MiB.pptx | 66.06 ms | 48.49 ms | 481.36 | 51.48 | 原路径资源超限失败；开启后检索命中 |

正常成功组峰值 RSS 下降约 62%；耗时中位数分别缩短约 9.70 ms / 13.87 ms。由于正文极少且未控制 OS 负载，只报告该样本观测值，不把毫秒差换算成产品级提速承诺。140 MiB 组未提高资产上限，原路径三轮均失败，文字模式三轮均成功；不把失败耗时与成功耗时作加速比。

### 输入身份

| 样本 | 压缩文件字节数 | SHA-256 |
| --- | ---: | --- |
| docx-140MiB.docx | 642057 | `4f7b712436a6e861a9a1f55951caf39c90cf031c680ebd3076d03af53b9ad63c` |
| docx-48MiB.docx | 221081 | `324893ac55ba08ad39b530ee0aa7c79818afa5eb0f6e867decbc7144acee1ede` |
| pptx-140MiB.pptx | 642620 | `46d8d2c6d4fd858bf666e98cb0182adfeac0612ef50f621ef1a25459e9ac6284` |
| pptx-48MiB.pptx | 221644 | `dd1feeea479f56579b23370c4085019263fe9e363bdcf0b205c21f90e9b506d1` |

### 逐轮记录

| 样本 | 轮次 | 模式 | scan 毫秒 | 峰值 RSS MiB | 检索命中数 |
| --- | ---: | --- | ---: | ---: | ---: |
| docx-48MiB.docx | 1 | off | 71.534 | 136.203 | 1 |
| docx-48MiB.docx | 1 | on | 48.679 | 51.500 | 1 |
| docx-48MiB.docx | 2 | on | 51.706 | 51.672 | 1 |
| docx-48MiB.docx | 2 | off | 58.861 | 137.656 | 1 |
| docx-48MiB.docx | 3 | off | 61.411 | 137.812 | 1 |
| docx-48MiB.docx | 3 | on | 61.061 | 51.859 | 1 |
| docx-140MiB.docx | 1 | off | 136.761 | 481.859 | 0 |
| docx-140MiB.docx | 1 | on | 45.749 | 51.875 | 1 |
| docx-140MiB.docx | 2 | on | 45.381 | 51.703 | 1 |
| docx-140MiB.docx | 2 | off | 67.370 | 481.984 | 0 |
| docx-140MiB.docx | 3 | off | 66.165 | 481.203 | 0 |
| docx-140MiB.docx | 3 | on | 46.734 | 51.484 | 1 |
| pptx-48MiB.pptx | 1 | off | 63.193 | 137.672 | 1 |
| pptx-48MiB.pptx | 1 | on | 46.425 | 51.656 | 1 |
| pptx-48MiB.pptx | 2 | on | 45.370 | 51.859 | 1 |
| pptx-48MiB.pptx | 2 | off | 60.300 | 137.859 | 1 |
| pptx-48MiB.pptx | 3 | off | 58.002 | 137.625 | 1 |
| pptx-48MiB.pptx | 3 | on | 46.985 | 51.812 | 1 |
| pptx-140MiB.pptx | 1 | off | 64.801 | 481.781 | 0 |
| pptx-140MiB.pptx | 1 | on | 48.489 | 51.734 | 1 |
| pptx-140MiB.pptx | 2 | on | 49.206 | 51.469 | 1 |
| pptx-140MiB.pptx | 2 | off | 66.061 | 481.359 | 0 |
| pptx-140MiB.pptx | 3 | off | 71.129 | 479.156 | 0 |
| pptx-140MiB.pptx | 3 | on | 46.890 | 51.484 | 1 |

## 后续真实文件验收

目前没有用户问题文件，不能宣称所有技术报告的兼容性和降幅已验证。用户找到资料后，重点补测 Word 文本框/表格/公式、PPT 分组/备注/图表，以及曾触发资源上限的真实文件；核对关键文字可检索、内容顺序和来源定位。图片中的文字与嵌入附件内部内容属于明确跳过范围，不能当作识别结果缺失的回归。
