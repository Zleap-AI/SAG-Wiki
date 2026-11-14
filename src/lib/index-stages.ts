import type {FileInfo,IndexTask} from './types';

export const indexStages=[
 {stage:'indexing_native',title:'文本索引',progress:'正在检查文本索引',description:'检查 Markdown、文本、代码等文件；复用未变化的索引，仅更新新增或变化的内容，完成后即可搜索。'},
 {stage:'converting',title:'文件转换',progress:'正在检查文件转换',description:'检查其他格式的转换缓存，复用可用结果，需要时才解析文件；已索引的文件仍可搜索。'},
 {stage:'indexing_converted',title:'转换结果索引',progress:'正在检查转换结果',description:'检查转换结果的索引，复用未变化的内容，仅更新新增或变化的内容；原文件保持不变。'},
];

export const progressiveStages=[indexStages[0],
 {stage:'extracting_basic',title:'基础文本',progress:'正在检查基础文本',description:'复用已有基础文本，仅在需要时提取文字层并更新索引；完成后即可搜索，扫描内容等待 OCR。'},
 {stage:'enhancing',title:'OCR 与表格增强',progress:'正在后台增强 OCR 与表格',description:'已有基础文本的文档可继续搜索；扫描内容和复杂表格可能尚不完整，纯扫描件需等待 OCR。'},
];
export function indexPipeline(stage:string){return ['extracting_basic','enhancing','retrying_enhancement'].includes(stage)?progressiveStages:indexStages;}

// Maps raw indexing errors to an actionable hint; null means no specific cause was recognized.
export function indexErrorHint(message:string|null|undefined){
 if(!message)return null;
 if(message.includes('文件监听失败'))return '文件监听失败，自动更新受限；请在索引设置中重建索引以重试。';
 if(/permission denied|access is denied|拒绝访问|权限不足/i.test(message))return '无法读取文件：请检查文件或文件夹的访问权限。';
 if(/no such file|file not found|找不到文件|文件不存在|路径不存在/i.test(message))return '文件已移动或删除，请确认文件仍位于该文件夹中。';
 if(/unsupported|not support|不支持|无法转换/i.test(message))return '文件格式暂不支持。';
 // Rust reports binary files as invalid UTF-8; re-saving them as text would not help.
 if(/valid utf-?8|binary/i.test(message))return '文件不是可识别的文本格式，已跳过。';
 if(/utf-?16|gbk|encoding|编码/i.test(message))return '文件编码无法识别，请将文件另存为 UTF-8 后重试。';
 if(/timeout|timed out|超时/i.test(message))return '处理文件超时，请稍后重新索引。';
 return null;
}

// Watch setup is a preparation step, not an additional file-indexing phase.
export function indexStage(stage:string|undefined,parsing=false){
 if(stage==='converting'&&parsing)return {...indexStages[1],progress:'正在解析文件'};
 if(stage==='awaiting_ocr')return {stage,title:'等待 OCR',progress:'正在识别扫描内容',description:'没有可用文字层，扫描内容尚不可搜索；识别完成后会自动加入索引。'};
 if(stage==='retrying_enhancement')return {stage,title:'重试增强',progress:'正在重试后台增强',description:'正在重试 OCR 与表格增强；已有基础文本仍可搜索，纯扫描件需等待识别完成。'};
 if(stage==='retrying_conversion')return {stage,title:'重试转换',progress:'尝试重试转换',description:'正在重试此前失败的文件，已成功转换的文件继续复用。'};
 if(stage==='scanning')return {stage,title:'文件扫描',progress:'正在扫描文件',description:'正在检查文件夹中的文件及索引范围。'};
 if(stage==='preparing_watch')return {stage,title:'文件监听',progress:'正在准备文件监听',description:'正在准备文件监听，大文件夹可能耗时数分钟；已索引内容仍可搜索。'};
 return [...indexStages,...progressiveStages].find(item=>item.stage===stage);
}

// Only active files in conversion stages contribute to the live parser label.
export function isConversionStage(stage:string|undefined){return !!stage&&['converting','retrying_conversion','enhancing','retrying_enhancement'].includes(stage);}
export function conversionParsers(tasks:IndexTask[]){
 const engines=new Set(tasks.filter(task=>isConversionStage(task.stage))
  .flatMap(task=>(task.conversions??[]).filter(item=>task.files.includes(item.path)).map(item=>item.engine).filter(Boolean)));
 return ['MinerU','anydoc','Docling'].filter(engine=>engines.delete(engine)).concat([...engines].sort());
}

// One compact document summary; details retain progress for every concurrent file.
export function conversionSegment(tasks:IndexTask[]){
 const item=tasks.filter(task=>isConversionStage(task.stage))
  .flatMap(task=>(task.conversions??[]).filter(item=>task.files.includes(item.path)))
  .find(item=>item.segment);
 return item?.segment?{...item.segment,path:item.path}:undefined;
}

export function fileIndexState(file:(Pick<FileInfo,'index_status'|'index_error'>&{searchable?:boolean})|undefined){
 if(file?.searchable===false&&file.index_status==='enhancing')return {label:'内容解析中 · 尚不可搜索',description:'正在解析文档内容，尚无可搜索的正文；解析完成后会自动加入索引。'};
 if(file?.searchable===false&&file.index_status==='enhancement_failed')return {label:'识别失败 · 尚不可搜索',description:'扫描内容未能完成识别，请查看索引详情并重试。'};
 switch(file?.index_status){
  case 'awaiting_conversion':return {label:'等待解析',description:'尚未提取到可搜索的正文，内容尚不可搜索；后台将继续尝试适用的解析方式。'};
  case 'basic':return {label:'基础文本可搜索 · 等待增强',description:'基础文本已可搜索，OCR 和表格增强尚未完成。'};
  case 'enhancing':return {label:'基础文本可搜索 · 增强中',description:'基础文本已可搜索，正在后台补充 OCR 和表格。'};
  case 'enhancement_failed':return {label:'基础文本可搜索 · 增强失败',description:'基础文本仍可搜索；OCR 或表格增强失败，请查看索引详情并重试。'};
  case 'awaiting_ocr':return {label:'等待 OCR',description:'没有可用文字层，扫描内容尚不可搜索；识别完成后会自动加入索引。'};
  default:return undefined;
 }
}
