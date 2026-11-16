import {pieceDocument,pieceText,type MarkdownModel} from './document-model';

export type OutlineHeading={offset:number;piece:number;ordinal:number;title:string;level:number};
const headingSelector='h1,h2,h3,h4,h5,h6';
export {headingSelector};

export function pieceHeadings(model:MarkdownModel,index:number):OutlineHeading[]{
 const p=model.pieces[index],result:OutlineHeading[]=[];
 // Plain prose cannot contain an ATX, Setext or HTML heading without one of these characters.
 if(p.source||!/[#=<-]/.test(pieceText(model,index)))return result;
 // The original piece start remounts it; the DOM ordinal identifies the heading even after an empty one.
 pieceDocument(model,index).body.querySelectorAll(headingSelector).forEach((el,ordinal)=>{
  const title=el.textContent?.trim();
  if(title)result.push({offset:p.from,piece:index,ordinal,title,level:Number(el.tagName[1])});
 });
 return result;
}

export class ReaderLayout{
 private heights:number[];
 private tops:number[]=[0];
 constructor(model:MarkdownModel){
  this.heights=model.pieces.map(piece=>Math.max(36,(piece.to-piece.from)*0.65));
  this.rebuild(0);
 }
 private rebuild(from:number){for(let i=from;i<this.heights.length;i++)this.tops[i+1]=this.tops[i]+this.heights[i];}
 update(measured:Map<number,number>){
  let first=this.heights.length;
  for(const [index,height] of measured)if(height>0&&this.heights[index]!==height){this.heights[index]=height;first=Math.min(first,index);}
  this.rebuild(first);
 }
 top(index:number){return this.tops[index];}
 get total(){return this.tops[this.heights.length];}
 at(top:number){
  let lo=0,hi=this.heights.length-1;
  while(lo<hi){const mid=(lo+hi+1)>>1;if(this.tops[mid]<=top)lo=mid;else hi=mid-1;}
  return lo;
 }
 window(top:number,viewport:number){
  const visible=this.at(top),first=Math.max(this.at(top-2*viewport),visible-3);
  return {first,last:Math.min(this.heights.length-1,first+11,Math.max(visible,this.at(top+3*viewport)))};
 }
}
