export const indexStages=[
 {stage:'indexing_native',title:'文本索引',progress:'正在索引文本',description:'Markdown、文本、代码等直接索引，完成后即可搜索。'},
 {stage:'converting',title:'文件转换',progress:'正在转换文件',description:'其他格式转换为 Markdown，已索引的文件仍可搜索。'},
 {stage:'indexing_converted',title:'转换结果索引',progress:'正在索引转换结果',description:'将转换结果加入索引，原文件保持不变。'},
];

// Maps raw indexing errors to an actionable hint; null means no specific cause was recognized.
export function indexErrorHint(message:string|null|undefined){
 if(!message)return null;
 if(/permission denied|access is denied|拒绝访问|权限不足/i.test(message))return '无法读取文件：请检查文件或文件夹的访问权限。';
 if(/no such file|file not found|找不到文件|文件不存在|路径不存在/i.test(message))return '文件已移动或删除，请确认文件仍位于该文件夹中。';
 if(/unsupported|not support|不支持|无法转换/i.test(message))return '文件格式暂不支持。';
 // Rust reports binary files as invalid UTF-8; re-saving them as text would not help.
 if(/valid utf-?8|binary/i.test(message))return '文件不是可识别的文本格式，已跳过。';
 if(/utf-?16|gbk|encoding|编码/i.test(message))return '文件编码无法识别，请将文件另存为 UTF-8 后重试。';
 if(/timeout|timed out|超时/i.test(message))return '处理文件超时，请稍后重新索引。';
 return null;
}
