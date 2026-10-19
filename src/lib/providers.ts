export type ModelKind = 'answer' | 'jev';
type Provider = {value: string; label: string; url: string};

export const providers: Record<ModelKind, Provider[]> = {
 answer: [
  {value:'openai',label:'OpenAI',url:'https://api.openai.com/v1'},
  {value:'deepseek',label:'DeepSeek',url:'https://api.deepseek.com'},
  {value:'glm',label:'GLM',url:'https://open.bigmodel.cn/api/paas/v4'},
  {value:'openrouter',label:'OpenRouter',url:'https://openrouter.ai/api/v1'},
  {value:'302',label:'302.AI',url:'https://api.302ai.cn/v1'},
  {value:'custom',label:'自定义',url:''},
 ],
 jev: [
  {value:'302',label:'302.AI',url:'https://api.302ai.cn/typesafeai/v1'},
  {value:'openrouter',label:'OpenRouter',url:'https://openrouter.ai/api/alpha/decisions'},
  {value:'official',label:'Typesafe',url:'https://api.typesafe.ai/v1'},
 ],
};

export function providerFor(kind: ModelKind, address: string): string {
 let url: URL;
 try { url = new URL(address); } catch { return 'custom'; }
 if (['api.302.ai','api.302ai.cn'].includes(url.hostname)) return '302';
 return providers[kind].find(provider=>provider.url && new URL(provider.url).hostname===url.hostname)?.value ?? 'custom';
}

export function sameProviderOrigin(a: string, b: string): boolean {
 try { return new URL(a).origin===new URL(b).origin; } catch { return false; }
}
