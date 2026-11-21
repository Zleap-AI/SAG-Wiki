import brandIconUrl from '../assets/brand-icon.png';
import {
 ArrowLeft, ArrowLeftRight, ArrowRight, ArrowUp, ArrowUpRight, Bold, Check, ChevronDown, ChevronRight, ChevronsLeft, CircleHelp, CodeXml, Copy,
 Clock3, Rss, Share2, Download, Ellipsis, FileText, Folder, FolderUp, Globe, History, Info, Italic, List, ListOrdered,
 Maximize, MessageSquareText, Minus, Moon, Network, PanelLeft, Pause, Play, Pencil, Plus, Quote,
 Redo2, RefreshCw, Route, Search, SlidersHorizontal, Square, Strikethrough, Sun, Trash2, Undo2, X, Plug, CircleAlert,
 Eye, EyeOff,
 type LucideIconData,
} from '@lucide/icons';
import {buildLucideSvg} from '@lucide/icons/build';

const glyph:Record<string,LucideIconData>={
 folder:Folder,eye:Eye,'eye-off':EyeOff,'folder-up':FolderUp,file:FileText,chat:MessageSquareText,search:Search,plus:Plus,
 chevron:ChevronRight,'chevrons-left':ChevronsLeft,down:ChevronDown,check:Check,globe:Globe,settings:SlidersHorizontal,
 sun:Sun,moon:Moon,arrow:ArrowUp,back:ArrowLeft,forward:ArrowRight,close:X,panel:PanelLeft,info:Info,
 edit:Pencil,download:Download,refresh:RefreshCw,more:Ellipsis,bold:Bold,italic:Italic,
 strike:Strikethrough,bulletList:List,orderedList:ListOrdered,blockquote:Quote,
 codeBlock:CodeXml,undo:Undo2,redo:Redo2,stop:Square,minimize:Minus,maximize:Maximize,
 clock:Clock3,rss:Rss,share:Share2,copy:Copy,delete:Trash2,pause:Pause,play:Play,history:History,
 externalLink:ArrowUpRight,plug:Plug,network:Network,route:Route,switch:ArrowLeftRight,help:CircleHelp,alert:CircleAlert,
};
export const icon=(name:string)=>buildLucideSvg(glyph[name],{
 className:'icon',hasA11yProp:false,attributes:{focusable:'false'},
});
export const logo=`<img class="brand-mark" src="${brandIconUrl}" alt="" aria-hidden="true" draggable="false">`;
