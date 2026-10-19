import catalog from './model-capabilities.json';
export type ReasoningEffort = 'none'|'low'|'medium'|'high'|'max';
export const reasoningLabels:Record<ReasoningEffort,string>={none:'关闭',low:'低',medium:'中',high:'高',max:'极高'};

export function modelCapability(model:string,endpoint:string){
 let host:string;
 try{host=new URL(endpoint).hostname}catch{return undefined}
 return catalog.models.find(item=>item.ids.includes(model.toLowerCase())&&item.hosts.includes(host));
}

export function reasoningLevels(model:string,endpoint:string):{value:ReasoningEffort|null;label:string}[]{
 const defaults={value:null,label:'默认'};
 const levels=modelCapability(model,endpoint)?.levels.map(level=>level.value as ReasoningEffort)??[];
 return [defaults,...levels.map(value=>({value,label:reasoningLabels[value]}))];
}
