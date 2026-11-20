import {expect,it} from 'vitest';
import {indexErrorHint} from '../src/lib/index-stages';
import {translate} from '../src/lib/i18n';
it('watch failures keep the automatic-update warning ahead of generic permission hints',()=>{
 expect(indexErrorHint('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。\nPermission denied'))
  .toBe('文件监听失败，自动更新受限；请在索引设置中重建索引以重试。');
});

it('retains a localized remedy for a current or legacy watch quota failure',()=>{
 const current='文件监听失败：已达到 Linux inotify 监听上限，自动更新受限。请调大 fs.inotify.max_user_watches（例如 sudo sysctl fs.inotify.max_user_watches=524288），然后重启 SAG 后台服务。';
 const old='文件监听失败，自动更新受限；请在索引设置中重建索引以重试。\nOS file watch limit reached. about ["/tmp/notes"]';
 expect(indexErrorHint(old)).toBe(current);
 expect(indexErrorHint(`未能索引 1 个文件\n\n/tmp/file\nPermission denied\n\n${current}`)).toBe(current);
 expect(translate(current,'en')).toContain('restart the SAG background service');
});

it.each(['inotify-notes','watch limit notes','OS file watch limit reached.'])('does not infer a quota failure from a folder named %s',folder=>{
 const warning='文件监听失败，自动更新受限；请在索引设置中重建索引以重试。';
 expect(indexErrorHint(`${warning}\nPermission denied. about ["/home/me/${folder}"]`)).toBe(warning);
});
