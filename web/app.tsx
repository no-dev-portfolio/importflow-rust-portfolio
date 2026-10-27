import React,{useEffect,useRef,useState} from 'react';
import {createRoot} from 'react-dom/client';
import {createEpoch,yen} from './guard.mjs';
type Job={id:string,label:string,status:string,attempts:number,version:number,row_count:number,issues:{line:number,message:string}[],message:string,created_at:string,updated_at:string};
type Product={sku:string,name:string,quantity:number,unit_price:number};
type Audit={action:string,actor:string,created_at:string};
type Summary={queued:number,running:number,completed:number,rejected:number,failed:number,cancelled:number,products:number,catalog_value:string};
const names:Record<string,string>={queued:'待機中',running:'処理中',completed:'完了',rejected:'取込不可',failed:'処理失敗',cancelled:'取消済み'};
const actions:Record<string,string>={submitted:'取込を受付',claimed:'ワーカーが取得',completed:'全件の取込完了',rejected:'検証により全件拒否',failed:'処理が失敗',cancel:'取込を取消',retry:'管理者が再試行',queued:'一時障害により再試行待ち'};
const sample='sku,name,quantity,unit_price\nPART-401,通信アダプター,20,3200\nPART-402,接続ケーブル,50,850\n';
class ApiError extends Error { constructor(message:string,readonly status:number){super(message);} }
async function api<T>(token:string,path:string,options:RequestInit={},signal?:AbortSignal):Promise<T>{
 const own=AbortSignal.timeout(12000);const res=await fetch('/api'+path,{...options,signal:signal?AbortSignal.any([signal,own]):own,headers:{'Authorization':'Bearer '+token,'Content-Type':'application/json',...options.headers}});
 const data=await res.json();if(!res.ok)throw new ApiError(data.error?.message||'処理を完了できませんでした。',res.status);return data;
}
const date=(s:string)=>new Intl.DateTimeFormat('ja-JP',{month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit'}).format(new Date(s));
function Badge({status}:{status:string}){return <span className={'badge '+status}><i/>{names[status]||status}</span>}
function App(){
 const[token,setToken]=useState('demo-manager-a');const[tab,setTab]=useState('overview');const[jobs,setJobs]=useState<Job[]>([]);const[products,setProducts]=useState<Product[]>([]);const[summary,setSummary]=useState<Summary|null>(null);const[error,setError]=useState('');const[busy,setBusy]=useState(false);const[open,setOpen]=useState(false);const[selected,setSelected]=useState<Job|null>(null);const[audits,setAudits]=useState<Audit[]>([]);const[filter,setFilter]=useState('all');
 const activeToken=useRef(token);const roleEpoch=useRef(createEpoch());const epoch=useRef(createEpoch());const detailEpoch=useRef(createEpoch());const controller=useRef<AbortController|null>(null);const detailController=useRef<AbortController|null>(null);
 const write=token!=='demo-viewer-a';const manager=token.startsWith('demo-manager');
 async function load(scope=token,roleTicket=roleEpoch.current.current()){
  // A stale closure must not invalidate the current user's read generation.
  if(activeToken.current!==scope||!roleEpoch.current.isCurrent(roleTicket))return;
  const ticket=epoch.current.next();
  controller.current?.abort();
  const c=new AbortController();controller.current=c;
  try{
   const[s,j,p]=await Promise.all([
    api<Summary>(scope,'/summary',{},c.signal),
    api<{data:Job[]}>(scope,'/jobs',{},c.signal),
    api<{data:Product[]}>(scope,'/catalog',{},c.signal)
   ]);
   if(activeToken.current===scope&&roleEpoch.current.isCurrent(roleTicket)&&epoch.current.isCurrent(ticket)){
    setSummary(s);setJobs(j.data);setProducts(p.data);setError('');
   }
  }catch(e){
   if(activeToken.current===scope&&roleEpoch.current.isCurrent(roleTicket)&&epoch.current.isCurrent(ticket)&&!c.signal.aborted)
    setError(e instanceof Error?e.message:'接続を確認してください。');
  }
 }
 useEffect(()=>{void load();const timer=setInterval(()=>void load(),3000);return()=>{clearInterval(timer);epoch.current.next();controller.current?.abort();};},[token]);
 function role(value:string){activeToken.current=value;roleEpoch.current.next();epoch.current.next();detailEpoch.current.next();controller.current?.abort();detailController.current?.abort();setToken(value);setSelected(null);setAudits([]);setJobs([]);setProducts([]);setSummary(null);setOpen(false);setError('');setBusy(false);}
 async function detail(job:Job){if(activeToken.current!==token)return;const ticket=detailEpoch.current.next();detailController.current?.abort();const c=new AbortController();detailController.current=c;setSelected(job);setAudits([]);try{const[current,audit]=await Promise.all([api<Job>(token,'/jobs/'+job.id,{},c.signal),api<Audit[]>(token,'/jobs/'+job.id+'/audit',{},c.signal)]);if(detailEpoch.current.isCurrent(ticket)){setSelected(current);setAudits(audit);}}catch(e){if(detailEpoch.current.isCurrent(ticket)&&!c.signal.aborted)setError(String(e));}}
 async function command(job:Job,name:string){
  const scope=token;const roleTicket=roleEpoch.current.current();const ticket=detailEpoch.current.current();
  setBusy(true);
  try{
   const result=await api<Job>(scope,`/jobs/${job.id}/${name}`,{method:'POST',body:JSON.stringify({version:job.version})});
   if(activeToken.current===scope&&roleEpoch.current.isCurrent(roleTicket)&&detailEpoch.current.isCurrent(ticket)){
    await detail(result);
    // detail() may return after a role switch aborts its reads.
    if(!roleEpoch.current.isCurrent(roleTicket)||activeToken.current!==scope)return;
    await load(scope,roleTicket);
   }
  }catch(e){
   if(detailEpoch.current.isCurrent(ticket))setError(e instanceof Error?e.message:'操作に失敗しました。');
  }finally{
   if(roleEpoch.current.isCurrent(roleTicket))setBusy(false);
  }
 }
 const displayed=jobs.filter(j=>filter==='all'||j.status===filter);
 return <div className="app"><aside><a className="brand" href="/" aria-label="ImportFlow ホーム"><span className="logo">if</span>ImportFlow</a><span className="workspace-label">WORKSPACE</span><div className="workspace"><span className="workspace-mark">N</span><div>北関東ファクトリー<small>架空データ専用デモ</small></div></div><nav>{[['overview','▦','ダッシュボード'],['jobs','↳','取込ジョブ'],['catalog','▤','商品カタログ'],['guide','⌘','API・使い方']].map(([id,icon,name])=><button className={tab===id?'active':''} key={id} onClick={()=>setTab(id)}><span>{icon}</span>{name}{id==='jobs'&&summary&&<b>{summary.queued+summary.running}</b>}</button>)}</nav><div className="sidebar-note"><span className="dot"/>LOCAL DEMO<small>Rust + Tokio + PostgreSQL<br/>データを安全に、次の工程へ。</small></div><footer>ImportFlow <span>v1.0.1</span></footer></aside><main><header><div>ワークスペース <span>/</span> {token==='demo-manager-b'?'別組織':'データ取込'}</div><label className="role">デモ利用者<select aria-label="デモ利用者" value={token} onChange={e=>role(e.target.value)}><option value="demo-manager-a">管理者 A</option><option value="demo-operator-a">操作担当 A</option><option value="demo-viewer-a">閲覧担当 A</option><option value="demo-manager-b">別組織 管理者 B</option></select></label></header><section className="content"><div className="title-row"><div><div className="eyebrow">DATA OPERATIONS</div><h1>{tab==='catalog'?'商品カタログ':tab==='jobs'?'取込ジョブ':tab==='guide'?'API・使い方':'取込ワークスペース'}</h1><p>CSVの受付から検証、取込まで。処理の状況をひとつの画面で。</p></div>{write&&<button className="primary" onClick={()=>setOpen(true)}>＋ CSV取込を作成</button>}</div>{error&&<div className="alert" role="alert">{error}<button onClick={()=>setError('')}>閉じる</button></div>}
 {(tab==='overview'||tab==='jobs')&&<><div className="cards">{[['待機・処理中',(summary?.queued||0)+(summary?.running||0),'並行ワーカーで自動処理'],['取込完了',summary?.completed??'—','すべての行を登録済み'],['要確認',(summary?.rejected||0)+(summary?.failed||0),'詳細から原因を確認'],['取込済みSKU',summary?.products??'—','組織ごとの商品データ']].map(([label,value,note],i)=><div className={'card card-'+i} key={label}><div>{label}<span>{['◷','✓','!','▤'][i]}</span></div><strong>{value}<small>件</small></strong><p>{note}</p></div>)}</div>{tab==='overview'&&<div className="pipeline"><div><span className="live-dot"/>処理パイプライン<small>全件成功、または全件拒否</small></div><ol><li><b>01</b>CSV受付</li><li><b>02</b>形式・値を検証</li><li><b>03</b>重複を確認</li><li><b>04</b>まとめて取込 <span>✓</span></li></ol></div>}<div className="panel"><div className="panel-heading"><div><h2>{tab==='overview'?'最近の取込':'取込一覧'}</h2><p>最新50件。処理状態は3秒ごとに更新されます。</p></div><div className="table-controls"><select aria-label="状態で絞り込み" value={filter} onChange={e=>setFilter(e.target.value)}><option value="all">すべての状態</option>{Object.entries(names).map(([k,v])=><option key={k} value={k}>{v}</option>)}</select><button className="secondary" onClick={()=>void load()} aria-label="一覧を更新">↻ 更新</button></div></div><div className="table-scroll"><table><thead><tr><th>取込名 / ID</th><th>状態</th><th>登録行数</th><th>試行回数</th><th>更新日時</th><th/></tr></thead><tbody>{displayed.map(j=><tr key={j.id}><td><button className="job-link" onClick={()=>void detail(j)}><span className="file-icon">CSV</span><span>{j.label}<small>{j.id.slice(0,8)}</small></span></button></td><td><Badge status={j.status}/></td><td>{j.status==='completed'?j.row_count.toLocaleString():'—'}<small className="inline-unit">行</small></td><td>{j.attempts}<span className="subtle"> / 3</span></td><td className="subtle">{date(j.updated_at)}</td><td><button className="text-button" onClick={()=>void detail(j)}>詳細 ↗</button></td></tr>)}</tbody></table>{displayed.length===0&&<div className="empty">表示する取込ジョブはありません。</div>}</div><div className="panel-footer"><span>{displayed.length} 件を表示</span><span className="dot"/> 自動更新</div></div><div className="bottom-note"><span>ⓘ</span><p>不備のあるCSVは、一部の行だけを取り込まず全件を拒否します。<br/><strong>商品SKUは組織内で一意</strong>です。登録済みSKUの更新・上書きは行いません。</p><a href="/openapi.json" target="_blank" rel="noreferrer">API仕様を見る ↗</a></div></>}
 {tab==='catalog'&&<><div className="catalog-summary">登録済み <strong>{summary?.products||0} SKU</strong><span>数量×単価の合計 <b>{yen(summary?.catalog_value||'0')}</b></span></div><div className="panel table-scroll"><table><thead><tr><th>SKU</th><th>商品名</th><th>数量</th><th>単価</th></tr></thead><tbody>{products.map(p=><tr key={p.sku}><td className="mono">{p.sku}</td><td>{p.name}</td><td>{p.quantity.toLocaleString()}</td><td>{yen(String(p.unit_price))}</td></tr>)}</tbody></table>{products.length===0&&<div className="empty">登録済みの商品はありません。</div>}<div className="panel-footer">SKU順で先頭50件を表示。APIではページ指定が可能です。</div></div></>}
 {tab==='guide'&&<div className="panel guide"><h2>3分で試す</h2><ol><li>「CSV取込を作成」からサンプルを読み込み、登録します。</li><li>取込詳細で完了・取込不可の理由と操作履歴を確認します。</li><li>利用者を切り替え、閲覧権限と組織別データ分離を確認します。</li></ol><h2>CSV仕様</h2><pre>{sample}</pre><p>UTF-8 / 最大512KiB / 1,000行。数量は1〜100,000、単価は0〜100,000,000の整数です。</p><h2>API</h2><p>Bearerキーは画面のデモ利用者IDと同じです。実データには使わないでください。</p><code>POST /api/jobs + Idempotency-Key</code><br/><code>POST /api/graphql　{'{ jobs { id label status } }'}</code><p><a href="/openapi.json">OpenAPI仕様を開く</a></p><p>クラウド配備・外部MQ・本番認証・受託実績を示すデモではありません。</p></div>}
 <div className="page-footer">© ImportFlow · 自主制作ポートフォリオ <span>型による境界、取引による整合性。</span></div></section></main>
 <CreateDialog key={token} token={token} open={open} close={()=>setOpen(false)} created={()=>{setOpen(false);void load();}}/>
 {selected&&<div className="overlay"><section className="dialog" role="dialog" aria-modal="true" aria-label="取込詳細"><div className="dialog-heading"><div className="eyebrow">IMPORT DETAIL</div><button aria-label="詳細を閉じる" onClick={()=>{detailEpoch.current.next();detailController.current?.abort();setSelected(null);}}>×</button></div><h2>{selected.label}</h2><p className="mono subtle">{selected.id}</p><Badge status={selected.status}/><div className="detail-stats"><span>登録行数 <b>{selected.row_count}</b></span><span>試行回数 <b>{selected.attempts} / 3</b></span><span>版数 <b>{selected.version}</b></span></div>{selected.message&&<p className="message">{selected.message}</p>}{selected.issues.map((e,i)=><div className="issue" key={i}>行 {e.line}：{e.message}</div>)}<h3>操作履歴</h3><div className="timeline">{audits.map((a,i)=><div key={i}><span className="dot"/><section>{actions[a.action]||a.action}<small>{a.actor} · {date(a.created_at)}</small></section></div>)}</div><div className="dialog-actions"><button className="secondary" onClick={()=>void detail(selected)}>最新状態を取得</button>{write&&['queued','running'].includes(selected.status)&&<button className="danger" disabled={busy} onClick={()=>void command(selected,'cancel')}>取込を取消</button>}{manager&&selected.status==='failed'&&<button className="primary" disabled={busy} onClick={()=>void command(selected,'retry')}>再試行</button>}</div></section></div>}
 </div>
}
function CreateDialog({token,open,close,created}:{token:string,open:boolean,close:()=>void,created:()=>void}){
 const[label,setLabel]=useState('商品マスター 追加取込');const[csv,setCsv]=useState(sample);const[failures,setFailures]=useState(0);const[busy,setBusy]=useState(false);const[error,setError]=useState('');const pending=useRef<{key:string,body:string}|null>(null);const mounted=useRef(true);
 useEffect(()=>()=>{mounted.current=false;},[]);
 async function send(e:React.FormEvent){e.preventDefault();setBusy(true);setError('');const record=pending.current||{key:crypto.randomUUID(),body:JSON.stringify({label,csv,simulate_failures:failures})};pending.current=record;try{await api<Job>(token,'/jobs',{method:'POST',headers:{'Idempotency-Key':record.key},body:record.body});if(mounted.current){pending.current=null;created();}}catch(e){if(mounted.current){if(e instanceof ApiError&&e.status>=400&&e.status<500&&e.status!==409){pending.current=null;setError(e.message+' 入力または状態を確認して再送してください。');}else{setError((e instanceof Error?e.message:'送信に失敗しました。')+' 同じ内容・要求キーで再送できます。');}}}finally{if(mounted.current)setBusy(false);}}
 if(!open)return null;
 return <div className="overlay"><section className="dialog wide" role="dialog" aria-modal="true" aria-label="CSV取込を作成"><div className="dialog-heading"><div className="eyebrow">NEW IMPORT</div><button disabled={busy} aria-label="作成画面を閉じる" onClick={close}>×</button></div><h2>CSV取込を作成</h2><p>ファイル内容を検証し、不備がなければ全件を取り込みます。</p><form onSubmit={send}><fieldset disabled={busy||!!pending.current}><label>取込名<input required maxLength={64} value={label} onChange={e=>setLabel(e.target.value)}/></label><label>CSVファイル<input type="file" accept=".csv,text/csv" onChange={async e=>{const f=e.target.files?.[0];if(f){if(f.size>524288){setError('512KiB以内のファイルを選択してください。');return;}try{const data=await f.arrayBuffer();setCsv(new TextDecoder('utf-8',{fatal:true}).decode(data));}catch{setError('UTF-8のCSVを選択してください。');}}}}/></label><div className="csv-label"><label htmlFor="csv">CSV内容</label><button type="button" className="text-button" onClick={()=>{const suffix=Date.now().toString().slice(-7);setCsv(sample.replaceAll('PART-40','P'+suffix+'-'));}}>新規SKUのサンプル</button></div><textarea id="csv" required value={csv} onChange={e=>setCsv(e.target.value)} spellCheck={false}/><label>模擬的な一時障害<select value={failures} onChange={e=>setFailures(Number(e.target.value))}><option value={0}>発生させない</option><option value={1}>1回失敗して自動回復</option><option value={2}>2回失敗して自動回復</option></select></label></fieldset>{error&&<div className="alert" role="alert">{error}</div>}<div className="form-note">不備・既存SKUとの重複があれば、すべての行を取り込みません。</div><div className="dialog-actions"><button type="button" className="secondary" disabled={busy} onClick={close}>閉じる</button><button type="submit" className="primary" disabled={busy}>{busy?'受付中…':pending.current?'同じ内容で再送':'取込を開始 →'}</button></div></form></section></div>
}
createRoot(document.getElementById('root')!).render(<App/>);
