import { handleAuthCallback } from './lib/platform';
try { handleAuthCallback(); }
catch (error) { document.getElementById('status')!.textContent = String(error); }
