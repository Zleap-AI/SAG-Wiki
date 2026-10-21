// @vitest-environment happy-dom
import {afterEach, beforeEach, expect, it, vi} from 'vitest';
import {mount, unmount} from 'svelte';
import CliSettings from '../src/components/CliSettings.svelte';
import {api} from '../src/lib/api';

vi.mock('../src/lib/api',()=>({api:vi.fn()}));
let component:ReturnType<typeof mount>;
beforeEach(()=>{
 document.body.innerHTML='';
 vi.mocked(api).mockReset().mockImplementation(async command=>{
  if(command==='get_access')return {enabled:true};
  if(command==='list_roots')return [{path:'/vol1/test'}];
  if(command==='fnos_remote_status')return {enabled:false,port:15679};
  if(command==='fnos_remote_rotate')return {token:'test-only-token'};
  if(command==='fnos_cli_run')return {code:0,stdout:'',stderr:'installed'};
  throw new Error(`Unexpected command: ${command}`);
 });
 component=mount(CliSettings,{target:document.body});
});
afterEach(async()=>{await unmount(component)});

it('opens with usable remote instructions and a downloadable Skill',async()=>{
 await vi.waitFor(()=>expect(document.querySelector<HTMLInputElement>('#remote-panel input')?.value).toContain(':15679/mcp'));
 const panel=document.querySelector<HTMLElement>('#remote-panel')!;
 expect(panel.hidden).toBe(false);
 expect(panel.textContent).toContain('~/.codex/config.toml');
 expect(panel.querySelector<HTMLAnchorElement>('a[download]')?.getAttribute('href')).toContain('/skills/sag-fnos-skill.zip');
 expect(panel.textContent).toContain('~/.agents/skills/sag-fnos');
 expect(panel.textContent).toContain('3. 开始使用');
});

it('installs and removes the selected NAS Agent Skill through the real CLI interface',async()=>{
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[aria-label="安装 Codex"]')?.disabled).toBe(false));
 document.querySelector<HTMLButtonElement>('#nas-tab')!.click();
 await vi.waitFor(()=>expect(document.querySelector<HTMLElement>('#nas-panel')!.hidden).toBe(false));
 document.querySelector<HTMLButtonElement>('[aria-label="安装 Codex"]')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('fnos_cli_run',{command:'sag install codex',directory:'/vol1/test'}));
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('[aria-label="卸载 Claude Code"]')!.disabled).toBe(false));
 document.querySelector<HTMLButtonElement>('[aria-label="卸载 Claude Code"]')!.click();
 await vi.waitFor(()=>expect(api).toHaveBeenCalledWith('fnos_cli_run',{command:'sag uninstall claude-code',directory:'/vol1/test'}));
 expect(document.querySelector<HTMLAnchorElement>('#nas-panel a[download]')?.getAttribute('href')).toContain('/skills/sag-cli-skill.zip');
 expect(document.querySelector<HTMLDetailsElement>('#nas-panel details')?.open).toBe(true);
});

it('keeps a newly generated remote configuration when switching between connection guides',async()=>{
 await vi.waitFor(()=>expect(document.querySelector<HTMLButtonElement>('#remote-panel .actions button')!.disabled).toBe(false));
 document.querySelector<HTMLButtonElement>('#remote-panel .actions button')!.click();
 await vi.waitFor(()=>expect(document.querySelector('#remote-panel')!.textContent).toContain('密钥仅在本次可复制'));
 document.querySelector<HTMLButtonElement>('#nas-tab')!.click();
 document.querySelector<HTMLButtonElement>('#remote-tab')!.click();
 await vi.waitFor(()=>expect(document.querySelector('#remote-panel')!.textContent).toContain('密钥仅在本次可复制'));
 expect(document.querySelector('#remote-panel pre')!.textContent).not.toContain('test-only-token');
});
