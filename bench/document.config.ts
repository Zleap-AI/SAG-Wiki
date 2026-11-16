import {defineConfig} from 'vite';
import appConfig from '../vite.config';
import {documentFixtureMiddleware} from './document-fixture-server.mjs';

const fixtures=()=>({name:'local-document-fixture',
 configureServer(server:any){server.middlewares.use(documentFixtureMiddleware(process.env.SAG_DOCUMENT_FIXTURE));},
 configurePreviewServer(server:any){server.middlewares.use(documentFixtureMiddleware(process.env.SAG_DOCUMENT_FIXTURE));}
});
export default defineConfig({plugins:[...(appConfig.plugins??[]),fixtures()],
 server:{host:'127.0.0.1',port:1438,strictPort:true,fs:{strict:true,allow:['.']}},
 build:{target:'es2022',outDir:'dist-bench/document',emptyOutDir:true,rollupOptions:{input:'bench/document.html'}},
 preview:{host:'127.0.0.1',port:1438,strictPort:true}
});
