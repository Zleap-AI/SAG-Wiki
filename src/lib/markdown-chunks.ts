// Splits Markdown into pieces that render to the same blocks as the whole text.
// marked's lexer is quadratic on JavaScriptCore (macOS WebView): one 1 MB pass
// takes about 12 s there, while 4 KB pieces take well under a millisecond each.
export type Chunk={from:number;to:number};

const fencePattern=/^ {0,3}(`{3,}|~{3,})(.*)$/;
const listMarker=/^(?:[-+*]|\d{1,9}[.)])(?:[ \t]|$)/;
const definitionStart=/^\[(?:\\[\s\S]|[^\]\\]){1,999}\]:/;
const htmlBlocks:[RegExp,RegExp][]=[
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
// processing instructions, declarations, CDATA) are never split. A single
// empty line also matters to tiptap, which turns extra trailing blank lines into
// empty paragraphs; see renderMarkdownPiece.
export function splitMarkdown(text:string,target=4000):Chunk[]{
 const chunks:Chunk[]=[];
 // The previous line was empty and the one before it had text.
 let start=0,fence='',html:RegExp|null=null,lastEmpty=false,lastText=false,textBeforeLast=false,lineStart=0;
 for(;;){
  const newline=text.indexOf('\n',lineStart),rawLine=text.slice(lineStart,newline<0?text.length:newline);
  // Inspect CRLF as a line ending without changing offsets into the original text.
  const line=rawLine.endsWith('\r')?rawLine.slice(0,-1):rawLine;
  // Keep definitions with the preceding block: a repeated definition can vanish
  // from the token stream, leaving the preceding blank line at the document end.
  if(!fence&&!html&&lastEmpty&&textBeforeLast&&lineStart-start>=target&&/^\S/.test(line)&&!listMarker.test(line)&&!definitionStart.test(text.slice(lineStart,lineStart+2004))){
   chunks.push({from:start,to:lineStart});start=lineStart;
  }
  if(html){if(html.test(line))html=null}
  else if(!fence){
   const block=htmlBlocks.find(([open])=>open.test(line));
   // The closer may sit on the opening line, after the opener itself.
   if(block&&!block[1].test(line.replace(block[0],'')))html=block[1];
  }
  const fenceMatch=html?null:fencePattern.exec(line);
  if(fenceMatch){
   if(!fence){if(fenceMatch[1][0]==='~'||!fenceMatch[2].includes('`'))fence=fenceMatch[1]}
   else if(fenceMatch[1][0]===fence[0]&&fenceMatch[1].length>=fence.length&&/^ *$/.test(fenceMatch[2]))fence='';
  }
  textBeforeLast=lastText;lastText=line.trim()!=='';lastEmpty=line==='';
  if(newline<0)break;
  lineStart=newline+1;
 }
 chunks.push({from:start,to:text.length});
 return chunks;
}

// Index of the chunk holding `offset`; the last chunk owns the end of the text.
export function chunkAt(chunks:Chunk[],offset:number){
 let low=0,high=chunks.length-1;
 while(low<high){const mid=(low+high+1)>>1;if(chunks[mid].from<=offset)low=mid;else high=mid-1}
 return low;
}
