import {build} from 'esbuild';
import fs from 'node:fs/promises';
await fs.mkdir('dist',{recursive:true});
await build({entryPoints:['app.tsx'],bundle:true,minify:true,outfile:'dist/app.js',target:'es2022',define:{'process.env.NODE_ENV':'"production"'}});
await fs.copyFile('index.html','dist/index.html');
