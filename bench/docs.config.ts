import {defineConfig} from 'vite';
import appConfig from '../vite.config';
export default defineConfig({plugins:appConfig.plugins,optimizeDeps:{entries:['bench/docs.html']},server:{host:'127.0.0.1',port:1437,strictPort:true},build:{target:'es2022',outDir:'dist-docs',rollupOptions:{input:'bench/docs.html'}},preview:{host:'127.0.0.1',port:1437,strictPort:true}});
