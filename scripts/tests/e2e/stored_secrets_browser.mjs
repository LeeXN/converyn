// Browser UX against the real HTTPS workflow fixture through its isolated mTLS relay.
import fs from 'node:fs';
import path from 'node:path';
const cfg=JSON.parse(fs.readFileSync(process.argv[2],'utf8'));
const person=cfg.person??'Alice',node=cfg.node??'node-a';
const target=await(await fetch(`http://127.0.0.1:${cfg.chrome}/json/new?${encodeURIComponent(cfg.url)}`,{method:'PUT'})).json();
const socket=new WebSocket(target.webSocketDebuggerUrl);await new Promise((ok,no)=>{socket.onopen=ok;socket.onerror=no});
let sequence=0;const pending=new Map(),errors=[];
socket.onmessage=event=>{const value=JSON.parse(event.data);if(value.id){const p=pending.get(value.id);pending.delete(value.id);value.error?p?.reject(Error(value.error.message)):p?.resolve(value.result)}else if(value.method==='Runtime.exceptionThrown')errors.push(value.params.exceptionDetails.text)};
function send(method,params={}){return new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params}));setTimeout(()=>{if(pending.delete(id))reject(Error(method+' timeout'))},15000).unref()})}
async function evaluate(expression){const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw Error(r.exceptionDetails.exception?.description??r.exceptionDetails.text);return r.result.value}
async function wait(expression){for(let i=0;i<300;i++){if(await evaluate('!!('+expression+')'))return;await new Promise(r=>setTimeout(r,100))}throw Error('Timeout: '+expression+' '+await evaluate('document.body?.innerText.slice(-2500)'))}
async function click(text){await evaluate(`Array.from(document.querySelectorAll('button,a')).find(b=>b.textContent.trim()===${JSON.stringify(text)})?.click()`)}
async function tap(selector){const point=await evaluate(`(()=>{const e=document.querySelector(${JSON.stringify(selector)});if(!e)throw Error('Missing control');const r=e.getBoundingClientRect();return{x:e.classList.contains('nav-backdrop')?innerWidth-12:r.x+Math.min(r.width/2,innerWidth-r.x-12),y:r.y+Math.min(r.height/2,200)}})()`);await send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,...point});await send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,...point})}
async function assert(expression,message){if(!await evaluate(expression))throw Error(message)}
await send('Runtime.enable');await send('Page.enable');await send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
await wait(`!!document.querySelector('input[name="token"]')`);

await evaluate(`document.querySelector('input[name="cluster"]').value=${JSON.stringify(cfg.cluster)};document.querySelector('input[name="token"]').value=${JSON.stringify(cfg.token)}`);
await click('连接');await wait(`!!document.querySelector('a[href="#/secrets"]')`);
await click('Secret 管理');await wait(`!!document.querySelector('input[name="file"]')`);
await evaluate(`document.querySelector('input[name="name"]').value='browser-secret';document.querySelector('input[name="version"]').value='v1'`);
await send('DOM.enable');const doc=await send('DOM.getDocument');const input=await send('DOM.querySelector',{nodeId:doc.root.nodeId,selector:'input[name="file"]'});
await send('DOM.setFileInputFiles',{nodeId:input.nodeId,files:[cfg.secret_input]});
await click('保存版本');await wait(`document.body?.innerText.includes('Secret 版本已保存。')&&document.body?.innerText.includes('browser-secret')`);
await assert(`!document.querySelector('input[name="file"]').value`,'private file selection was retained after upload');
const content=fs.readFileSync(cfg.secret_input,'utf8');
await assert(`!document.body.innerText.includes(${JSON.stringify(content)})&&!JSON.stringify(localStorage).includes(${JSON.stringify(content)})&&!JSON.stringify(sessionStorage).includes(${JSON.stringify(content)})`,'Secret content leaked into page or browser storage');
await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
await assert(`document.documentElement.scrollWidth<=innerWidth+2`,'Secret page overflows mobile viewport');
if(errors.length)throw Error(errors.join('; '));
fs.writeFileSync(cfg.report,JSON.stringify({passed:true,upload:true,metadata_only:true,file_cleared:true,mobile:true})+'\n');
socket.close();
