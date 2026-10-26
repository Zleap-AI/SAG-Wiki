// @vitest-environment happy-dom
import {afterEach,beforeEach,expect,it,vi} from 'vitest';
import {followChat} from '../src/lib/chat-scroll';

let contentHeight=1000,viewportHeight=200;
let host:HTMLDivElement;
let action:ReturnType<typeof followChat>;
const frame=()=>new Promise(requestAnimationFrame);
const bottom=()=>Math.max(0,contentHeight-viewportHeight);

beforeEach(()=>{
 contentHeight=1000;viewportHeight=200;
 host=document.createElement('div');host.append(document.createElement('div'));document.body.append(host);
 Object.defineProperty(host,'scrollHeight',{get:()=>contentHeight});
 Object.defineProperty(host,'clientHeight',{get:()=>viewportHeight});
 vi.stubGlobal('ResizeObserver',class{observe(){}disconnect(){}});
 action=followChat(host,[0]);
});
afterEach(()=>{action.destroy();host.remove();vi.unstubAllGlobals()});

it.each(['process collapse','larger viewport'])('keeps following after browser clamps scrollTop on %s',async(reason)=>{
 await frame();expect(host.scrollTop).toBe(bottom());
 // Browsers clamp the old scrollTop when a process panel closes or the viewport grows.
 if(reason==='process collapse')contentHeight=650;else viewportHeight=550;
 host.scrollTop=bottom();host.dispatchEvent(new Event('scroll'));
 contentHeight+=200;action.update([0,'new streamed text']);
 await frame();expect(host.scrollTop).toBe(bottom());
});

it('keeps a reader paused when layout also shrinks after they scroll up',async()=>{
 await frame();host.dispatchEvent(new WheelEvent('wheel',{deltaY:-1}));
 host.scrollTop=700;host.dispatchEvent(new Event('scroll'));
 contentHeight=650;host.scrollTop=bottom();host.dispatchEvent(new Event('scroll'));
 contentHeight+=200;action.update([0,'new streamed text']);
 await frame();expect(host.scrollTop).toBe(450);
});
