/** Word-level diff for comparing a cited passage with its current text. */
export type DiffPart={kind:'same'|'removed'|'added';text:string};
function tokens(text:string):string[]{return text.match(/\s+|[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]/gu)??[];}
export function wordDiff(before:string,after:string):DiffPart[]{
 const a=tokens(before),b=tokens(after);
 const n=a.length,m=b.length;
 // Bound the table so very long passages still render promptly.
 if(n*m>4_000_000)return[{kind:'removed',text:before},{kind:'added',text:after}];
 const table=new Uint32Array((n+1)*(m+1));
 for(let i=n-1;i>=0;i--)for(let j=m-1;j>=0;j--){table[i*(m+1)+j]=a[i]===b[j]?table[(i+1)*(m+1)+j+1]+1:Math.max(table[(i+1)*(m+1)+j],table[i*(m+1)+j+1]);}
 const parts:DiffPart[]=[];
 const push=(kind:DiffPart['kind'],text:string)=>{const last=parts.at(-1);if(last&&last.kind===kind)last.text+=text;else parts.push({kind,text});};
 let i=0,j=0;
 while(i<n&&j<m){
  if(a[i]===b[j]){push('same',a[i]);i++;j++;}
  else if(table[(i+1)*(m+1)+j]>=table[i*(m+1)+j+1]){push('removed',a[i]);i++;}
  else{push('added',b[j]);j++;}
 }
 while(i<n)push('removed',a[i++]);
 while(j<m)push('added',b[j++]);
 return parts;
}
