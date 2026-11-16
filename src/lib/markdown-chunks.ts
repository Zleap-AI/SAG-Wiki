// Splits Markdown into pieces that render to the same blocks as the whole text.
// marked's lexer is quadratic on JavaScriptCore (macOS WebView): one 1 MB pass
// takes about 12 s there, while 4 KB pieces take well under a millisecond each.
export type Chunk={from:number;to:number};

const fencePattern=/^ {0,3}(`{3,}|~{3,})(.*)$/;
const listMarker=/^(?:[-+*]|\d{1,9}[.)])(?:[ \t]|$)/;
// Required indentation for a continuation block in a top-level list item.
function listContentIndent(line:string){
 const item=/^( {0,3})(?:[-+*]|\d{1,9}[.)])([ \t]{1,5}|$)/.exec(line);
 if(!item)return 0;
 const markerEnd=item[0].length-item[2].length;
 let end=markerEnd;
 for(const c of item[2])end+=c==='\t'?4-end%4:1;
 const padding=end-markerEnd;
 return markerEnd+(padding>=1&&padding<=4?padding:1);
}
const thematicBreak=/^ {0,3}(?:(?:-[ \t]*){3,}|(?:\*[ \t]*){3,}|(?:_[ \t]*){3,})$/;
const definitionStart=/^\[(?:\\[\s\S]|[^\]\\]){1,999}\]:/;
const htmlBlocks:[RegExp,RegExp][]=[
 [/^ {0,3}<table(?:\s|>|$)/i,/<\/table\s*>/i],
 [/^ {0,3}<p(?:\s|>|$)/i,/<\/p\s*>/i],
 [/^ {0,3}<a(?:\s|>|$)/i,/<\/a\s*>/i],
 [/^ {0,3}<h[1-6](?:\s|>|$)/i,/<\/h[1-6]\s*>/i],
 [/^ {0,3}<(?:script|pre|style|textarea)(?:\s|>|$)/i,/<\/(?:script|pre|style|textarea)>/i],
 [/^ {0,3}<!--/,/-->/],
 [/^ {0,3}<\?/,/\?>/],
 [/^ {0,3}<![A-Za-z]/,/>/],
 [/^ {0,3}<!\[CDATA\[/,/\]\]>/],
];

// A line can start a new chunk only after exactly one empty line, outside fences,
// at column zero, and when it is not a list item: in CommonMark such a line can
// never continue the previous block (lists, indented code, and lazy lines all
// need indentation or no blank line). Fences and the HTML blocks that may hold
// blank lines (CommonMark types 1-5: script/pre/style/textarea, comments,
// processing instructions, declarations, CDATA) and HTML content blocks are never split. A single
// empty line also matters to tiptap, which turns extra trailing blank lines into
// empty paragraphs; see renderMarkdownPiece.
export function* scanMarkdown(text:string,target=4000,semanticCuts=false,definitionCutLimit=Infinity):Generator<void,Chunk[]>{
 let checkpoint=0,listIndent=0;
 const chunks:Chunk[]=[];
 // The previous line was empty and the one before it had text.
 let start=0,fence='',html:RegExp|null=null,math='',lastEmpty=false,lastText=false,textBeforeLast=false,lineStart=0,previousEndedBlock=false;
 for(;;){
  const newline=text.indexOf('\n',lineStart),rawLine=text.slice(lineStart,newline<0?text.length:newline);
  // Inspect CRLF as a line ending without changing offsets into the original text.
  const line=rawLine.endsWith('\r')?rawLine.slice(0,-1):rawLine;
  // Keep definitions with the preceding block: a repeated definition can vanish
  // from the token stream, leaving the preceding blank line at the document end.
  const topLevel=!fence&&!html&&!math;
  const hasText=line.trim()!=='';
  const candidate=semanticCuts&&topLevel?/^( {0,3})#{1,6}(?:[ \t]|$)/.exec(line):null;
  if(semanticCuts&&topLevel&&hasText){
   let indent=0;
   for(let i=0;i<line.length&&indent<Math.max(4,listIndent);i++){
    if(line[i]===' ')indent++;else if(line[i]==='\t')indent+=4-indent%4;else break;
   }
   const rule=thematicBreak.test(line);
   const item=rule?0:listContentIndent(line);
   // A blank followed by an under-indented line closes the list. An ATX
   // heading also interrupts lazy paragraph continuation without a blank.
   const interrupt=candidate||item||rule||/^ {0,3}(?:>|\x60{3,}|~{3,}|<(?:table|p|h[1-6]|pre|script|style)(?:\s|>))/i.test(line);
   if(listIndent&&indent<listIndent&&(!lastText||interrupt))listIndent=0;
   if(item&&!listIndent)listIndent=item;
  }
  const heading=!!candidate&&(!listIndent||candidate[1].length<listIndent);
  const marker=fencePattern.exec(line),wasFenced=!!fence;
  const openingFence=semanticCuts&&topLevel&&line[0]!==' '&&marker&&(marker[1][0]==='~'||!marker[2].includes('`'));
  const oversized=lineStart-start>definitionCutLimit;
  const standalone=lastEmpty&&textBeforeLast||oversized&&!lastText;
  const definitionIndent=oversized&&(standalone||previousEndedBlock)?/^( {0,3})\[/.exec(line):null;
  const standaloneDefinition=definitionIndent&&(!listIndent||definitionIndent[1].length<listIndent)
   &&definitionStart.test(text.slice(lineStart+definitionIndent[1].length,lineStart+2007));
  const blankBoundary=standaloneDefinition||standalone&&/^\S/.test(line)&&!listMarker.test(line)&&(oversized||!definitionStart.test(text.slice(lineStart,lineStart+2004)));
  if(topLevel&&lineStart-start>=target&&(blankBoundary||heading||openingFence||(previousEndedBlock&&/^\S/.test(line)))){
   chunks.push({from:start,to:lineStart});start=lineStart;
  }
  if(html){if(html.test(line))html=null}
  else if(!fence){
   const block=htmlBlocks.find(([open])=>open.test(line));
   // The closer may sit on the opening line, after the opener itself.
   if(block&&!block[1].test(line.replace(block[0],'')))html=block[1];
  }
  // A display formula owns its blank lines: `aligned`/`cases` blocks often contain
  // them, and splitting there would turn one formula into several paragraphs.
  if(math){
   if((math==='$$'&&/\$\$[ \t]*$/.test(line))||(math==='\\['&&/\\\][ \t]*$/.test(line)))math='';
  }
  else if(!fence&&!html){
   const open=/^ {0,3}(\$\$|\\\[)/.exec(line);
   if(open){
    const rest=line.slice(open[0].length);
    const closed=open[1]==='$$'?/\$\$/.test(rest):/\\\]/.test(rest);
    if(!closed)math=open[1];
   }
  }
  const fenceMatch=html?null:marker;
  if(fenceMatch){
   if(!fence){if(fenceMatch[1][0]==='~'||!fenceMatch[2].includes('`'))fence=fenceMatch[1]}
   else if(fenceMatch[1][0]===fence[0]&&fenceMatch[1].length>=fence.length&&/^ *$/.test(fenceMatch[2]))fence='';
  }
  previousEndedBlock=heading||(semanticCuts&&wasFenced&&!fence);
  textBeforeLast=lastText;lastText=hasText;lastEmpty=line==='';
  if(newline<0)break;
  lineStart=newline+1;
  if(lineStart-checkpoint>=32_768){checkpoint=lineStart;yield;}
 }
 chunks.push({from:start,to:text.length});
 return chunks;
}

export function splitMarkdown(text:string,target=4000):Chunk[]{
 const scan=scanMarkdown(text,target);
 let step=scan.next();while(!step.done)step=scan.next();
 return step.value;
}

// Index of the chunk holding `offset`; the last chunk owns the end of the text.
export function chunkAt(chunks:Chunk[],offset:number){
 let low=0,high=chunks.length-1;
 while(low<high){const mid=(low+high+1)>>1;if(chunks[mid].from<=offset)low=mid;else high=mid-1}
 return low;
}
