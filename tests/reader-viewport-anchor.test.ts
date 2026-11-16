// @vitest-environment jsdom
import {expect,it,vi} from 'vitest';
import {captureViewportAnchor,restoreViewportAnchor} from '../src/lib/reader-viewport-anchor';
it('retains the same visible character when an image and a rewrapped paragraph move it within its piece',()=>{
 const host=document.createElement('div'),section=document.createElement('section');
 section.className='reader-piece';section.innerHTML='<img><p>Visible paragraph text</p>';host.append(section);document.body.append(host);
 const text=section.querySelector('p')!.firstChild!;
 host.scrollTop=100;let contentY=114;
 const rect=(top:number)=>({top,bottom:top+16,height:16,left:0,right:300,width:300});
 vi.spyOn(host,'getBoundingClientRect').mockReturnValue({...rect(0),bottom:300,height:300} as DOMRect);
 const caret=document.createRange();caret.setStart(text,8);caret.collapse(true);
 Object.defineProperty(document,'caretRangeFromPoint',{value:()=>caret,configurable:true});
 Object.defineProperty(Range.prototype,'getBoundingClientRect',{value:function(this:Range){
  expect(this.startContainer).toBe(text);expect(this.startOffset).toBe(8);return rect(contentY-host.scrollTop);
 },configurable:true});
 try{
  const anchor=captureViewportAnchor(host);expect(anchor).not.toBeNull();
  contentY+=600;expect(restoreViewportAnchor(host,anchor)).toBe(true);expect(host.scrollTop).toBe(700);
  contentY+=48;expect(restoreViewportAnchor(host,anchor)).toBe(true);expect(host.scrollTop).toBe(748);
  section.remove();expect(restoreViewportAnchor(host,anchor)).toBe(false);
 }finally{Reflect.deleteProperty(document,'caretRangeFromPoint');Reflect.deleteProperty(Range.prototype,'getBoundingClientRect');host.remove();}
});
