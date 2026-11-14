// @vitest-environment happy-dom
import {afterEach,expect,it,vi} from 'vitest';
import {api} from '../src/lib/api';
vi.mock('../src/lib/web-connection',()=>({web:true,webRequest:vi.fn()}));
afterEach(()=>vi.restoreAllMocks());
it('opens a known about target in an isolated browser tab',async()=>{
 const open=vi.spyOn(window,'open').mockReturnValue(null);
 await api('open_about_link',{target:'project'});
 expect(open).toHaveBeenCalledWith('https://github.com/Zleap-AI/SAG-Wiki','_blank','noopener,noreferrer');
});
it.each(['unknown','__proto__'])('rejects unknown about target %s without opening a tab',async target=>{
 const open=vi.spyOn(window,'open').mockReturnValue(null);
 await expect(api('open_about_link',{target})).rejects.toThrow('未知的关于页面链接');
 expect(open).not.toHaveBeenCalled();
});
