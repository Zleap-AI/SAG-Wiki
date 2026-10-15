# MuSiQue 100 题：DeepSeek Flash 客户端与原版 SAG2 对照

同一批 100 题中，客户端多次调用的合并召回率为 **94.83%**，原版前 10 段召回率为 **86.67%**。严格答对分别为 **48/100** 和 **51/100**。失败题保留在分母中。

## 实验设置

- 使用此前同样顺序的前 100 道 MuSiQue 可回答验证题和全部 11,656 个语料段落。两组答案模型均为 deepseek-flash。这是 100 题子集，不能代表完整 MuSiQue。
- 客户端使用当前生产代码：每次 50 候选、扩展 1 次、返回前 10 段，每题最多 3 次模型工具调用，输入上下文预算 65,536。未设置输出 token 上限。
- 原版直接调用未修改的 SAG2 实现：100 候选、扩展 2 次、返回前 10 段；沿用原版 QA 提示词、前 5 段证据、temperature=0。客户端 DeepSeek 使用服务默认采样参数。
- 两组都完整运行了 100 题。客户端复用可命中的历史 Jev 评分缓存；原版此次重新请求 Jev。所有 DeepSeek 答案均为本轮生成，没有复用之前 23 题的答案。
- 这是两种完整流程的比较；提示词、采样设置、候选数、扩展次数、证据数量与分段方式存在差异，不能将结果差异单独归因于其中一个因素。
- 检索与回答请求不包含标准答案、人工分解步骤或支持段标签。网络失败计为答错，保留具体错误，没有挑选成功重跑的答案。

## 指标

召回率按每题找回的标准支持段数 / 标准支持段总数计算，再对 100 题取平均。合并召回率包含模型所有成功 SAG2 调用返回的证据，排除评测程序额外执行的固定问题检索。答案采用标准别名的归一化精确匹配（EM）和词级 F1。

| 指标 | 原版 SAG2 + DeepSeek | 客户端 + DeepSeek |
|---|---:|---:|
| 单次 Recall@5 | 80.92% | 76.83% |
| 单次 Recall@10 | 86.67% | 83.00% |
| 全部返回证据的召回率 | 86.67%（一次，前 10 段） | 94.83%（最多三次，合并） |
| 实际回答上下文召回率 | 前 5 段，见 Recall@5 | 94.83% |
| 严格答对 | 51/100 | 48/100 |
| 答案 F1 | 65.01% | 56.20% |
| 全部支持段找齐 | 66/100（前 10 段） | 85/100（多次调用合并） |
| 请求失败题数 | 0 | 3 |

客户端共发起 219 次工具调用，成功返回检索结果 219 次。相对原版新增答对 6 题，丢失 9 题；两组共同答对 42 题，共同答错 43 题。

客户端有 40 题已取得全部标注支持段，却未通过严格答案评分，其中也包括回答阶段失败的题目。标注段齐全不代表答案一定正确，需要继续区分回答错误、格式差异和数据标注问题。

## 失败记录

- 客户端 `2hop__77233_33207`：模型超过本次工具调用次数上限
- 客户端 `3hop1__144678_443779_52195`：接收模型回答超时
- 客户端 `4hop1__802394_153080_159767_81096`：接收模型回答超时

## 逐题结果

| # | 题目 | 标准答案 | 原版答案 | 客户端答案 | 原版 R@10 | 客户端合并召回 |
|---|---|---|---|---|---:|---:|
| 1 | When was the person who Messi's goals in Copa del Rey compared to get signed by Barcelona? | June 1982 | June 1982 | June 1982 | 100% | 100% |
| 2 | What month did the Tripartite discussions begin between Britain, France, and the country where, despite being headquartered in the nation called the nobilities commonwealth, the top-ranking Warsaw Pact operatives originated? | mid-June | June | June | 100% | 100% |
| 3 | What county is Erik Hort's birthplace a part of? | Rockland County | Rockland County | Rockland County, New York | 100% | 100% |
| 4 | What year did the publisher of Labyrinth end? | 1986 | 1986 | 1986 | 100% | 100% |
| 5 | When was Lady Godiva's birthplace abolished? | 918 | 918 | 空答 / 请求失败 | 100% | 100% |
| 6 | When was the region immediately north of the region where Israel is located and the location of the Battle of Qurah and Umm al Maradim created? | 1930 | 22 September 1932 | 空答 / 请求失败 | 100% | 50% |
| 7 | Where does the body of water by the city where the Southeast Library designer died empty into the Gulf of Mexico? | the Mississippi River Delta | Mississippi River Delta | 空答 / 请求失败 | 75% | 75% |
| 8 | When did the explorer reach the city where the headquarters of the only group larger than Vilaiyaadu Mankatha's record label is located? | August 3, 1769 | 1542 | August 3, 1769 | 50% | 100% |
| 9 | When was the start of the battle of the birthplace of the performer of III? | December 14, 1814 | December 14, 1814 | December 14, 1814 | 67% | 100% |
| 10 | What is the Till dom ensamma performer's birth date? | 11 September 1962 | 11 September 1962 | 11 September 1962 | 100% | 100% |
| 11 | How many episodes are in season 5 of the series with The Bag or the Bat? | 12 | 12 | 12 | 100% | 100% |
| 12 | How were the people from whom new coins were a proclamation of independence by the Somali Muslim Ajuran Empire expelled from the country between Thailand and A Lim's country? | The dynasty regrouped and defeated the Portuguese | They were defeated and expelled by King Anaukpetlun’s Burmese forces after the capture of Syriam in 1613. | The Portuguese were defeated by the dynasty in 1613. | 100% | 100% |
| 13 | What is the birthplace of the person after whom São José dos Campos was named? | Nazareth | Bethlehem | 空答 / 请求失败 | 100% | 100% |
| 14 | How many times did plague occur in the place where Crucifixion's creator died? | 22 | 22 times | 22 times | 100% | 100% |
| 15 | What year did the Governor of the city where the basilica named after the same saint as the one that Mantua Cathedral is dedicated to die? | 1952 | 1952 | 1952 | 67% | 100% |
| 16 | What company succeeded the owner of Empire Sports Network? | Time Warner Cable | Time Warner Cable | 空答 / 请求失败 | 100% | 100% |
| 17 | When was the Palau de la Generalitat constructed in the city where Martin from the region where Perdiguera is located died? | built in the 15th century | The Palau de la Generalitat in Barcelona was built in the 15th century. | 15th century | 100% | 100% |
| 18 | Who married the actor from Terminator? | Maria Shriver | Maria Shriver | Maria Shriver | 100% | 100% |
| 19 | Who was in charge of the place where Castricum is located? | Johan Remkes | Johan Remkes | Johan Remkes | 100% | 100% |
| 20 | Who is the mascot of the university related to Randy Conrads? | Benny Beaver | Benny Beaver | Benny Beaver | 100% | 100% |
| 21 | What does seal stand for in the U.S. military branch that operates destroyers, as well as the USS Edsall? | Sea, Air, and Land | Sea, Air, and Land | Sea, Air, and Land | 67% | 67% |
| 22 | Who is the child of Chiang Hsiao-wen's father? | Chiang Hsiao-wu | Chiang Hsiao-chang | Chiang Hsiao-wen, Chiang Hsiao-wu, Chiang Hsiao-chang, Winston Chang, and John Chiang | 100% | 100% |
| 23 | Where was the person who wrote about the rioting being a dividing factor in Birmingham educated? | University of Glasgow | Greenock Grammar School | 空答 / 请求失败 | 50% | 100% |
| 24 | Whose sister played Susie in miracle on 34th street? | Lana Wood | Lana Wood’s sister, Natalie Wood. | 空答 / 请求失败 | 100% | 50% |
| 25 | Who is the child of the Italian navigator who explored the eastern coast of the continent César Gaytan was born in for the English? | Sebastian Cabot | Sebastian Cabot | Sebastian Cabot | 75% | 75% |
| 26 | Who is the spouse of Young Man Luther's author? | Joan Erikson | Joan Erikson | 空答 / 请求失败 | 100% | 100% |
| 27 | How many times did the plague occur in the city where the painter of The Bacchanal of the Andrians died? | 22 | 22 times | 空答 / 请求失败 | 100% | 100% |
| 28 | In what year was the group who performed Attics To Eden formed? | 2005 | 2005 | 2005 | 100% | 100% |
| 29 | Where did the arguer that the country Directive 10/2 called for actions against had become an imperialist power declare he would intervene in the Korean conflict? | the Politburo | to the Politburo | to the Politburo | 100% | 100% |
| 30 | When did military instruction start at the place where Larry Alcala was educated? | 1912 | 1912 | 1912 | 100% | 100% |
| 31 | When did the city where Greenwood Laboratory School is located become capitol of the state where the screenwriter of The Poor Boob was born? | 1839 | 1839 | 1839 | 100% | 100% |
| 32 | Who is played by the director of The Good Shepherd in The Godfather? | Vito Corleone | Vito Corleone (young Vito Corleone in *The Godfather Part II*). | Vito Corleone (the young Vito Corleone, portrayed by Robert De Niro in The Godfather Part II) | 100% | 100% |
| 33 | How long are the council terms of the city that shares a county with Helvetia? | four-year | Four-year terms. | Four years | 100% | 100% |
| 34 | When was the region immediately north of the region that prevailed with the disgrace of Near East and the terrain feature on which shamal is located created? | 1930 | 100,000 to 10,000 years ago | 1932 | 50% | 100% |
| 35 | How did did the people fare during the reign of the abolisher of sati partha in India? | a long period of prosperity for the British people | Under Queen Victoria, after the Indian Rebellion of 1857, the East India Company was dissolved and India came under direct British Crown rule. Victoria condemned atrocities on both sides and insisted that the transfer of power be proclaimed with “generosity, benevolence and religious toleration.” At her request, a passage threatening native religions and customs was replaced by one guaranteeing religious freedom. | 空答 / 请求失败 | 50% | 50% |
| 36 | What is the position of the 1st governor general of India? | Governor-General of India | Warren Hastings was the first Governor-General of India, also serving as Governor of the Presidency of Fort William (Bengal) and head of the Supreme Council of Bengal. | Warren Hastings | 100% | 100% |
| 37 | Who is the child of the president who suffered a major defeat when the body which approves members of the American cabinet refused to ratify the Versailles treaty? | Jessie Woodrow Wilson | Jessie Woodrow Wilson Sayre | Jessie Woodrow Wilson Sayre | 100% | 100% |
| 38 | What does dell call the feature letting the interface replacing FireWire in later iterations of the iPod drives to remain powered when the computer is off? | PowerShare | PowerShare | PowerShare | 100% | 100% |
| 39 | Who wrote Turn Me On by the singer of Come Away with Me? | John D. Loudermilk | John D. Loudermilk | John D. Loudermilk | 100% | 100% |
| 40 | Who was second pick in the 1999 draft of the league that has a competition where they give out the MLB MVP award after it? | Josh Beckett | Josh Beckett | Josh Beckett | 75% | 75% |
| 41 | The Commission of Truth and Friendship established governments for Bustami's birth country and a second country that had who as president after declaring independence? | Francisco Guterres | José Ramos-Horta | 空答 / 请求失败 | 100% | 100% |
| 42 | Where is the country with ISO code ISO 3166-2:CV located? | central Atlantic Ocean | Cabo Verde (Cape Verde) is located in the central Atlantic Ocean, about 570 kilometres (350 mi) west of the Cape Verde Peninsula off the coast of Northwest Africa. | In the central Atlantic Ocean, 570 kilometres (350 mi) west of the Cape Verde Peninsula off the coast of Northwest Africa | 100% | 100% |
| 43 | What was the form of the language Auctor is in, used in the era of the Frankish king who created the Holy Roman Empire, later known as? | Medieval Latin | Medieval Latin | Medieval Latin | 100% | 100% |
| 44 | When did the political party that favored harsh punishment of southern states after the Civil War, gain control of the House? | the 2010 election | In the 2010 United States House of Representatives elections. | 空答 / 请求失败 | 50% | 100% |
| 45 | When did the person chosen to be president of the confederacy end his fight in the Mexican-American war? | 1848 | 1848 | 1848 | 100% | 100% |
| 46 | What city is the star of Sous les pieds des femmes from? | La Goulette | Tunis | Tunis | 100% | 100% |
| 47 | what is meaning of the word that is a majority religion of the area that became India when the country origin of Mizraab was created in Arabic dictionary? | the country of India | The word is “Hindu,” derived from the Sanskrit “Sindhu,” meaning “a large body of water” (covering “river, ocean”). It was originally an ethno-geographical term, not a religious one. The Arabic equivalent “Al-Hind” referred to the country of India. | a large body of water, covering "river, ocean" (the word is "Hindu", derived from Sanskrit Sindhu) | 67% | 100% |
| 48 | Who stars in the video "One Last Time" by the performer of Baby I? | Matt Bennett | Matt Bennett | Matt Bennett | 100% | 100% |
| 49 | When did the majority party in the House of Representatives gain control of the body which approves members of the Cabinet? | January 2015 | The 2014 elections | The 2014 elections, effective when the 114th Congress began on January 3, 2015 | 67% | 100% |
| 50 | How many ethnic minorities were looked at differently in the city where the headquarters of the only group larger than Långa nätter's record label is located? | two | 2 | two | 100% | 100% |
| 51 | When did the country the top-ranking Warsaw Pact operatives came from, despite it being headquartered in the country where A Generation is set, agree to a unified Germany inside NATO? | May 1990 | May 1990. | May 1990 | 100% | 100% |
| 52 | Who won the Indy Car Race in the largest populated city of the state where the performer of Mingus Three is from? | Mario Andretti | Mario Andretti | Mario Andretti | 50% | 100% |
| 53 | Who did the person who married their half sister in the bible marry after the death of sarah? | Keturah | Keturah | Keturah | 100% | 100% |
| 54 | What is the average salary of a working person with the same nationality as the producer of The Wild Women of Chastity Gulch? | $59,039 | More than $100,000 annually (the passage specifies this for some construction workers in the US/Canada; it does not provide a general average). | $72,641 (the mean household income in the United States, i.e. the average income earned by American households) | 33% | 100% |
| 55 | Who was Beyonce's husband talking about in the the song Cry? | three different relationships he had in the past | Jay-Z said “Song Cry” was inspired by three different relationships he had in the past, combining his experiences in different verses. | Three different relationships he had in the past | 50% | 100% |
| 56 | When was the region immediately north of the region where Israel is located and the site of the most growth in desalination for agricultural use established? | 1932 | 1824 | 空答 / 请求失败 | 50% | 75% |
| 57 | An Indy car race was held in the capital of the state where the performer of Mingus Plays Piano was born. Who won the race? | Mario Andretti | Scott Dixon | 空答 / 请求失败 | 75% | 75% |
| 58 | What is the name of the famous bridge in the birth city of the composer of Scanderbeg? | Rialto Bridge | Rialto Bridge | Rialto Bridge | 100% | 100% |
| 59 | Who was the spouse of a leading speaker against slavery and publisher of an antislavery newspaper? | Helen Pitts Douglass | Helen Pitts Douglass | Helen Pitts Douglass | 100% | 100% |
| 60 | Where is the district that the person who wanted to reform and address John Kodwo Amissah's religion preached a sermon on Marian devotion before his death located? | Saxony-Anhalt | Wittenberg is a district in the east of Saxony-Anhalt, Germany. | in the east of Saxony-Anhalt, Germany | 100% | 100% |
| 61 | What is the capital of the county where Fort Deposit is located? | Hayneville | Hayneville | Hayneville | 100% | 100% |
| 62 | How long had Pfrang Association's headquarters location been the capitol city of the area where Guangling District is located? | about 400 years | Nanjing, Pfrang Association’s headquarters, has been the capital of Jiangsu Province—the province containing Guangling District—since 1949, when the People’s Republic of China was established. That is about 77 years as of 2026. | About 400 years, from late Han to early Tang | 100% | 100% |
| 63 | What was the language Auctor comes from during the era of the man crowned first Holy Roman Emperor later known as? | Medieval Latin | Medieval Latin | Medieval Latin | 100% | 100% |
| 64 | When did the 1979-80 European Cup winner win the FA Cup? | 1898 and 1959 | Nottingham Forest won the FA Cup in 1898 and 1959. | 1898 and 1959 | 100% | 100% |
| 65 | Who was a prominent figure at the radio division of the network that created the version of The Biggest Loser set in the country where Seria is? | Walter Sabo | Ralph Edwards | Joel McCrea | 67% | 100% |
| 66 | What was the 2018 population of the Italian city that's underwater? | 260,897 | 260,897 | 260,897 | 100% | 100% |
| 67 | What was the language from which the last name Sylvester originated during the era of the person crowned emperor of the west in 800 CE later known as? | Medieval Latin | Medieval Latin | Medieval Latin | 100% | 100% |
| 68 | Who is the head of the agency that created MYSTIC? | ADM Michael S. Rogers | Michael S. Rogers (Director of the National Security Agency at the time). | Keith B. Alexander | 50% | 100% |
| 69 | What was the wettest year of the administrative territorial entity that contains the administrative territorial entity where Drexel Heights is located? | 1905 | 1905 | 1905 | 100% | 100% |
| 70 | Who won the Indy Car Race in the largest populated city of the state where the performer of Mingus Plays Piano is from? | Mario Andretti | Mario Andretti | Mario Andretti | 50% | 100% |
| 71 | What does the river that Darling Mills Creek turns into flow into? | Sydney Harbour | Sydney Harbour (Port Jackson) | Sydney Harbour, a branch of Port Jackson | 100% | 100% |
| 72 | What administrative territorial entity does Darley Ramon Torres's place of birth located? | Minas Gerais | Minas Gerais, Brazil | State of Minas Gerais | 100% | 100% |
| 73 | Which county shares a border with the county where the most populous city in the state where Washington State Prison can be found is located? | Presque Isle County | Cobb County | 空答 / 请求失败 | 50% | 50% |
| 74 | What is the meaning of the name of the city where the Yongle emperor greeted the person to whom the edict was addressed? | "Southern Capital" | Southern Capital | Southern Capital | 67% | 100% |
| 75 | What is the direction of flow of the body of water by the city where Write This Down was formed? | rises in northern Minnesota and meanders slowly southwards | Southward | southwards | 100% | 100% |
| 76 | How were people from whom new coins were a proclamation of independence by the Somali Muslim Ajuran Empire expelled from the natural boundary between Thailand and A Don's country? | The dynasty regrouped and defeated the Portuguese | The Rohingya were forcibly expelled by the Burmese regime, which refused to recognize them as citizens and tried to replace them with non-Rohingyas; this resulted in the expulsion of about half of the 800,000 Rohingya from Burma/Myanmar. | 空答 / 请求失败 | 75% | 75% |
| 77 | Who is the father of Empress Wang's husband? | Yang Xingmi | Yang Xingmi | Yang Xingmi | 100% | 100% |
| 78 | When did the country with a co-official language used in the film titled after the city that is the final resting place of Tito first attend the Olympics games? | 2016 | 2016 Summer Olympics | 2016 Summer Olympics | 100% | 100% |
| 79 | Who is the spouse of the actress who played the fairy god mother in Shrek 2? | Adrian Edmondson | Adrian Edmondson | 空答 / 请求失败 | 100% | 100% |
| 80 | When was the last time Darren Carter's team beat the 1894-95 FA Cup winner? | 1 December 2010 | 1 December 2010 | 空答 / 请求失败 | 100% | 100% |
| 81 | What is the name of the famous bridge in the city where the composer of La Silvia was born? | Rialto Bridge | Rialto Bridge | Rialto Bridge | 67% | 100% |
| 82 | Who is the spouse of the actor that plays Jarvis in the Avengers Age of Ultron? | Jennifer Connelly | Jennifer Connelly | 空答 / 请求失败 | 50% | 100% |
| 83 | What is the location of formation of the film company distributing The Boss? | Fort Lee | Fort Lee, New Jersey | Fort Lee, New Jersey | 100% | 100% |
| 84 | How long are the city council terms in the second largest city in the state where Yuma is located? | four-year | Four-year terms. | Four years | 50% | 100% |
| 85 | Who is the child of the president under whom prohibition occurred? | Jessie Woodrow Wilson | Jessie Woodrow Wilson Sayre | Jessie Woodrow Wilson Sayre | 100% | 100% |
| 86 | What administrative territorial entity contains the place where KPRM is licensed to broadcast to? | Hubbard County | Hubbard County, Minnesota | Minnesota | 100% | 100% |
| 87 | Who played the girlfriend of who plays marty mcfly's daughter in back to the future 2? | Claudia Wells | Elisabeth Shue | Elisabeth Shue | 100% | 100% |
| 88 | When was the death penalty abolished in the country near the country where the writer of The Book Thief is a citizen of? | 1989 | 1989 | 空答 / 请求失败 | 50% | 75% |
| 89 | What is the tallest building in the state where KZAR is located? | JPMorgan Chase Tower | JPMorgan Chase Tower (formerly Texas Commerce Tower). | The JPMorgan Chase Tower | 100% | 100% |
| 90 | What is the maximum load drawn by the 1.0 and 2.0 versions of the device that replaced FireWire in later iPods? | five unit loads | 500 mA | 500 mA | 100% | 100% |
| 91 | Where is the headquarters of Radio Television of this country that has a co-official language that is the same as the language of Olivera Markovic? | Pristina | Pristina, Kosovo. | 空答 / 请求失败 | 67% | 67% |
| 92 | What county has a city that is adjacent to the capital city of Erskine College's state? | Richland County | Richland County | Richland County, South Carolina | 75% | 100% |
| 93 | When did the majority party in the House of Representatives gain control of the body which determines rules of the US House and US Senate? | January 2015 | January 3, 2015. | The 2014 elections; the 114th Congress began January 3, 2015 | 67% | 100% |
| 94 | When did Nissan, the Acura Legend maker and the Scion Fuse manufacturer open US assembly plants? | 1981 | In the early 1980s, about a decade after the 1973 oil crisis. | By the early 1980s (a decade after the 1973 oil crisis, following the 1981 voluntary export restraints) | 67% | 100% |
| 95 | Who had the lowest batting average in the league where the team with the most games in the series after which the MLB MVP is awarded played? | Bill Bergen | The provided passages do not contain batting average statistics, so the lowest batting average in the relevant league cannot be determined from them. | Harland Rowe | 75% | 75% |
| 96 | What is the public library system called in the place where the Cosbys live in New York? | Brooklyn Public Library | Brooklyn Public Library | The Brooklyn Public Library | 100% | 100% |
| 97 | What is an example of a railroad line in the country first to invade Manchuria? | Yamanote Line loop | The Yamanote Line | Yamanote Line | 100% | 100% |
| 98 | Who is the child of the Victim of Romance performer? | Chynna Phillips | Chynna Phillips | Chynna Phillips | 100% | 100% |
| 99 | How many times did the plague occur in the birth place of Concerto in C Major Op 3 6's composer? | 22 | 22 times | 22 times | 100% | 100% |
| 100 | When was the region immediately north of the region where the country in which Aluf can be found is located and the Persian Gulf established? | 1932 | 1922 | 1922 | 75% | 50% |

## 检查记录

校验 2722 个原文件、代码和已有评测文件，11,656 个语料文件，以及 1365 个唯一引用片段。文件变化 0 处、语料变化 0 处、引用内容或位置错误 0 处。客户端设置保持不变。

检索回放共 319 次，返回结果和排序全部相同；完整过程记录 318 次相同。另 1 次为空查询结果：原版记录了一条数量为 0 的 BM25 步骤，客户端没有记录这条空步骤。已单独复核，两者都未调用评分模型、也未返回证据，召回率不受影响。

- [汇总数据](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-100-20261015/comparison-summary.json)
- [逐题完整数据](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-100-20261015/comparison-details.json)
- [逐题 CSV](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-100-20261015/逐题对照.csv)
- [文件保护与引用检查](/Users/jomymac/Desktop/SAG/SAG-Desktop/.cache/musique-deepseek-100-20261015/verification.json)
