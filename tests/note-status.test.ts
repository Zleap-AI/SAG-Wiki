// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {mount,tick,unmount} from 'svelte';
import NoteStatus from '../src/components/NoteStatus.svelte';
import {wordDiff} from '../src/lib/word-diff';
import type {NoteDetail,NoteSourceStatus} from '../src/lib/types';

let app:ReturnType<typeof mount>|undefined;
afterEach(async()=>{if(app)await unmount(app);app=undefined;document.body.innerHTML=''});
function source(id:number,state:NoteSourceStatus['state'],extra:Partial<NoteSourceStatus>={}):NoteSourceStatus{
 return{source:{id,path:`/kb/src/file${id}.rs`,hash:'h',start_byte:0,end_byte:10,start_line:42,end_line:67,text:'fn check(user) {\n  old\n}'},state,...extra};
}
function detail(sources:NoteSourceStatus[],state:NoteDetail['summary']['state']):NoteDetail{
 return{summary:{path:'/kb/notes/n.md',title:'n',question:'权限在哪里校验？',asked_at:1_761_370_800_000,model:'m',scope:['/kb/src'],state,source_count:sources.length,attention:sources.filter(s=>s.state==='changed'||s.state==='missing'||s.state==='unverified').length,checked_at:1_761_370_800_000},sources};
}
function show(note:NoteDetail){
 const onRecheck=vi.fn(),onOpenSource=vi.fn(),onReask=vi.fn();
 app=mount(NoteStatus,{target:document.body,props:{note,onRecheck,onOpenSource,onReask}});
 return{onRecheck,onOpenSource,onReask};
}
const click=(text:string)=>[...document.querySelectorAll<HTMLButtonElement>('button')].find(b=>b.textContent?.includes(text))!.click();

it('汇总需要关注的引用，并按状态展示每条来源',async()=>{
 show(detail([
  source(1,'moved',{current:{start_byte:0,end_byte:10,start_line:50,start_column:1,end_line:75,end_column:1}}),
  source(2,'changed',{similarity:0.71,current_text:'fn check(user, action) {\n  new\n}'}),
  source(3,'missing',{reason:'文件已删除'}),
 ],'missing'));
 const status=document.querySelector('.note-status')!;
 expect(status.classList.contains('attention')).toBe(true);
 expect(status.textContent).toContain('2 条引用需要关注');
 click('查看引用');await tick();
 const rows=[...document.querySelectorAll('.note-source')];
 expect(rows.map(r=>r.getAttribute('data-state'))).toEqual(['moved','changed','missing']);
 expect(rows[0].textContent).toContain('第 50–75 行（保存时 42–67）');
 expect(rows[1].textContent).toContain('相似度 71%');
 expect(rows[2].textContent).toContain('文件已删除');
 expect(rows[2].querySelector('button[class*="outline"]')?.textContent).toContain('保存时原文');
});

it('对照视图用删除线和高亮展示保存时与当前内容的差异',async()=>{
 const {onOpenSource}=show(detail([source(2,'changed',{similarity:0.71,current_text:'fn check(user, action) {\n  new\n}'})],'changed'));
 click('查看引用');await tick();
 click('查看对照');await tick();
 const diff=document.querySelector('.note-compare-diff')!;
 expect(diff.querySelector('del')?.textContent).toBe('old');
 expect([...diff.querySelectorAll('ins')].map(i=>i.textContent).join('')).toContain('action');
 click('打开原文');
 expect(onOpenSource).toHaveBeenCalledWith(expect.objectContaining({state:'changed'}));
 document.querySelector<HTMLButtonElement>('[aria-label="关闭对照"]')!.click();await tick();
 expect(document.querySelector('.note-compare')).toBeNull();
});

it('解析失败的笔记显示原因，没有引用的笔记说明无需核对',async()=>{
 show({...detail([],'unverified'),summary:{...detail([],'unverified').summary,parse_error:'缺少 sag_note'}});
 expect(document.querySelector('.note-status')!.textContent).toContain('笔记格式无法解析：缺少 sag_note');
 await unmount(app!);app=undefined;document.body.innerHTML='';
 const {onRecheck,onReask}=show(detail([],'intact'));
 expect(document.querySelector('.note-status')!.textContent).toContain('此笔记没有引用，不需要核对。');
 click('重新检查');expect(onRecheck).toHaveBeenCalled();
 click('重新提问');expect(onReask).toHaveBeenCalled();
});

it('词级 diff 保留相同部分并标出增删',()=>{
 const parts=wordDiff('fn check(user) -> bool','fn check(user, action) -> bool');
 expect(parts.filter(p=>p.kind==='removed')).toEqual([]);
 expect(parts.filter(p=>p.kind==='added').map(p=>p.text).join('')).toBe(', action');
 expect(parts.map(p=>p.kind==='removed'?'':p.text).join('')).toBe('fn check(user, action) -> bool');
 expect(wordDiff('','')).toEqual([]);
 expect(wordDiff('甲 乙','甲 丙')).toEqual([{kind:'same',text:'甲 '},{kind:'removed',text:'乙'},{kind:'added',text:'丙'}]);
});
