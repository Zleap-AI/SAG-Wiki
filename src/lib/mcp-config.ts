import type {McpServer, McpCredentials} from './types';

function object(value: unknown): value is Record<string, unknown> {
 return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function stringMap(value: unknown, label: string): Record<string,string> {
 if(!object(value)||Object.values(value).some(v=>typeof v!=='string'))throw Error(`${label}必须是 JSON 对象，且值为字符串`);
 return value as Record<string,string>;
}
export function parseMcpCredentials(server:McpServer,draft:string|undefined):McpCredentials|undefined {
 if(!draft?.trim())return undefined;
 const value=stringMap(JSON.parse(draft),'MCP 环境变量和请求头');
 return server.transport==='http'?{headers:value,env:{}}:{headers:{},env:value};
}

export function parseMcpImport(text:string):{server:McpServer;credentials:McpCredentials}[] {
 let value:unknown;
 try {value=JSON.parse(text)} catch {throw Error('JSON 格式不正确，请检查逗号和引号。')}
 if(!object(value))throw Error('MCP 配置必须是 JSON 对象。');
 if('inputs' in value)throw Error('暂不支持 inputs 变量，请替换为实际值后导入。');
 const single='command' in value||'url' in value||'serverUrl' in value;
 const group=value.mcpServers??value.servers??value;
 if(!object(group))throw Error('服务列表必须是以服务名称为键的 JSON 对象。');
 const entries=single?[[typeof value.name==='string'?value.name:'',value] as const]:Object.entries(group);
 if(!entries.length)throw Error('配置中没有 MCP 服务。');
 return entries.map(([key,config])=>{
  if(!object(config))throw Error(`服务 ${key||''} 的配置必须是 JSON 对象。`);
  const unsupported=Object.keys(config).filter(key=>!['name','type','transport','url','serverUrl','command','args','env','headers'].includes(key));
  if(unsupported.length)throw Error(`服务 ${key||''} 包含暂不支持的字段：${unsupported.join('、')}。`);
  const url=config.url??config.serverUrl;
  const transport=config.type??config.transport??(url!==undefined?'http':'stdio');
  if(!['http','streamable-http','stdio'].includes(String(transport)))throw Error('仅支持 Streamable HTTP 和 stdio，暂不支持 SSE。');
  if(typeof config.name!=='undefined'&&typeof config.name!=='string')throw Error('服务名称必须是字符串。');
  let name=(key||String(config.name??'')).trim();
  const server:McpServer={id:crypto.randomUUID(),name,transport:transport==='stdio'?'stdio':'http',url:'',command:'',args:[],require_confirmation:true,has_credentials:false,tools:null};
  const credentials:McpCredentials={env:{},headers:{}};
  if(server.transport==='http'){
   if(typeof url!=='string'||!URL.canParse(url)||!['http:','https:'].includes(new URL(url).protocol))throw Error('请提供有效的 HTTP(S) 服务地址。');
   if(config.command!==undefined||config.args!==undefined||config.env!==undefined)throw Error('HTTP 服务不支持 command、args 或 env 字段。');
   server.url=url;name ||= new URL(url).hostname;
   if(config.headers!==undefined)credentials.headers=stringMap(config.headers,'请求头');
  }else{
   if(typeof config.command!=='string'||!config.command.trim())throw Error('stdio 服务需要填写 command。');
   if(url!==undefined||config.headers!==undefined)throw Error('stdio 服务不支持 url 或 headers 字段。');
   if(config.args!==undefined&&(!Array.isArray(config.args)||config.args.some(arg=>typeof arg!=='string')))throw Error('args 必须是字符串数组。');
   server.command=config.command;server.args=(config.args as string[]|undefined)??[];name ||= config.command.split(/[\\/]/).pop()!;
   if(config.env!==undefined)credentials.env=stringMap(config.env,'环境变量');
  }
  const values=[server.url,server.command,...server.args,...Object.values(credentials.env),...Object.values(credentials.headers)];
  if(values.some(value=>/\$\{[^}]+\}/.test(value)))throw Error('配置包含变量占位符，请替换为实际值后导入。');
  server.name=name;
  return {server,credentials};
 });
}
