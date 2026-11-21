import type {MineruConfig,ModelConfig,Settings} from './types';
import {providers,sameProviderOrigin} from './providers';

const provider302=(kind:'answer'|'jev')=>providers[kind].find(provider=>provider.value==='302')!.url;

// One 302.AI key configures the answer model, the Jev decision model and MinerU parsing.
export const quickSetup={
 answerUrl:provider302('answer'),
 answerModel:'qwen3.8-flash',
 jevUrl:provider302('jev'),
 jevModel:'jev-latest',
} as const;

const mineruDefaults:MineruConfig={enabled:false,provider:'302',base_url:'',has_key:false,request_timeout_secs:60,task_timeout_secs:1800,poll_interval_secs:2,concurrency:2,model_version:'vlm',version:'2.5',parse_method:'auto',ocr:true,table:true,formula:true};

export function quickAnswerConfig(current:ModelConfig):ModelConfig{
 const keep=sameProviderOrigin(current.url,quickSetup.answerUrl)?current.models.filter(model=>model!==quickSetup.answerModel):[];
 return {...current,url:quickSetup.answerUrl,model:quickSetup.answerModel,models:[quickSetup.answerModel,...keep],auth_mode:'bearer',has_key:false,reasoning_effort:undefined};
}

// Mirrors the app's readiness check: a missing `has_key` comes from an older service and counts as configured.
export function jevConfigured(config:ModelConfig|undefined):boolean{
 return !!config&&(config.model??'').trim()!==''&&(config.auth_mode==='none'||config.has_key!==false);
}

export function quickJevConfig(current:ModelConfig):ModelConfig{
 return {...current,url:quickSetup.jevUrl,model:quickSetup.jevModel,auth_mode:'bearer',has_key:false};
}

export function quickMineruConfig(current:MineruConfig|undefined):MineruConfig{
 return {...mineruDefaults,...current,provider:'302',base_url:'',has_key:false};
}

// Builds the settings to save after every test passed. `mineru` also routes PDF
// parsing to 302.AI MinerU. A Jev or MinerU that is already configured is kept.
export function quickSettings(settings:Settings,mineru:boolean):Settings{
 const next:Settings=structuredClone(settings);
 next.answer=quickAnswerConfig(settings.answer);
 if(!jevConfigured(settings.jev))next.jev=quickJevConfig(settings.jev);
 const models=next.answer.models;
 if(next.search_summary_model&&!models.includes(next.search_summary_model))next.search_summary_model='';
 if(mineru){
  next.mineru=quickMineruConfig(settings.mineru);
  next.document_parser='mineru';next.document_parser_model='';
  const formats=settings.conversion_formats??null;
  // Conversion was off, so only the PDF selection is newly meaningful. A list
  // that was already active keeps its formats and gains PDF.
  if(!settings.convert_other_formats)next.conversion_formats=['pdf'];
  else if(formats&&!formats.includes('pdf'))next.conversion_formats=[...formats,'pdf'];
  next.convert_other_formats=true;
 }else if(next.document_parser==='vision'&&!models.includes(next.document_parser_model??'')){
  next.document_parser='docling';next.document_parser_model='';
 }
 return next;
}
