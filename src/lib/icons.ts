import brandIconUrl from '../assets/brand-icon.png';
import {
 ArrowLeft, ArrowUp, Bold, Check, ChevronDown, ChevronRight, CodeXml, Copy,
 Download, Ellipsis, FileText, Folder, Globe, Info, Italic, List, ListOrdered,
 Maximize, MessageSquareText, Minus, Moon, PanelLeft, Pencil, Plus, Quote,
 Redo2, RefreshCw, Search, SlidersHorizontal, Square, Strikethrough, Sun, Trash2, Undo2, X, Plug,
 type LucideIconData,
} from '@lucide/icons';
import {buildLucideSvg} from '@lucide/icons/build';

const glyph:Record<string,LucideIconData>={
 folder:Folder,file:FileText,chat:MessageSquareText,search:Search,plus:Plus,
 chevron:ChevronRight,down:ChevronDown,check:Check,globe:Globe,settings:SlidersHorizontal,
 sun:Sun,moon:Moon,arrow:ArrowUp,back:ArrowLeft,close:X,panel:PanelLeft,info:Info,
 edit:Pencil,download:Download,refresh:RefreshCw,more:Ellipsis,bold:Bold,italic:Italic,
 strike:Strikethrough,bulletList:List,orderedList:ListOrdered,blockquote:Quote,
 codeBlock:CodeXml,undo:Undo2,redo:Redo2,stop:Square,minimize:Minus,maximize:Maximize,
 copy:Copy,delete:Trash2,
 plug:Plug,
};
export const icon=(name:string)=>buildLucideSvg(glyph[name],{
 className:'icon',hasA11yProp:false,attributes:{focusable:'false'},
});
export const logo=`<img class="brand-mark" src="${brandIconUrl}" alt="" aria-hidden="true" draggable="false">`;
