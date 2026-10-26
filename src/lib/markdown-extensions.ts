import {Table,TableCell,TableHeader,TableRow} from '@tiptap/extension-table';
import {TaskItem,TaskList} from '@tiptap/extension-list';
import type {AnyExtension,MarkdownTokenizer} from '@tiptap/core';

// The upstream Table and TaskList Markdown tokenizers split the whole remaining
// source on every block, which makes parsing quadratic: a 1 MB document took
// seconds and 5 MB took minutes. These wrappers give them only the text they
// can inspect, so results are identical and parsing stays linear.
type Tokenizer=MarkdownTokenizer&{start:(src:string)=>number};

function firstLines(src:string,count:number){
 let end=-1;
 for(let line=0;line<count;line++){end=src.indexOf('\n',end+1);if(end<0)return src}
 return src.slice(0,end);
}

// Shortest prefix that still holds the first `count` non-space characters plus the
// whitespace run after them, so an anchored pattern with `\s` runs sees the same text.
function leadingTokens(src:string,count:number){
 let seen=0,index=0;
 while(index<src.length&&seen<count){if(!/\s/.test(src[index]))seen++;index++}
 while(index<src.length&&/\s/.test(src[index]))index++;
 return src.slice(0,index);
}

function firstContentLine(src:string){
 let start=0;
 for(;;){
  const end=src.indexOf('\n',start),line=end<0?src.slice(start):src.slice(start,end);
  if(line.trim()!==''||end<0)return line;
  start=end+1;
 }
}

const tableTokenizer=Table.config.markdownTokenizer as Tokenizer;
// Only lines 0 and 1 matter to start(); tokenize() never reads past the first blank line.
export const BoundedTable=Table.extend({markdownTokenizer:{
 ...tableTokenizer,
 start:src=>tableTokenizer.start(firstLines(src,2)),
 tokenize:(src,tokens,lexer)=>{const blank=src.indexOf('\n\n');return tableTokenizer.tokenize(blank<0?src:src.slice(0,blank),tokens,lexer)},
}});

// Same members and order as TableKit.
export const BoundedTableKit:AnyExtension[]=[BoundedTable,TableCell,TableHeader,TableRow];

const taskItemPattern=/^(\s*)([-+*])\s+\[([ xX])\]\s+(.*)$/;
const taskTokenizer=TaskList.config.markdownTokenizer as Tokenizer;
// start() is anchored and spans at most four non-space characters; tokenize() bails
// unless the first non-blank line is a task item.
export const BoundedTaskList=TaskList.extend({markdownTokenizer:{
 ...taskTokenizer,
 start:src=>taskTokenizer.start(leadingTokens(src,4)),
 tokenize:(src,tokens,lexer)=>taskItemPattern.test(firstContentLine(src))?taskTokenizer.tokenize(src,tokens,lexer):undefined,
}});

export {TaskItem};
