import { ArrowDown, ArrowRight, Database, FileText, ScanSearch, ListChecks, RefreshCw } from 'lucide-react';

type Language = 'zh' | 'en';

export function RetrievalDiagram({ lang }: { lang: Language }) {
  const zh = lang === 'zh';
  const steps = zh
    ? [
        { title: '召回候选', detail: '用问题中的词项查找原文', label: 'BM25 · 本地', icon: ScanSearch },
        { title: '判断证据', detail: '结合问题与已有证据评分', label: 'Jev · 服务请求', icon: ListChecks },
        { title: '返回原文', detail: '本地排序，保留文件与位置', label: '检索结果', icon: FileText },
      ]
    : [
        { title: 'Retrieve candidates', detail: 'Match question terms to source text', label: 'BM25 · local', icon: ScanSearch },
        { title: 'Judge evidence', detail: 'Score with the question and context', label: 'Jev · service call', icon: ListChecks },
        { title: 'Return sources', detail: 'Rank locally; retain paths and locations', label: 'Search results', icon: FileText },
      ];
  return (
    <figure className="sag-diagram not-prose">
      <div className="sag-diagram-heading"><span>01</span><strong>{zh ? '从建库到检索' : 'From indexing to retrieval'}</strong></div>
      <div className="sag-index-flow">
        <FileText size={21} aria-hidden="true" />
        <span>{zh ? '本地文档' : 'Local documents'}</span>
        <ArrowRight size={18} aria-hidden="true" />
        <span>{zh ? '文本处理与切分' : 'Process & split text'}</span>
        <ArrowRight size={18} aria-hidden="true" />
        <Database size={21} aria-hidden="true" />
        <strong>SQLite FTS5</strong>
      </div>
      <div className="sag-index-note">{zh ? '建立全文索引 · 无需向量嵌入 · 不调用 LLM' : 'Build a full-text index · No embeddings · No LLM calls'}</div>
      <div className="sag-query"><span>{zh ? '你的问题' : 'Your question'}</span><ArrowDown size={16} aria-hidden="true" /></div>
      <div className="sag-retrieval-flow">
        {steps.map(({ title, detail, label, icon: Icon }, i) => (
          <div className={`sag-flow-step sag-flow-step-${i}`} key={title}>
            <div className="sag-flow-label"><Icon size={17} aria-hidden="true" />{label}</div>
            <strong>{title}</strong><p>{detail}</p>
            {i < steps.length - 1 && <ArrowRight className="sag-step-arrow" size={18} aria-hidden="true" />}
          </div>
        ))}
      </div>
      <div className="sag-expansion"><RefreshCw size={18} aria-hidden="true" /><div><strong>{zh ? '按需扩展，再次召回与评分' : 'Expand, retrieve and score again as needed'}</strong><p>{zh ? '从高分原文提取词项与名称，继续查找其他片段。' : 'Terms and names from high-ranked passages retrieve more source material.'}</p></div></div>
      <figcaption>{zh ? '检索到的原文可直接查看，也可交给回答模型生成带引用的答案。' : 'Read the retrieved sources directly, or pass them to an answer model for a cited response.'}</figcaption>
    </figure>
  );
}

export function EvidenceDiagram({ lang }: { lang: Language }) {
  const zh = lang === 'zh';
  return (
    <figure className="sag-diagram sag-evidence not-prose">
      <div className="sag-diagram-heading"><span>02</span><strong>{zh ? '跨文件查找如何连接事实' : 'How facts connect across files'}</strong></div>
      <div className="sag-example-question">{zh ? '“登录后，系统如何确定用户能访问哪些接口？”' : '“After login, how does the system decide which APIs a user can access?”'}</div>
      <div className="sag-evidence-list">
        <div className="sag-source">
          <div className="sag-source-heading"><FileText size={17} aria-hidden="true" /><strong>authentication.md</strong><span>01</span></div>
          <p>{zh ? <>登录成功后，会话保存用户的 <mark>role_id</mark>。</> : <>After login, the session stores the user’s <mark>role_id</mark>.</>}</p>
        </div>
        <div className="sag-source-link"><ArrowDown size={15} aria-hidden="true" /><span>{zh ? '从原文提取 role_id，继续检索' : 'Extract role_id to retrieve more evidence'}</span></div>
        <div className="sag-source">
          <div className="sag-source-heading"><FileText size={17} aria-hidden="true" /><strong>roles.md</strong><span>02</span></div>
          <p>{zh ? <>每个 <mark>role_id</mark> 对应一组 <mark>permission_keys</mark>。</> : <>Each <mark>role_id</mark> maps to a set of <mark>permission_keys</mark>.</>}</p>
        </div>
        <div className="sag-source-link"><ArrowDown size={15} aria-hidden="true" /><span>{zh ? '根据 permission_keys 补充查找' : 'Follow permission_keys to another source'}</span></div>
        <div className="sag-source">
          <div className="sag-source-heading"><FileText size={17} aria-hidden="true" /><strong>api-permissions.md</strong><span>03</span></div>
          <p>{zh ? <>接口检查用户的 <mark>permission_keys</mark> 是否包含所需权限。</> : <>The API checks whether <mark>permission_keys</mark> includes the required permission.</>}</p>
        </div>
      </div>
      <figcaption>{zh ? '示例内容，仅用于说明关联方式。实际由 Jev 判断证据价值，词项扩展与匹配在本地进行。' : 'Illustrative content only. Jev judges evidence value; term expansion and matching run locally.'}</figcaption>
    </figure>
  );
}
