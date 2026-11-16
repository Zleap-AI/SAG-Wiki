import {expect,it} from 'vitest';
import {createFeedbackMonitor,createFrameGapMonitor} from '../bench/document-frame-gaps';

it('includes blocking work before the first animation frame in the worst gap',()=>{
 const monitor=createFrameGapMonitor(100);
 // A parser blocks the thread for 12 seconds before the browser delivers its first RAF.
 monitor.frame(12100);monitor.frame(12116);
 expect(monitor.worstFrameMs).toBe(12000);
});
it('includes the unfinished final frame gap when navigation ends or cancels',()=>{
 const monitor=createFrameGapMonitor(50);
 monitor.frame(66);monitor.frame(82);monitor.frame(500);
 expect(monitor.worstFrameMs).toBe(418);
});
it('records feedback only after it has survived an opportunity to paint',()=>{
 const monitor=createFeedbackMonitor(100);
 monitor.frame(116,null);monitor.frame(132,'loading');
 expect(monitor.feedback.firstPaintMs).toBeNull();
 monitor.frame(148,'content');
 expect(monitor.feedback).toEqual({firstPaintMs:48,kind:'loading'});
 monitor.frame(200,'content');
 expect(monitor.feedback.firstPaintMs).toBe(48);
});
it('does not count a status removed before the next frame as visible feedback',()=>{
 const monitor=createFeedbackMonitor(0);
 monitor.frame(16,'loading');monitor.frame(32,null);monitor.frame(48,'content');
 expect(monitor.feedback.firstPaintMs).toBeNull();
 monitor.frame(64,'content');
 expect(monitor.feedback).toEqual({firstPaintMs:64,kind:'content'});
});
