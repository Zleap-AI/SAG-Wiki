import {expect,it} from 'vitest';
import {overviewEdges,graphViewportBounds,approachGraphPoint,graphFolderColor} from '../src/lib/graph-layout';
import type {ExploreEdge} from '../src/lib/explore';
it('大型总览减少重复连线但保持原有连通性，原始数据不变',()=>{
 const edges:ExploreEdge[]=[];
 for(let a=1;a<=50;a++)for(let b=a+1;b<=50;b++)edges.push({source:a,target:b,score:1-((a+b)%8)*.02,terms:['知识'],source_hash:'a',target_hash:'b',snapshot:'test'});
 const shown=overviewEdges(edges,50,.08);
 expect(shown.length).toBeLessThan(100);expect(edges).toHaveLength(1225);
 const visited=new Set([1]);let previous=0;
 while(previous!==visited.size){previous=visited.size;for(const e of shown)if(visited.has(e.source)||visited.has(e.target)){visited.add(e.source);visited.add(e.target)}}
 expect(visited.size).toBe(50);
 expect(overviewEdges(edges.slice(0,3),4,.08)).toHaveLength(3);
});

it('大图默认取景不受少量离群点影响，小图完整显示',()=>{
 const points=Array.from({length:100},(_,i)=>({x:i,y:i*2}));points.push({x:10000,y:-10000});
 expect(graphViewportBounds(points)).toEqual({x:50,y:98,width:96,height:192});
 expect(points).toHaveLength(101);
 expect(graphViewportBounds([{x:0,y:0},{x:100,y:20}])).toEqual({x:50,y:10,width:100,height:20});
});

it('逐帧缓动不越过目标，长帧不会瞬间跳到终点',()=>{
 const target={x:10,y:-10};let point={x:0,y:0};
 const delayed=approachGraphPoint(point,target,5000);expect(delayed.x).toBeLessThan(4);
 for(let frame=0;frame<180;frame++){
  const next=approachGraphPoint(point,target,16);
  expect(next.x).toBeGreaterThanOrEqual(point.x);expect(next.x).toBeLessThanOrEqual(target.x);point=next;
 }
 expect(point.x).toBeCloseTo(target.x,4);expect(point.y).toBeCloseTo(target.y,4);
});
it('同文件夹颜色稳定，兼容两种路径分隔符与深色主题',()=>{
 expect(graphFolderColor('/notes/team/a.md')).toBe(graphFolderColor('/notes/team/b.txt'));
 expect(graphFolderColor('/notes/team/a.md')).toBe(graphFolderColor('\\notes\\team\\b.md'));
 expect(new Set(['/a/one.md','/b/two.md','/c/three.md'].map(path=>graphFolderColor(path))).size).toBeGreaterThan(1);
 expect(graphFolderColor('/notes/team/a.md',true)).not.toBe(graphFolderColor('/notes/team/a.md'));
});
