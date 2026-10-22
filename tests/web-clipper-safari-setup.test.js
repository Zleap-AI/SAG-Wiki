import {readFileSync} from 'node:fs';
import {JSDOM} from 'jsdom';
import {afterEach,expect,it,vi} from 'vitest';

let dom;
afterEach(()=>dom?.window.close());
function setup() {
 dom=new JSDOM(readFileSync('browser-extension/safari/Resources/Base.lproj/Main.html','utf8'),{url:'https://safari-setup.test',runScripts:'outside-only'});
 const postMessage=vi.fn();
 dom.window.webkit={messageHandlers:{controller:{postMessage}}};
 dom.window.eval(readFileSync('browser-extension/safari/Resources/Script.js','utf8'));
 return {postMessage,button:dom.window.document.querySelector('button'),state:dom.window.document.querySelector('#state')};
}

it('lets the user open Safari settings even while status detection has not returned',()=>{
 const ui=setup();
 expect(ui.button.disabled).toBe(false);
 ui.button.click();
 expect(ui.postMessage).toHaveBeenLastCalledWith('open-preferences');
});

it('shows a failed or missing status without reporting enabled and permits another check',()=>{
 const ui=setup();
 dom.window.show(true);
 for(const error of ['not-found','timeout','SFErrorDomain (3)']) {
  dom.window.show(null,error);
  expect(ui.state.textContent).not.toContain('enabled.');
  expect(ui.state.textContent).not.toContain('Checking');
  expect(ui.button.disabled).toBe(false);
  ui.button.click();
  expect(ui.postMessage).toHaveBeenLastCalledWith('open-preferences');
  const retry=dom.window.document.querySelector('#retry');
  expect(retry.hidden).toBe(false);
  expect(retry.disabled).toBe(false);
  retry.click();
  expect(ui.postMessage).toHaveBeenLastCalledWith('check-state');
 }
});

it('clears stale success while checking again and handles enabling after a retry',()=>{
 const ui=setup();
 dom.window.show(true);
 expect(ui.button.textContent).toBe('Done');
 dom.window.show(null);
 expect(ui.button.textContent).not.toBe('Done');
 expect(ui.state.textContent).toContain('Checking');
 dom.window.show(false);
 expect(ui.state.textContent).toContain('not enabled');
 dom.window.show(true);
 expect(ui.button.textContent).toBe('Done');
 expect(dom.window.document.querySelector('#retry').hidden).toBe(true);
});
