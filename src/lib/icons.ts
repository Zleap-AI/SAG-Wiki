import brandIconUrl from '../assets/brand-icon.png';
import {
 ArrowLeft, ArrowLeftRight, ArrowRight, ArrowUp, Bold, Check, ChevronDown, ChevronRight, ChevronsLeft, CircleHelp, CodeXml, Copy,
 Download, Ellipsis, FileText, Folder, FolderUp, Globe, History, Info, Italic, List, ListOrdered,
 Maximize, MessageSquareText, Minus, Moon, PanelLeft, Pause, Play, Pencil, Plus, Quote,
 Redo2, RefreshCw, Search, SlidersHorizontal, Square, Strikethrough, Sun, Trash2, Undo2, X, Plug,
 type LucideIconData,
} from '@lucide/icons';
import {buildLucideSvg} from '@lucide/icons/build';

const glyph:Record<string,LucideIconData>={
 folder:Folder,'folder-up':FolderUp,file:FileText,chat:MessageSquareText,search:Search,plus:Plus,
 chevron:ChevronRight,'chevrons-left':ChevronsLeft,down:ChevronDown,check:Check,globe:Globe,settings:SlidersHorizontal,
 sun:Sun,moon:Moon,arrow:ArrowUp,back:ArrowLeft,forward:ArrowRight,close:X,panel:PanelLeft,info:Info,
 edit:Pencil,download:Download,refresh:RefreshCw,more:Ellipsis,bold:Bold,italic:Italic,
 strike:Strikethrough,bulletList:List,orderedList:ListOrdered,blockquote:Quote,
 codeBlock:CodeXml,undo:Undo2,redo:Redo2,stop:Square,minimize:Minus,maximize:Maximize,
 copy:Copy,delete:Trash2,pause:Pause,play:Play,history:History,
 plug:Plug,switch:ArrowLeftRight,help:CircleHelp,
};
export const icon=(name:string)=>buildLucideSvg(glyph[name],{
 className:'icon',hasA11yProp:false,attributes:{focusable:'false'},
});
export const logo=`<img class="brand-mark" src="${brandIconUrl}" alt="" aria-hidden="true" draggable="false">`;
