# DeepSeek Flash：同批 MuSiQue 23 题对照（2026-10-15）

同一批 23 道问题，当前客户端流程下，Qwen3.6-flash 严格答对 **9/23**，deepseek-flash 严格答对 **11/23**。相对上轮新增命中 3 题、丢失 1 题。

## 实验设置

- 使用实际客户端后端的 SAG2 工具、跨段阅读、回答和引用检查流程。代码没有修改；只在独立评测进程替换答案模型。
- 同样的 23 题、11,656 个原始段落、候选 50、扩展 1 轮、每题工具上限 3、上下文 65,536。复用既有 Jev 评分缓存。
- 两组提示词和计分方式相同；模型的后续检索查询可不同，这是完整工具流程对照。
- 23 题首次检索中 23/23 排序与上轮完全相同。
- 生成参数沿用当前客户端：Qwen 为 temperature=0、enable_thinking=false；DeepSeek 未额外覆盖 temperature 或推理设置。因此结果反映当前客户端下的模型替换效果，不能将差异全归因于模型本身。
- deepseek-flash 接口连接与原生工具调用已通过；未修改客户端默认模型或原文件。

## 结果

| 指标 | Qwen3.6-flash | deepseek-flash |
|---|---:|---:|
| 严格答对 | 9/23 | 11/23 |
| F1 | 42.27% | 57.51% |
| 平均每题耗时 | 25.0 秒 | 80.6 秒 |
| 工具调用总数 | 37 | 59 |
| 未调用检索 | 0 | 0 |
| 无效引用编号 | 0 | 0 |
| 请求失败 | 1 | 1 |
| 召回全部标注段的题数 | 14 | 17 |

耗时来自本次真实调用，受模型推理、缓存、服务状态和检索次数影响。标注段落齐全不等于原文事实链齐全；引用编号合法不等于语义支持正确。

## 结果解读

- 新增严格命中的是第 13、14、16 题，均为 Mingus 相关问题。第 15 题从 Medieval Latin 退为 Latin，导致丢失 1 题，净增加 2 题。
- 第 11 题没有再被服务拒绝；DeepSeek 找齐 Mizraab、Partition of India、Hindus 三段，并回答 India / Al-Hind。第 12 题找齐国会、Senate 选举、Cabinet 三段，给出 January 3, 2015。两题都因回答长于要求的最短答案而未严格命中。
- 第 21 题在 124.4 秒时请求超时，已保留失败记录，没有重跑后挑选结果。其余 22 题完成。
- Godiva、Saudi Arabia、Sati、配偶等问题仍有空答；更换模型没有消除此前确认的资料缺失、题面歧义和全部推理问题。
- 这批题中答案匹配有所改善，平均耗时从 25.0 秒增加到 80.6 秒，约为 3.2 倍。客户端默认模型仍是 qwen3.6-flash。

## 逐题结果

| # | 问题 | 标准答案 | Qwen3.6-flash | deepseek-flash | 严格命中 |
|---|---|---|---|---|---|
| 1 | Tripartite | mid-June | June | June | — |
| 2 | Godiva | 918 | 空答 | 空答 | — |
| 3 | Saudi Arabia | 1930 | 空答 | 空答 | — |
| 4 | A Lim | The dynasty regrouped and defeated the Portuguese | The provided documents do not contain information about the Somali Muslim Ajuran Empire or Somalis being expelled from Myanmar (the country between Thailand and Laos). So | They were defeated by the Burmese dynasty in 1613. | — |
| 5 | Empire Sports | Time Warner Cable | Time Warner Cable | Time Warner Cable | ✓ |
| 6 | Palau | built in the 15th century | 15th century | the 15th century | ✓ |
| 7 | Sati | a long period of prosperity for the British people | 空答 | 空答 | — |
| 8 | 1999 draft | Josh Beckett | Josh Beckett | Josh Beckett | ✓ |
| 9 | East Timor | Francisco Guterres | Francisco Guterres | Francisco Guterres | ✓ |
| 10 | Auctor | Medieval Latin | Medieval Latin | Medieval Latin | ✓ |
| 11 | Arabic Hindu | the country of India | 服务拒绝 | it referred to the country of India (Al-Hind) | — |
| 12 | Senate / Cabinet | January 2015 | The provided passages do not identify which body approves members of the Cabinet, so it is impossible to determine when the majority party in the House gained control of  | The Republican majority party gained control of the Senate—the body that confirms Cabinet members—in the 2014 elections, with control beginning in the 114th Congress on J | — |
| 13 | Mingus Three | Mario Andretti | 空答 | Mario Andretti | ✓ |
| 14 | Mingus Piano / capital | Mario Andretti | Will Power | Mario Andretti | ✓ |
| 15 | Sylvester | Medieval Latin | Medieval Latin | Latin | — |
| 16 | Mingus Piano / largest | Mario Andretti | Will Power | Mario Andretti | ✓ |
| 17 | A Don | The dynasty regrouped and defeated the Portuguese | The Somali Muslim Ajuran Empire used new coinage to proclaim independence from the Portuguese. The natural boundary between Thailand and Myanmar (Burma) is partly formed  | 空答 | — |
| 18 | Empress Wang | Yang Xingmi | Yang Xingmi | Yang Xingmi | ✓ |
| 19 | Kosovo Olympics | 2016 | 2016 Summer Olympics | 2016 Summer Olympics | — |
| 20 | Shrek spouse | Adrian Edmondson | 空答 | 空答 | — |
| 21 | Senate / rules | January 2015 | 2014 | 请求超时 | — |
| 22 | Cosby library | Brooklyn Public Library | Brooklyn Public Library | Brooklyn Public Library | ✓ |
| 23 | Venice plague | 22 | 22 | 22 | ✓ |

## 原文保护与检查

核对 388 个唯一引用片段的内容、UTF-8 字节范围和行列位置，差异 0 处。
核对 691 个用户文件、11656 个语料文件、2 个原始数据文件及旧评测结果。校验结果见下列记录。

## 记录

- [逐题完整结果](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-flash-20261015/comparison.json)
- [逐题 CSV](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-flash-20261015/逐题对比.csv)
- [原始评测汇总](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-flash-20261015/summary.json)
- [原文和配置校验](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-flash-20261015/verification.json)

这 23 题是此前筛出的失败题，结果不能代表整个 MuSiQue 测试集。此轮没有改写参考答案，也没有将人工分解或标准答案放入模型上下文。
