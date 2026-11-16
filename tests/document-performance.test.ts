import {describe,expect,it,vi} from 'vitest';
import {createDocumentPerformance,afterDocumentPaint} from '../src/lib/document-performance';

describe('opt-in document performance traces',()=>{
 it('does no clock or frame work while disabled',async()=>{
  const now=vi.fn(()=>10),frame=vi.fn();
  const performance=createDocumentPerformance({now});
  const trace=performance.begin('editor');
  trace.stage('read-rpc')();trace.firstPaint();trace.end();trace.cancel();
  await afterDocumentPaint(trace,()=>true,frame);
  expect(now).not.toHaveBeenCalled();expect(frame).not.toHaveBeenCalled();
  expect(performance.snapshot()).toEqual([]);
 });
 it('aggregates repeated stages and returns detached, finite, bounded samples',()=>{
  let clock=0;const performance=createDocumentPerformance({now:()=>clock});
  performance.configure({enabled:true,maxSamples:2});
  for(let i=0;i<4;i++){
   const trace=performance.begin('reader');
   for(let n=0;n<100;n++){const stop=trace.stage('reader-render');clock+=2;stop();stop();}
   trace.firstPaint();trace.end();
  }
  const samples=performance.snapshot();
  expect(samples).toHaveLength(2);
  expect(samples[0].stages['reader-render']).toEqual({durationMs:200,count:100});
  expect(samples[0].durationMs).toBe(200);
  samples[0].stages['reader-render']!.durationMs=999;
  expect(performance.snapshot()[0].stages['reader-render']!.durationMs).toBe(200);
  expect(JSON.stringify(samples)).not.toMatch(/NaN|Infinity/);
 });
 it('cancels overlapping navigation and ignores late completion of the old request',()=>{
  let clock=0;const performance=createDocumentPerformance({now:()=>clock});performance.configure({enabled:true});
  const first=performance.begin('editor'),stop=first.stage('read-rpc');clock=5;
  const second=performance.begin('reader');clock=9;stop();first.firstPaint();first.end();
  second.stage('model')();second.firstPaint();second.end();
  expect(performance.snapshot().map(sample=>({status:sample.status,paint:sample.firstPaintMs,duration:sample.durationMs}))).toEqual([
   {status:'cancelled',paint:null,duration:5},{status:'complete',paint:4,duration:4}
  ]);
  expect(performance.snapshot()[0].stages['read-rpc']).toBeUndefined();
 });
 it('drops invalid runtime stage names instead of retaining paths or content',()=>{
  const performance=createDocumentPerformance();performance.configure({enabled:true});
  const trace=performance.begin('editor');
  trace.stage('/private/customer-name/secret.md' as never)();
  trace.stage('# private document body' as never)();
  trace.end();
  expect(JSON.stringify(performance.snapshot())).not.toMatch(/private|customer|secret|body/);
  expect(performance.snapshot()[0].stages).toEqual({});
 });
 it('bounds configuration, sanitizes a broken clock, and cancels handles on reset or disable',()=>{
  const performance=createDocumentPerformance({now:()=>NaN});performance.configure({enabled:true,maxSamples:Infinity});
  const trace=performance.begin('editor');trace.stage('read-rpc')();trace.firstPaint();trace.end();
  expect(performance.snapshot()[0].durationMs).toBe(0);
  expect(performance.snapshot()[0].firstPaintMs).toBe(0);
  const stale=performance.begin('reader');performance.reset();stale.end();
  expect(performance.snapshot()).toEqual([]);
  performance.begin('editor');performance.configure({enabled:false});
  expect(performance.snapshot().at(-1)?.status).toBe('cancelled');
  expect(performance.begin('reader').enabled).toBe(false);
 });
 it('marks first paint only after microtasks and two frames and ignores disposed content',async()=>{
  let clock=0;const performance=createDocumentPerformance({now:()=>clock});performance.configure({enabled:true});
  const frames:FrameRequestCallback[]=[];
  const frame=(callback:FrameRequestCallback)=>{frames.push(callback);return frames.length;};
  const trace=performance.begin('reader');const painted=afterDocumentPaint(trace,()=>true,frame);
  expect(frames).toHaveLength(0);await Promise.resolve();expect(frames).toHaveLength(1);
  clock=3;frames.shift()!(clock);expect(performance.snapshot()[0].firstPaintMs).toBeNull();
  clock=7;frames.shift()!(clock);await painted;
  expect(performance.snapshot()[0].firstPaintMs).toBe(7);
  expect(performance.snapshot()[0].status).toBe('complete');
  let current=true;const disposed=performance.begin('editor');const cancelled=afterDocumentPaint(disposed,()=>current,frame);
  await Promise.resolve();current=false;frames.shift()!(8);await cancelled;
  expect(frames).toHaveLength(0);
  expect(performance.snapshot().at(-1)?.status).toBe('cancelled');
 });
 it('does not schedule another frame after a navigation has cancelled the trace',async()=>{
  const performance=createDocumentPerformance();performance.configure({enabled:true});
  const frames:FrameRequestCallback[]=[],trace=performance.begin('reader');
  const painted=afterDocumentPaint(trace,()=>true,callback=>frames.push(callback));
  await Promise.resolve();trace.cancel();frames.shift()!(0);await painted;
  expect(frames).toHaveLength(0);
  expect(performance.snapshot()[0].firstPaintMs).toBeNull();
 });
 it('labels the resolved renderer without resetting the original RPC or trace identity',()=>{
  let clock=0;const performance=createDocumentPerformance({now:()=>clock});performance.configure({enabled:true});
  const trace=performance.begin('readonly'),stop=trace.stage('read-rpc');clock=8;stop();
  trace.setRenderer('editor');trace.setRenderer('/private/document.md' as never);clock=12;trace.end();
  expect(performance.snapshot()).toMatchObject([{id:1,renderer:'editor',durationMs:12,stages:{'read-rpc':{durationMs:8,count:1}}}]);
  expect(JSON.stringify(performance.snapshot())).not.toContain('private');
 });
});
