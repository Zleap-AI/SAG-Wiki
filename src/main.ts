import { mount } from 'svelte';
import App from './App.svelte';
import './tokens.css';
import './prototype.css';
import './app.css';
import './native.css';
import './components/settings.css';
import {initializeStorage} from './lib/identity';
import {basePath} from './lib/platform';
async function start() {
 const response=await fetch(`${basePath}api/health`);
 if(!response.ok)throw new Error('请从飞牛桌面打开 SAG Wiki。');
 const identity=await response.json();
 initializeStorage(identity.uid);
 mount(App,{target:document.getElementById('app')!});
}
void start().catch(error=>{document.getElementById('app')!.textContent=String(error);});

import './components/ui/controls.css';
