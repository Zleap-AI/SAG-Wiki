import {expect,it} from 'vitest';
import {indexErrorHint} from '../src/lib/index-stages';
it('watch failures keep the automatic-update warning ahead of generic permission hints',()=>{
 expect(indexErrorHint('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。\nPermission denied'))
  .toBe('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。');
});
