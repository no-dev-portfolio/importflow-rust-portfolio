import fs from 'node:fs/promises';
import assert from 'node:assert/strict';
import {build} from 'esbuild';
import {JSDOM} from 'jsdom';
const dom=new JSDOM('<!doctype html><html><body><div id="root"></div></body></html>',{url:'http://localhost/'});
globalThis.window=dom.window;globalThis.document=dom.window.document;
Object.defineProperty(globalThis,'navigator',{value:dom.window.navigator,configurable:true});
globalThis.HTMLElement=dom.window.HTMLElement;globalThis.IS_REACT_ACT_ENVIRONMENT=true;
const source=(await fs.readFile('app.tsx','utf8')).replace("createRoot(document.getElementById('root')!).render(<App/>);",'export {App};');
await build({stdin:{contents:source,resolveDir:process.cwd(),sourcefile:'app-review.tsx',loader:'tsx'},bundle:true,format:'esm',platform:'node',outfile:'.role-race-test.mjs',external:['react','react-dom/client']});
const {App}=await import('../.role-race-test.mjs');const React=await import('react');const{createRoot}=await import('react-dom/client');
const{act}=React;
const log=[];let cancelled=false;let switched=false;const held=[];
const job={id:'11111111-1111-4111-8111-111111111111',tenant:'factory-a',label:'A社だけの商品取込',status:'queued',attempts:0,version:1,row_count:0,issues:[],message:'',created_at:'2026-10-27T00:00:00Z',updated_at:'2026-10-27T00:00:00Z'};
const summary=(a)=>({queued:a?1:0,running:0,completed:0,rejected:0,failed:0,cancelled:0,products:a?7:0,catalog_value:'0'});
globalThis.fetch=async(url,options={})=>{
 const token=options.headers.Authorization;const a=token==='Bearer demo-manager-a';
 log.push({path:url,tenant:a?'A':'B',afterSwitch:switched});
 if(options.method==='POST'){cancelled=true;return {ok:true,status:200,json:async()=>({...job,status:'cancelled',version:2})};}
 const detail=url.includes('/jobs/'+job.id);
 if(detail&&cancelled&&a){return new Promise((resolve,reject)=>{held.push({resolve,reject});options.signal?.addEventListener('abort',()=>reject(new DOMException('aborted','AbortError')),{once:true});});}
 let data=url.endsWith('/summary')?summary(a):url.endsWith('/catalog')?{data:[]}:url.endsWith('/audit')?[]:detail?job:{data:a?[job]:[]};
 return {ok:true,status:200,json:async()=>data};
};
const root=createRoot(document.getElementById('root'));
const click=async(name)=>{const b=Array.from(document.querySelectorAll('button')).find(b=>b.textContent.includes(name));assert.ok(b,'button missing '+name);await act(async()=>b.dispatchEvent(new dom.window.MouseEvent('click',{bubbles:true})));};
let result;
try{
 await act(async()=>root.render(React.createElement(App)));
 await click('A社だけの商品取込');
 await click('取込を取消');
 assert.equal(held.length,2,'post-mutation detail and audit should be waiting');
 const select=document.querySelector('select[aria-label="デモ利用者"]');
 switched=true;
 await act(async()=>{select.value='demo-manager-b';select.dispatchEvent(new dom.window.Event('change',{bubbles:true}));});
 await act(async()=>{await Promise.resolve();await Promise.resolve();});
 result={selectedRole:select.value,containsTenantAJob:document.body.textContent.includes('A社だけの商品取込'),oldTenantReadsAfterSwitch:log.filter(x=>x.afterSwitch&&x.tenant==='A'),requests:log};
 console.log(JSON.stringify(result,null,2));
 if(process.env.REVIEW_OUTPUT)await fs.writeFile(process.env.REVIEW_OUTPUT,JSON.stringify(result,null,2));
 assert.equal(result.oldTenantReadsAfterSwitch.length,0,'old tenant started reads after role switch');
 assert.equal(result.containsTenantAJob,false,'tenant A job was displayed after switching to tenant B');
}finally{await act(async()=>root.unmount());dom.window.close();await fs.rm('.role-race-test.mjs',{force:true});}
