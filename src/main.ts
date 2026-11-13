import { mount } from 'svelte';
import App from './App.svelte';
import QuickSearch from './QuickSearch.svelte';
import WebApp from './WebApp.svelte';
import {web} from './lib/web-connection';
import './tokens.css';
import './prototype.css';
import './app.css';
import './native.css';
import './styles/search.css';
import './components/settings.css';
mount(web?WebApp:new URLSearchParams(location.search).has('quick-search')?QuickSearch:App,{target:document.getElementById('app')!});

import './components/ui/controls.css';
