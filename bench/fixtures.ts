// Deterministic Chinese-first Markdown that resembles converted PDF books:
// full-width punctuation, nested headings, tables, code, lists, images,
// reference links, and an occasional mermaid diagram.
function random(seed: number) {
  return () => {
    seed = (seed * 1664525 + 1013904223) >>> 0;
    return seed / 2 ** 32;
  };
}

const words = ['模型', '推理', '上下文', '缓存', '注意力', '前缀', '计算', '向量', '检索', '索引', '事件', '实体', '智能体', '工具调用', '提示词', '延迟', '吞吐', '显存', '批处理', '调度'];
const latin = ['token', 'Transformer', 'GPU', 'API', 'Agent', 'RAG', 'prompt', 'LLM'];

export function fixture(targetBytes: number, keyword = 'KV Cache', hitsPerMegabyte = 220) {
  const next = random(42);
  const pick = <T>(items: T[]) => items[Math.floor(next() * items.length)];
  const encoder = new TextEncoder();
  const parts: string[] = ['---\ntitle: 合成测试文档\n---\n', '# 深入理解智能体系统\n'];
  let bytes = 0, chapter = 0, paragraphs = 0;
  const keywordChance = () => hitsPerMegabyte / (1_000_000 / 180);
  const sentence = () => {
    let text = '';
    const length = 6 + Math.floor(next() * 10);
    for (let i = 0; i < length; i++) {
      text += next() < 0.15 ? ` ${pick(latin)} ` : pick(words);
      if (next() < keywordChance() / 6) text += ` **${keyword}** `;
      else if (next() < keywordChance() / 6) text += ` ${keyword} `;
    }
    return text + pick(['，', '；', '：']) + pick(words) + pick(['。', '。', '！', '？']);
  };
  while (bytes < targetBytes) {
    chapter++;
    let block = `\n## 第 ${chapter} 章 ${pick(words)}与${pick(words)}\n\n`;
    for (let section = 1; section <= 4; section++) {
      block += `### ${chapter}.${section} ${pick(words)}${next() < 0.3 ? ` ${keyword} ` : ''}友好的${pick(words)}设计\n\n`;
      for (let p = 0; p < 4; p++) {
        paragraphs++;
        block += `${sentence()}${sentence()}${sentence()}（参见[第 ${chapter} 章][ref${chapter % 7}]）${sentence()}\n\n`;
      }
      if (section === 2) block += `| 指标 | ${pick(words)} | ${pick(words)} |\n| --- | --- | --- |\n${Array.from({ length: 4 }, () => `| ${pick(words)} | ${Math.floor(next() * 900)} ms | ${pick(words)}，${pick(words)} |`).join('\n')}\n\n`;
      if (section === 3) block += '```python\nfor step in range(steps):\n    cache = model.forward(tokens, past_key_values=cache)\n```\n\n';
      if (section === 4) block += `- ${sentence()}\n- ${sentence()}\n  - ${sentence()}\n\n![图 ${chapter}.${section}](images/figure-${chapter}-${section}.png)\n\n`;
    }
    if (chapter % 9 === 0) block += '```mermaid\nflowchart LR\n  A[请求] --> B[缓存命中]\n  B --> C[生成]\n```\n\n';
    parts.push(block);
    bytes += encoder.encode(block).length;
  }
  parts.push('\n' + Array.from({ length: 7 }, (_, i) => `[ref${i}]: https://example.com/chapter-${i}`).join('\n') + '\n');
  return { body: parts.join(''), paragraphs };
}

// A cited paragraph roughly `position` of the way into the document.
export function citedSpan(body: string, position: number) {
  let from = body.indexOf('\n\n', Math.floor(body.length * position)) + 2;
  while (!/^[一-鿿]/.test(body.slice(from, from + 1))) from = body.indexOf('\n\n', from) + 2;
  const to = body.indexOf('\n\n', from);
  const encoder = new TextEncoder();
  const startLine = body.slice(0, from).split('\n').length;
  return {
    content: body.slice(from, to),
    span: { start_byte: encoder.encode(body.slice(0, from)).length, end_byte: encoder.encode(body.slice(0, to)).length, start_line: startLine, end_line: startLine, start_column: 1, end_column: 1 },
    startLine,
  };
}
