export const noteSources={
 notion:{name:'Notion',accept:'.zip',hint:'从 Notion 导出 HTML，勾选包含子页面，然后选择 ZIP 文件。'},
 evernote:{name:'Evernote',accept:'.enex',hint:'从 Evernote 导出 ENEX 文件。可一次选择多个笔记本。'},
 keep:{name:'Google Keep',accept:'.zip',hint:'从 Google Takeout 导出 Keep 数据，然后选择 ZIP 文件。已删除笔记不会导入。'},
 apple:{name:'Apple Notes',accept:'',hint:''},
} as const;
export type NoteSource=keyof typeof noteSources;
