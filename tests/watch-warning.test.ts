import {expect,it} from 'vitest';
import {indexErrorHint} from '../src/lib/index-stages';
it('watch failures keep the automatic-update warning ahead of generic permission hints',()=>{
 expect(indexErrorHint('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。\nPermission denied'))
  .toBe('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。');
});
it('inotify watch-limit failures explain the sysctl remedy instead of advising a rebuild',()=>{
 const hint='文件监听失败：系统监视数量已达上限（Linux inotify）。请先调大上限并重启应用：sudo sysctl fs.inotify.max_user_watches=524288（写入 /etc/sysctl.conf 可持久化）；在此之前重建索引仍会失败。';
 expect(indexErrorHint('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。\nOS file watch limit reached. about ["/tmp/p1"]')).toBe(hint);
 expect(indexErrorHint(hint)).toBe(hint);
});
