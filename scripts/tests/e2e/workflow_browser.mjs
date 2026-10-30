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
async function click(text){await evaluate(`Array.from(document.querySelectorAll('button,a')).filter(b=>b.checkVisibility()).find(b=>b.textContent.trim()===${JSON.stringify(text)}||(b.matches('.step-trigger')&&b.querySelector('strong')?.textContent===${JSON.stringify(text)}))?.click()`)}
async function openWork(step,tab='执行结果'){if(await evaluate(`!!document.querySelector('dialog[open]')`))await click('关闭');await click(step);await wait(`!!document.querySelector('dialog[open] #work-detail')`);await click(tab)}
async function tap(selector){const point=await evaluate(`(()=>{const e=document.querySelector(${JSON.stringify(selector)});if(!e)throw Error('Missing control');const r=e.getBoundingClientRect();return{x:e.classList.contains('nav-backdrop')?innerWidth-12:r.x+Math.min(r.width/2,innerWidth-r.x-12),y:r.y+Math.min(r.height/2,200)}})()`);await send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',clickCount:1,...point});await send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',clickCount:1,...point})}
async function assert(expression,message){if(!await evaluate(expression))throw Error(message)}
await send('Runtime.enable');await send('Page.enable');await send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
await wait(`!!document.querySelector('input[name="token"]')`);
// Aborted requests leave the active set immediately, before fetch's rejection microtask.
await evaluate(`document.querySelector('input[name="cluster"]').value=${JSON.stringify(cfg.cluster)};document.querySelector('input[name="token"]').value=${JSON.stringify(cfg.token)};window.__writes=[];window.__active={};window.__max={};const originalFetch=fetch;window.fetch=async(url,options)=>{const key=String(url);let counted=!options?.signal?.aborted;const release=()=>{if(counted){counted=false;window.__active[key]--}};if(counted){window.__active[key]=(window.__active[key]||0)+1;window.__max[key]=Math.max(window.__max[key]||0,window.__active[key])}options?.signal?.addEventListener('abort',release,{once:true});if(options?.method==='POST')window.__writes.push({url:key,body:JSON.parse(options.body)});try{return await originalFetch(url,options)}finally{release();options?.signal?.removeEventListener('abort',release)}}`);
if(cfg.inventory_log_fixture)await evaluate(`{const f=fetch;window.__logOffsets=[];window.__cutLog=false;window.fetch=async(url,options)=>{const response=await f(url,options);if(!String(url).includes('/events?offset='))return response;window.__logOffsets.push(Number(new URL(String(url),location.href).searchParams.get('offset')));if(window.__cutLog||!response.ok||!response.body)return response;const reader=response.body.getReader();const decoder=new TextDecoder();let pending='';return new Response(new ReadableStream({async start(controller){try{while(true){const r=await reader.read();if(r.done){controller.close();break}controller.enqueue(r.value);pending+=decoder.decode(r.value,{stream:true});if(pending.split(String.fromCharCode(10,10)).slice(0,-1).some(frame=>/"next_offset":\\s*[1-9]/.test(frame))){window.__cutLog=true;controller.close();void reader.cancel();break}}}catch(e){controller.error(e)}}}),{status:response.status,headers:response.headers})}}`);
await click('连接');await wait(`document.body?.innerText.includes('Demo workflow')`);
await assert(`document.body?.innerText.includes(${JSON.stringify(person)})&&!document.body?.innerText.includes('待审批')`,'named login or approval removal failed');
await click('Demo workflow');await wait(`!!Array.from(document.querySelectorAll('button')).find(b=>b.textContent==='预览执行计划')`);
await click('步骤与脚本');
await assert(`document.querySelectorAll('.workflow-steps .workflow-step').length===2`,'registered steps are hidden before preview');
await click('verify');await wait(`document.querySelector('dialog[open] pre')?.textContent.includes('printf')`);
await assert(`document.querySelector('dialog[open]').getBoundingClientRect().width>innerWidth*.85`,'source reader is constrained by a table cell');
await click('全屏阅读');await assert(`document.querySelector('dialog[open]').getBoundingClientRect().width===innerWidth`,'full screen code reader failed');
const sourceShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(cfg.report+'.workflow-script.png',Buffer.from(sourceShot.data,'base64'));
await click('关闭');await click('执行配置');
await assert(`Array.from(document.querySelectorAll('.run-configuration button')).filter(b=>b.textContent==='恢复默认值').length===1`,'duplicate configuration reset buttons');
await assert(`Array.from(document.querySelectorAll('input')).every(input=>input.closest('.parameter-grid, .workflow-values-editor'))`,'fixed configuration exposed as arbitrary input');
await assert(`document.querySelector('.run-configuration')?.innerText.includes('本次配置')&&Array.from(document.querySelectorAll('.parameter-help')).some(e=>e.innerText.includes('默认值: one'))&&!document.body?.innerText.includes('由运维人员在服务器或仓库维护')`,'run configuration defaults or copy cleanup missing');
if(cfg.inventory_log_fixture){
 async function input(name,value){await evaluate(`{const input=Array.from(document.querySelectorAll('.parameter-grid label')).find(l=>l.firstChild.textContent===${JSON.stringify(name)}).querySelector('input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(input,${JSON.stringify(value)});input.dispatchEvent(new Event('input',{bubbles:true}))}`)}
 const writes=await evaluate('window.__writes.length');
 for(const value of ['1.5','0','4']){
  await input('RETRIES',value);await click('预览执行计划');await wait(`!!document.querySelector('[role="alert"]')`);
  await assert(`window.__writes.length===${writes}&&!document.body?.innerText.includes('执行已预览计划')`,'invalid integer or range reached the API');
 }
 await input('RETRIES','3');await input('NOTE','中文中');await click('预览执行计划');await wait(`document.body?.innerText.includes('参数超过最大长度')`);
 await assert(`window.__writes.length===${writes}`,'UTF-8 byte length was not checked before preview');
 await input('NOTE','中文');
 await evaluate(`{const select=Array.from(document.querySelectorAll('.parameter-grid label')).find(l=>l.firstChild.textContent==='ENABLED').querySelector('select');select.value='true';select.dispatchEvent(new Event('change',{bubbles:true}))}`);
 await click('预览执行计划');await wait(`document.body?.innerText.includes('执行已预览计划')`);
 await assert(`window.__writes.at(-1).body.parameters.RETRIES===3&&window.__writes.at(-1).body.parameters.NOTE==='中文'&&window.__writes.at(-1).body.parameters.ENABLED===true`,'parameter types or UTF-8 boundary changed');
 await click('恢复默认值');await wait(`!document.body?.innerText.includes('执行已预览计划')`);
}
await click('预览执行计划');await wait(`document.body?.innerText.includes('执行已预览计划')`);
await assert(`window.__writes.at(-1).body.parameters.VERSION==='one'`,'default parameter missing');
await assert(`document.querySelector('.workflow-values-editor input[aria-label="REGION"]').disabled`,'immutable workflow value is editable');
for(const port of ['90','80'])await evaluate(`{const field=document.querySelector('.workflow-values-editor input[aria-label="PORT"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,${JSON.stringify(port)});field.dispatchEvent(new Event('input',{bubbles:true}))}`);
await click('预览执行计划');await wait(`document.body?.innerText.includes('执行已预览计划')`);
await assert(`window.__writes.at(-1).body.parameters.PORT===80`,'explicit workflow default did not override node default');
await evaluate(`document.querySelector('.run-configuration > header .reset-configuration').click()`);

await click('恢复默认值');await wait(`!document.body?.innerText.includes('执行已预览计划')`);
await evaluate(`const select=Array.from(document.querySelectorAll('label')).find(l=>l.textContent.startsWith('VERSION')).querySelector('select');const set=Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set;set.call(select,'two');select.dispatchEvent(new Event('change',{bubbles:true}))`);
await wait(`!document.body?.innerText.includes('执行已预览计划')`);
if(cfg.inventory_log_fixture){
 await evaluate(`{const e=document.querySelector('.configuration-editor');e.open=true;const input=e.querySelector('textarea');Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set.call(input,'temporary browser configuration\\n');input.dispatchEvent(new Event('input',{bubbles:true}))}`);
 await wait(`document.body?.innerText.includes('已临时修改')`);
}
await assert(`document.querySelectorAll('.workflow-values-editor textarea').length===0`,'workflow values require raw JSON');
await evaluate(`{const editor=document.querySelector('.workflow-values-editor');const field=editor.querySelector('input[name="variable-name"]');const setter=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;setter.call(field,'BROWSER_VARIABLE');field.dispatchEvent(new Event('input',{bubbles:true}))}`);
await evaluate(`{const editor=document.querySelector('.workflow-values-editor');const field=Array.from(editor.querySelectorAll('label')).find(l=>l.textContent.startsWith('本次值')).querySelector('input');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(field,'from-gui');field.dispatchEvent(new Event('input',{bubbles:true}))}`);
await evaluate(`document.querySelector('.variable-composer input[name="variable-value"]').focus()`);
await send('Input.dispatchKeyEvent',{type:'keyDown',key:'Enter',code:'Enter',text:'\r',unmodifiedText:'\r',windowsVirtualKeyCode:13});
await send('Input.dispatchKeyEvent',{type:'keyUp',key:'Enter',code:'Enter',windowsVirtualKeyCode:13});
await wait(`document.querySelector('.workflow-values-editor').innerText.includes('BROWSER_VARIABLE')`);
await assert(`document.activeElement?.name==='variable-name'&&!document.querySelector('input[name="variable-name"]').value`,'keyboard add did not reset and focus the composer');
await click('预览执行计划');await wait(`document.body?.innerText.includes('执行已预览计划')`);
await assert(`window.__writes.at(-1).body.parameters.BROWSER_VARIABLE==='from-gui'&&!('PORT' in window.__writes.at(-1).body.parameters)`,'new variable or untouched node value lost');
await assert(`window.__writes.at(-1).body.parameters.VERSION==='two'`,'typed override missing');
if(cfg.inventory_log_fixture)await assert(`window.__writes.at(-1).body.parameters.CONFIG_CONTENT==='temporary browser configuration\\n'`,'temporary file content missing from preview');
await click('查看执行脚本（含变量）');await wait(`document.querySelector('dialog[open] pre')?.textContent.includes('two')`);
await assert(`document.querySelector('dialog[open]').getBoundingClientRect().width>innerWidth*.85`,'rendered execution script is still constrained by the preview table');
await click('关闭');
await click('执行已预览计划');await wait(`location.hash.startsWith('#/runs/run-')&&document.body?.innerText.includes(${JSON.stringify('操作人员：'+person)})`);
await wait(`!!document.querySelector('.work-table')`);
await assert(`!document.querySelector('dialog[open]')&&!document.querySelector('pre[aria-live]')&&document.querySelector('.run-timing').innerText.includes('总用时')`,'task opened a reader or log stream without user selection');
await openWork('write');
if(cfg.inventory_log_fixture){const before=await evaluate(`document.querySelector('.run-timing .elapsed').textContent`);await wait(`document.querySelector('.run-timing .elapsed').textContent!==${JSON.stringify(before)}&&document.querySelector('.run-timing .elapsed').textContent.includes('进行中')`)}
await assert(`document.querySelector('#work-detail').innerText.includes('路径')`,'file operation details missing');
if(cfg.inventory_log_fixture){await openWork('verify','日志');await wait(`document.body?.innerText.includes('line-0000-中文-log')&&document.body?.innerText.includes('实时片段')`)}
await wait(`document.querySelector('.run-heading > .badge')?.textContent.includes('已完成')`);
const run=await evaluate(`decodeURIComponent(location.hash.split('?')[0].split('/').at(-1))`);
const finishedElapsed=await evaluate(`document.querySelector('.run-timing .elapsed').textContent`);await new Promise(r=>setTimeout(r,1100));await assert(`document.querySelector('.run-timing .elapsed').textContent===${JSON.stringify(finishedElapsed)}&&!document.querySelector('.run-timing .elapsed').textContent.includes('进行中')`,'completed elapsed time did not freeze');
if(!cfg.inventory_log_fixture)await openWork('verify','日志');await wait(`document.body?.innerText.includes('version=two')`);
if(cfg.inventory_log_fixture){
 await wait(`window.__cutLog&&window.__logOffsets.some(v=>v>0)`);
 await wait(`document.body?.innerText.includes('已保存日志上传完成')`);
 await assert(`window.__cutLog&&window.__logOffsets.some(v=>v>0)`,'live log did not reconnect from its accepted cursor: '+JSON.stringify(await evaluate('({cut:window.__cutLog,offsets:window.__logOffsets})')));
 await assert(`document.querySelector('pre[aria-live]').textContent.split('line-0000-中文-log').length===2`,'reconnect duplicated log output');
 const downloads=fs.mkdtempSync(cfg.report+'.downloads-');await send('Browser.setDownloadBehavior',{behavior:'allow',downloadPath:path.resolve(downloads)});await click('下载完整已保存日志');
 let downloaded;for(let i=0;i<100;i++){downloaded=fs.readdirSync(downloads).find(n=>n.endsWith('.log'));if(downloaded)break;await new Promise(r=>setTimeout(r,100))}
 if(!downloaded)throw Error('GUI full log download missing: '+await evaluate('document.body?.innerText.slice(-1000)'));
 const saved=fs.readFileSync(path.join(downloads,downloaded),'utf8');if(!saved.includes('line-0000-中文-log')||!saved.includes('line-0599-中文-log')||Buffer.byteLength(saved)<=4096)throw Error('GUI downloaded an incomplete log');
}

await assert(`document.querySelector('.work-table tr.selected button')?.textContent==='verify'&&document.querySelector('dialog[open]').contains(document.activeElement)`,'step selection or modal focus feedback missing');
await click('关闭');
await evaluate(`Array.from(document.querySelectorAll('details')).find(e=>e.querySelector(':scope > summary')?.textContent==='实际参数快照').open=true`);
await assert(`document.querySelector('.panel').innerText.includes('实际参数快照')&&document.querySelector('.panel').innerText.includes('two')`,'actual parameter snapshot missing');
await evaluate(`Array.from(document.querySelectorAll('details')).find(e=>e.querySelector(':scope > summary')?.textContent==='实际参数快照').open=false`);
await openWork('verify');
await assert(`document.querySelector('#work-detail').innerText.includes('超时')&&document.querySelector('#work-detail').innerText.includes('退出码')`,'execution content or result missing');
await evaluate(`{window.__copiedId='';Object.defineProperty(navigator,'clipboard',{configurable:true,value:{writeText:async(value)=>{window.__copiedId=value}}})}`);
await click('复制工作项编号');await wait(`document.querySelector('.work-id [role="status"]').textContent==='已复制'`);
await assert(`window.__copiedId===document.querySelector('.work-id code').textContent`,'copy used the wrong work ID');
await evaluate(`navigator.clipboard.writeText=async()=>{throw Error('fixture clipboard unavailable')}`);
await click('复制工作项编号');await wait(`document.querySelector('.work-id [role="status"]').textContent.includes('复制失败')`);
await click('日志');
async function logStream(value){await evaluate(`{const select=document.querySelector('.log-controls select');select.value=${JSON.stringify(value)};select.dispatchEvent(new Event('change',{bubbles:true}))}`)}
await logStream('stderr');await wait(`document.querySelector('pre[aria-live]')?.textContent==='此日志流没有输出'`);
await evaluate(`{const f=fetch;window.__logUnavailable=true;window.fetch=(url,options)=>window.__logUnavailable&&String(url).includes('/events?offset=')?Promise.resolve(new Response(JSON.stringify({code:'fixture_log_unavailable'}),{status:503})):f(url,options)}`);
await logStream('stdout');await wait(`document.body?.innerText.includes('集中存储不可用：原回执尾部，非完整输出')&&document.querySelector('pre[aria-live]')?.textContent.includes('version=two')`);
await evaluate(`window.__logUnavailable=false`);
await logStream('stderr');await wait(`document.querySelector('pre[aria-live]')?.textContent==='此日志流没有输出'`);
await logStream('stdout');await wait(`document.querySelector('pre[aria-live]')?.textContent.includes('version=two')&&!document.querySelector('.log-controls').parentElement.querySelector('[role="alert"]')`);
// A structured, non-retryable SSE failure must not become an endless 503 loop.
await evaluate(`{const f=fetch;window.__fatalLog=true;window.__fatalRequests=0;window.fetch=(url,options)=>{if(window.__fatalLog&&String(url).includes('/events?offset=')){window.__fatalRequests++;return Promise.resolve(new Response('event: error'+String.fromCharCode(10)+'data: '+JSON.stringify({status:503,code:'log_chunk_checksum_mismatch',message:'corrupt fixture',retryable:false,request_id:'sse-fixture'})+String.fromCharCode(10,10),{headers:{'content-type':'text/event-stream'}}))}return f(url,options)}}`);
await logStream('stderr');await wait(`document.querySelector('dialog [role="alert"]')?.textContent.includes('日志校验失败')`);
const fatalRequests=await evaluate('window.__fatalRequests');await new Promise(r=>setTimeout(r,1300));
await assert(`window.__fatalRequests===${fatalRequests}&&document.querySelector('dialog [role="alert"]').textContent.includes('sse-fixture')`,'fatal SSE errors retried or lost correlation');
await evaluate(`window.__fatalLog=false`);await click('重新连接');await wait(`document.querySelector('pre[aria-live]')?.textContent==='此日志流没有输出'`);
// Appending output scrolls only when the reader explicitly enables following.
await click('执行结果');
await evaluate(`{const f=fetch;window.__scrollFixture=true;window.fetch=(url,options)=>{if(window.__scrollFixture&&String(url).includes('/events?offset=')){let offset=0;return Promise.resolve(new Response(new ReadableStream({start(c){window.__pushLog=(text,complete=false)=>{const n=offset+text.length;c.enqueue(new TextEncoder().encode('event: log'+String.fromCharCode(10)+'data: '+JSON.stringify({data:btoa(text),offset,next_offset:n,complete,expired:false,truncated:false,source:'live_fragment'})+String.fromCharCode(10,10)));offset=n;if(complete)c.close()};window.__pushLog(Array.from({length:200},(_,i)=>'scroll-'+i).join(String.fromCharCode(10)));options.signal.addEventListener('abort',()=>{try{c.close()}catch{}},{once:true})}}),{headers:{'content-type':'text/event-stream'}}))}return f(url,options)}}`);
await click('日志');await wait(`document.querySelector('.log-output')?.textContent.includes('scroll-199')`);
await evaluate(`document.querySelector('.log-output').scrollTop=180`);await new Promise(r=>setTimeout(r,80));
const readingTop=await evaluate(`document.querySelector('.log-output').scrollTop`), readingPage=await evaluate('window.scrollY');
await evaluate(`window.__pushLog('append-hold')`);await wait(`document.querySelector('.log-output').textContent.includes('append-hold')`);
await assert(`Math.abs(document.querySelector('.log-output').scrollTop-${readingTop})<2&&window.scrollY===${readingPage}`,'new logs moved the reader or page without consent');
await evaluate(`document.querySelector('.log-controls input[type="checkbox"]').click()`);
await evaluate(`window.__pushLog('append-follow')`);await wait(`document.querySelector('.log-output').textContent.includes('append-follow')`);
await assert(`(()=>{const p=document.querySelector('.log-output');return p.scrollTop+p.clientHeight>=p.scrollHeight-2})()`,'follow mode did not scroll its log container');
await evaluate(`document.querySelector('.log-output').scrollTop=0`);await wait(`!document.querySelector('.log-controls input[type="checkbox"]').checked`);
await evaluate(`window.__scrollFixture=false;window.__pushLog('',true)`);
await openWork('write');await wait(`document.querySelector('#work-detail').innerText.includes('发生变更')`);
await assert(`document.querySelector('#work-detail').innerText.includes('执行结果')&&!document.querySelector('pre[aria-live]')&&Array.from(document.querySelectorAll('.record-details')).every(d=>!d.open)`,'native evidence or collapsed receipt missing');
await openWork('verify','日志');await wait(`document.querySelector('pre[aria-live]')?.textContent.includes('version=two')`);
await evaluate(`scrollTo(0,0)`);
await click('关闭');
await evaluate(`{const f=fetch;window.__longRun=true;window.fetch=async(url,options)=>{const r=await f(url,options);if(window.__longRun&&String(url).split('?')[0].endsWith('/runs/'+${JSON.stringify(run)})&&r.ok){const v=await r.json();v.plan.work=[...Array.from({length:80},(_,i)=>({...v.plan.work[0],id:'display-'+i,step_id:'display-'+i})),...v.plan.work];return new Response(JSON.stringify(v),{status:200,headers:r.headers})}return r}}`);
await wait(`document.querySelectorAll('.work-table tbody tr').length>80`);
await evaluate(`Array.from(document.querySelectorAll('.work-table td:first-child button')).find(b=>b.textContent==='verify').scrollIntoView({block:'center'})`);
const originScroll=await evaluate('window.scrollY');
await tap('.work-table tbody tr:last-child td:first-child button');await wait(`!!document.querySelector('dialog[open]')`);
await click('日志');await wait(`document.querySelector('pre[aria-live]')?.textContent.includes('version=two')`);
await click('关闭');
await assert(`Math.abs(window.scrollY-${originScroll})<2&&document.activeElement?.textContent==='verify'`,'closing the reader lost the list scroll position or row focus');
await evaluate(`window.__longRun=false`);await wait(`document.querySelectorAll('.work-table tbody tr').length===2`);
await openWork('verify','日志');await wait(`document.querySelector('pre[aria-live]')?.textContent.includes('version=two')`);
const detailShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.details.png',Buffer.from(detailShot.data,'base64'));
await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
await assert(`document.documentElement.scrollWidth<=innerWidth`,'task details overflow the mobile viewport');
const mobileDetailShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.details-mobile.png',Buffer.from(mobileDetailShot.data,'base64'));
await send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});

// Exercise the empty configuration shown in the UI report, using actual controls.
const valuesRoute=await evaluate('location.hash');
await click('关闭');
await evaluate(`location.hash='#/workflows/zz-empty-values?tab=execute'`);await wait(`!!document.querySelector('.variable-composer')`);
await assert(`!document.querySelector('.workflow-values-list table')&&Array.from(document.querySelectorAll('.run-configuration button')).filter(b=>b.textContent==='恢复默认值').length===1`,'empty variable table or duplicate reset remains');
const emptyShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.parameters-empty.png',Buffer.from(emptyShot.data,'base64'));
async function variableField(name,value){await evaluate(`{const f=document.querySelector('.variable-composer [name='+${JSON.stringify(name)}+']');Object.getOwnPropertyDescriptor(f.tagName==='SELECT'?HTMLSelectElement.prototype:HTMLInputElement.prototype,'value').set.call(f,${JSON.stringify(value)});f.dispatchEvent(new Event(f.tagName==='SELECT'?'change':'input',{bubbles:true}))}`)}
await variableField('variable-name','COUNT');await variableField('variable-type','number');await click('新增变量');
await wait(`document.querySelector('.variable-error')?.textContent==='请输入有效数字'`);
await assert(`document.activeElement?.name==='variable-value'&&!document.querySelector('.workflow-values-list table')`,'invalid number was added or error focus is wrong');
await variableField('variable-value','12.5');await click('新增变量');
await wait(`!!document.querySelector('input[aria-label="COUNT"]')`);
await variableField('variable-name','COUNT');await variableField('variable-value','2');await click('新增变量');
await wait(`document.querySelector('.variable-error')?.textContent==='变量已存在'`);
await assert(`document.querySelectorAll('.workflow-values-list tbody tr').length===1&&document.activeElement?.name==='variable-name'`,'duplicate variable was added');
await variableField('variable-name','ENABLED_EXTRA');await variableField('variable-type','boolean');await variableField('variable-value','true');await click('新增变量');
await click('预览执行计划');await wait(`document.body?.innerText.includes('执行已预览计划')`);
await assert(`window.__writes.at(-1).body.parameters.COUNT===12.5&&window.__writes.at(-1).body.parameters.ENABLED_EXTRA===true`,'composer lost number or boolean types');
const valuesShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.parameters.png',Buffer.from(valuesShot.data,'base64'));
await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
await assert(`document.documentElement.scrollWidth<=innerWidth&&document.querySelector('.variable-add').getBoundingClientRect().top>=document.querySelector('.variable-composer-value').getBoundingClientRect().bottom`,'parameter form does not stack on mobile');
const valuesMobileShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.parameters-mobile.png',Buffer.from(valuesMobileShot.data,'base64'));
await send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
await click('恢复默认值');await wait(`!document.querySelector('.workflow-values-list table')&&!document.body?.innerText.includes('执行已预览计划')`);
await assert(`document.querySelector('[name="variable-name"]').value===''&&document.querySelector('[name="variable-value"]').value===''&&document.querySelector('[name="variable-type"]').value==='string'`,'unified reset did not clear the composer');
await evaluate(`location.hash=${JSON.stringify(valuesRoute)}`);await wait(`!!document.querySelector('#work-detail')`);

await assert(`window.__writes.filter(w=>w.url.endsWith('/runs')).every(w=>!('plan' in w.body)&&!('actor' in w.body)&&!('script' in w.body))`,'browser uploaded executable content');
await assert(`Object.values(window.__max).every(n=>n<=1)`,'overlapping live requests: '+JSON.stringify(await evaluate('window.__max')));
await evaluate(`location.hash='#/audit'`);await wait(`!!document.querySelector('.table-wrap')`);
await assert(`document.querySelector('button[role="tab"][aria-selected="true"]')?.textContent==='业务操作与失败'`,'audit did not default to actions and failures');
await click('节点观测');await wait(`document.querySelector('.page-content tbody')?.innerText.includes('节点状态上报')`);
await click('全部记录');await wait(`document.querySelector('button[role="tab"][aria-selected="true"]')?.textContent==='全部记录'&&!!document.querySelector('tbody tr')`);
await click('业务操作与失败');await wait(`document.querySelector('button[role="tab"][aria-selected="true"]')?.textContent==='业务操作与失败'&&!!document.querySelector('tbody tr')`);

// Persistent labs have earlier audit pages full of heartbeats; follow real pagination.
let auditFound=false;
for(let page=0;page<100;page++){
  if(await evaluate(`document.body?.innerText.includes('执行流程')&&document.body?.innerText.includes(${JSON.stringify(person)})`)){auditFound=true;break}
  const canNext=await evaluate(`Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='下一页'&&!b.disabled)`);
  if(!canNext)break;
  const previous=await evaluate(`document.querySelector('.table-wrap').innerText`);
  await click('下一页');await wait(`!!document.querySelector('.table-wrap')&&document.querySelector('.table-wrap').innerText!==${JSON.stringify(previous)}&&Array.from(document.querySelectorAll('button')).some(b=>b.textContent==='上一页'&&!b.disabled)`);
}
if(!auditFound)throw Error('Named workflow audit entry missing across audit pages');
await evaluate(`{const f=fetch;window.fetch=async(url,options)=>{const r=await f(url,options);if(String(url).includes('/api/v1/nodes')&&r.ok){const v=await r.json();const n=v.nodes?.[${JSON.stringify(node)}];if(n?.inventory){n.inventory.data.addresses=[{interface:'lo',address:'127.0.0.1'},{interface:'ens192',address:'192.0.2.10'},{interface:'ens192',address:'2001:db8::1'},{interface:'cni0',address:'10.244.0.1'}];if(n.resources){n.resources.collected_ms=1;n.resources.truncated=true}}return new Response(JSON.stringify(v),{status:200,headers:r.headers})}return r}}`);
await evaluate(`location.hash='#/nodes'`);await wait(`document.body?.innerText.includes(${JSON.stringify(node)})`);
await assert(`document.querySelector('.node-addresses > code')?.textContent==='192.0.2.10'&&!document.querySelector('.node-addresses details').open`,'other IPs are not collapsed or the host address is not preferred');
await assert(`document.querySelector('.page-content').innerText.includes('资源信息已过期')&&(document.querySelector('.page-content').innerText.includes('部分采集明细已省略')||document.querySelector('.page-content').innerText.includes('列表不完整'))`,'staleness hides truncation');
await evaluate(`location.hash='#/nodes/'+encodeURIComponent(${JSON.stringify(node)})`);await wait(`document.body?.innerText.includes('主机资产')&&document.body?.innerText.includes('资源概况')`);
await assert(`document.body?.innerText.includes('整机序列号')&&document.body?.innerText.includes('最近检查')`,'node grouped detail missing');
// Delay one real detail response, then navigate away. It must never populate the next node.
await evaluate(`{const f=fetch;window.fetch=async(url,options)=>{const response=await f(url,options);if(String(url).includes('/nodes/'+${JSON.stringify(node)})){await new Promise(r=>setTimeout(r,700));if(options?.signal?.aborted)throw new DOMException('Aborted','AbortError')}return response};location.hash='#/nodes';}`);
await wait(`!!document.querySelector('h1')&&document.querySelector('h1').textContent==='节点'`);
await evaluate(`location.hash='#/nodes/'+encodeURIComponent(${JSON.stringify(node)})`);await new Promise(r=>setTimeout(r,80));
await evaluate(`location.hash='#/nodes/switch-missing'`);await wait(`document.querySelector('h1')?.textContent==='switch-missing'`);await new Promise(r=>setTimeout(r,900));
await assert(`!document.body?.innerText.includes('主机资产')&&!document.body?.innerText.includes('整机序列号')`,'late node details leaked across navigation');
await evaluate(`location.hash='#/nodes/'+encodeURIComponent(${JSON.stringify(node)})`);await wait(`document.body?.innerText.includes('主机资产')`);

await evaluate(`location.hash='#/targets'`);await wait(`document.querySelector('h1')?.textContent==='外部目标'&&(document.querySelector('.empty')||document.querySelector('.target-grid'))`);
await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
await tap('button.menu-toggle');await wait(`document.querySelector('button.menu-toggle').getAttribute('aria-expanded')==='true'&&document.querySelector('.sidebar').getBoundingClientRect().left===0`);
await tap('.nav-backdrop');await wait(`document.querySelector('button.menu-toggle').getAttribute('aria-expanded')==='false'`);
await tap('button.menu-toggle');await send('Input.dispatchKeyEvent',{type:'keyDown',key:'Escape',code:'Escape'});await wait(`document.querySelector('button.menu-toggle').getAttribute('aria-expanded')==='false'`);
await tap('button.menu-toggle');await tap('nav a[href="#/runs"]');await wait(`!document.querySelector('.layout').classList.contains('nav-open')`);
await assert(`document.documentElement.scrollWidth<=innerWidth+2`,'mobile horizontal overflow');
await evaluate(`location.hash='#/runs'`);await wait(`!!Array.from(document.querySelectorAll('tbody button')).find(b=>b.title===${JSON.stringify(run)})`);
await assert(`document.querySelector('th[aria-sort="descending"]')?.textContent.includes('执行日期')&&Array.from(document.querySelectorAll('th')).some(th=>th.textContent==='结束日期')`,'run date columns or descending indicator missing');
await assert(`document.querySelector('tbody tr button')?.title===${JSON.stringify(run)}&&document.querySelectorAll('tbody tr:first-child .date-cell').length===2&&Array.from(document.querySelectorAll('tbody tr:first-child .date-cell')).every(td=>td.textContent!=='—')`,'newest run or persisted dates missing from the first row');
await send('Emulation.setDeviceMetricsOverride',{width:1600,height:1000,deviceScaleFactor:1,mobile:false});
const datesScreenshot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(cfg.report+'.runs.png',Buffer.from(datesScreenshot.data,'base64'));
await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:1,mobile:true});
fs.mkdirSync(path.dirname(cfg.report),{recursive:true});
const screenshot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(cfg.report+'.png',Buffer.from(screenshot.data,'base64'));
// Refresh keeps the route and verifies identity before any business read.
await assert(`sessionStorage.getItem('converyn.session')!==null&&!Object.values(localStorage).some(v=>v.includes(${JSON.stringify(cfg.token)}))`,'tab session or token storage scope is incorrect');
await evaluate(`location.hash='#/nodes/'+encodeURIComponent(${JSON.stringify(node)})`);await wait(`document.body?.innerText.includes('主机资产')`);
const route=await evaluate('location.hash');
await send('Page.addScriptToEvaluateOnNewDocument',{source:`window.__restoreReads=[];window.__authFault=0;const original=fetch;window.fetch=async(url,options)=>{window.__restoreReads.push(String(url));if(String(url).endsWith('/identity/me')&&window.__authFault)return new Response(JSON.stringify({code:'fixture_auth',message:'fixture',retryable:true}),{status:window.__authFault,headers:{'Content-Type':'application/json'}});return original(url,options)}`});
await send('Page.reload');await wait(`document.body?.innerText.includes('主机资产')`);
await assert(`location.hash===${JSON.stringify(route)}&&!document.querySelector('input[name="token"]')&&window.__restoreReads[0].endsWith('/identity/me')`,'refresh did not restore and validate the same route');
await evaluate(`{const select=document.querySelector('.language-switch');select.value='en';select.dispatchEvent(new Event('change',{bubbles:true}))}`);
await wait(`document.documentElement.lang==='en'&&document.body?.innerText.includes('Host inventory')`);
await assert(`!/[\u3400-\u9fff]/.test(document.querySelector('.page-content').innerText)`,'untranslated node interface text');
await evaluate(`location.hash='#/audit'`);await wait(`document.querySelector('h1')?.textContent==='Audit'&&document.querySelector('tbody tr')`);
await assert(`!/[\u3400-\u9fff]/.test(document.querySelector('.page-content').innerText)`,'untranslated audit interface text');
await send('Page.reload');await wait(`document.querySelector('h1')?.textContent==='Audit'&&document.querySelector('tbody tr')`);
await assert(`document.documentElement.lang==='en'`,'language preference did not survive refresh');
await evaluate(`{const select=document.querySelector('.language-switch');select.value='zh-CN';select.dispatchEvent(new Event('change',{bubbles:true}))}`);
await wait(`document.querySelector('h1')?.textContent==='审计'`);
await click('退出');await wait(`!!document.querySelector('input[name="token"]')`);
await assert(`!sessionStorage.getItem('converyn.session')`,'logout retained credentials');
await send('Page.reload');await wait(`!!document.querySelector('input[name="token"]')`);
if(cfg.admin_token){
 await evaluate(`document.querySelector('input[name="cluster"]').value=${JSON.stringify(cfg.cluster)};document.querySelector('input[name="token"]').value=${JSON.stringify(cfg.admin_token)}`);
 await click('连接');await wait(`!!document.querySelector('nav a[href="#/controlplane"]')`);
 await evaluate(`location.hash='#/controlplane'`);await wait(`document.body?.innerText.includes('控制面成员')&&document.querySelector('tbody tr')`);
 await assert(`!document.querySelector('.page-content pre')&&document.body?.innerText.includes('投票成员')`,'control plane lacks structured member information');
 await send('Emulation.setDeviceMetricsOverride',{width:1440,height:1000,deviceScaleFactor:1,mobile:false});
 await assert(`document.querySelector('.topbar').getBoundingClientRect().width===innerWidth-document.querySelector('.sidebar').getBoundingClientRect().width&&getComputedStyle(document.querySelector('.menu-toggle')).display==='none'`,'desktop header layout is incorrect');
 await evaluate(`location.hash='#/access'`);await wait(`document.querySelector('h1')?.textContent==='访问管理'&&document.body?.innerText.includes('现有授权')`);
 await assert(`document.querySelectorAll('.management-form').length>=2&&document.body?.innerText.includes('流程执行者')`,'access management lacks clear role grants');
 // Exercise a complete group/grant/revoke cycle through the actual management forms.
 await evaluate(`{window.confirm=()=>true;const form=document.querySelectorAll('.management-layout .panel')[1].querySelector('form');form.querySelector('[name="name"]').value='Browser operators';const members=form.querySelector('[name="members"]');for(const option of members.options)option.selected=option.textContent==='Bob';form.requestSubmit()}`);
 await wait(`Array.from(document.querySelectorAll('.management-row strong')).some(e=>e.textContent==='Browser operators')`);
 await evaluate(`{const form=document.querySelectorAll('.management-layout .panel')[2].querySelector('form');const select=form.querySelector('select');select.value=Array.from(select.options).find(o=>o.textContent==='Browser operators').value;select.dispatchEvent(new Event('change',{bubbles:true}))}`);
 await evaluate(`{const form=document.querySelectorAll('.management-layout .panel')[2].querySelector('form');const select=form.querySelectorAll('select')[1];select.value='viewer';select.dispatchEvent(new Event('change',{bubbles:true}))}`);
 await evaluate(`{const form=document.querySelectorAll('.management-layout .panel')[2].querySelector('form');const select=form.querySelectorAll('select')[2];select.value='demo';select.dispatchEvent(new Event('change',{bubbles:true}))}`);
 await wait(`!!document.querySelector('select[name="presets"]')`);
 await evaluate(`{const form=document.querySelectorAll('.management-layout .panel')[2].querySelector('form');for(const option of form.querySelector('select[name="presets"]').options)option.selected=option.value==='qa';form.requestSubmit()}`);
 await wait(`Array.from(document.querySelectorAll('.management-layout .panel:last-child tbody tr')).some(r=>r.innerText.includes('Browser operators'))`);
 await evaluate(`Array.from(document.querySelectorAll('.management-layout .panel:last-child tbody tr')).find(r=>r.innerText.includes('Browser operators')).querySelector('button').click()`);
 await wait(`!Array.from(document.querySelectorAll('.management-layout .panel:last-child tbody tr')).some(r=>r.innerText.includes('Browser operators'))`);
 await evaluate(`Array.from(document.querySelectorAll('.management-row')).find(r=>r.querySelector('strong')?.textContent==='Browser operators').querySelectorAll('button')[1].click()`);
 await wait(`!Array.from(document.querySelectorAll('.management-row strong')).some(e=>e.textContent==='Browser operators')`);
 const accessShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.access.png',Buffer.from(accessShot.data,'base64'));
 await evaluate(`location.hash='#/infrastructure'`);await wait(`document.querySelector('h1')?.textContent==='节点组管理'&&!!document.querySelector('.group-grid')`);
 await click('创建节点组');await wait(`!!document.querySelector('dialog form.group-editor')`);
 await evaluate(`{const form=document.querySelector('form.group-editor');form.querySelector('input[name="uid"]').value='browser-scope';form.querySelector('input[name="name"]').value='Browser scope';form.requestSubmit()}`);
 await wait(`!document.querySelector('dialog')&&Array.from(document.querySelectorAll('section.panel h2')).some(e=>e.textContent.includes('Browser scope'))`);
 await evaluate(`Array.from(document.querySelectorAll('.group-card')).find(e=>e.querySelector('h2')?.textContent.includes('Browser scope')).querySelector('button').click()`);
 await wait(`document.querySelector('input[name="uid"]')?.readOnly===true`);
 await evaluate(`{const form=document.querySelector('form.group-editor');form.querySelector('input[name="name"]').value='Browser scope updated';form.querySelector('input[type="checkbox"][aria-label="node-a"]').click()}`);
 const editorShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(cfg.report+'.group-editor.png',Buffer.from(editorShot.data,'base64'));
 await evaluate(`{const f=fetch;window.__rejectGroupUpdate=true;window.fetch=(url,options)=>window.__rejectGroupUpdate&&String(url).endsWith('/node-groups/commands')&&JSON.parse(options?.body||'{}').action==='update_node_group'?Promise.resolve(new Response(JSON.stringify({code:'node_group_revision_conflict',message:'fixture conflict',retryable:false}),{status:409})):f(url,options)}`);
 await click('保存');await wait(`document.querySelector('dialog [role="alert"]')?.textContent.includes('节点组已更新')`);
 await assert(`document.querySelector('dialog input[name="name"]').value==='Browser scope updated'&&document.querySelector('dialog input[aria-label="node-a"]').checked`,'save conflict lost the draft');
 await evaluate(`window.__rejectGroupUpdate=false`);await click('保存');
 await wait(`!document.querySelector('dialog')&&Array.from(document.querySelectorAll('.group-card')).some(e=>e.innerText.includes('Browser scope updated')&&e.innerText.includes('node-a'))`);
 // A rejected deletion stays beside the action, retaining the entered reason.
 await evaluate(`{const f=fetch;window.__rejectGroupDelete=true;window.fetch=(url,options)=>window.__rejectGroupDelete&&String(url).endsWith('/node-groups/commands')&&JSON.parse(options?.body||'{}').action==='delete_node_group'?Promise.resolve(new Response(JSON.stringify({code:'node_group_referenced_by_workflow',message:'fixture reference',retryable:false,request_id:'delete-fixture'}),{status:409})):f(url,options)}`);
 await evaluate(`Array.from(document.querySelectorAll('.group-card')).find(e=>e.querySelector('h2')?.textContent.includes('Browser scope updated')).querySelectorAll('button')[1].click()`);
 await evaluate(`{const form=document.querySelector('dialog form');form.querySelector('textarea').value='Browser acceptance complete';form.requestSubmit()}`);
 await wait(`document.querySelector('dialog [role="alert"]')?.textContent.includes('当前流程仍引用该节点组')`);
 await assert(`document.querySelector('dialog textarea').value==='Browser acceptance complete'`,'failed deletion discarded its reason');
 const deleteErrorShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(cfg.report+'.delete-error.png',Buffer.from(deleteErrorShot.data,'base64'));
 await evaluate(`window.__rejectGroupDelete=false`);await click('确认删除');
 await wait(`!document.querySelector('dialog')&&!Array.from(document.querySelectorAll('.group-card h2')).some(e=>e.textContent.includes('Browser scope updated'))`);
 await click('节点登记');await wait(`document.body?.innerText.includes('节点登记与成员关系')`);
 await assert(`document.querySelector('.page-content').innerText.includes('撤销节点接入资格')`,'retirement semantics are unexplained');
 const infraShot=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});fs.writeFileSync(cfg.report+'.infrastructure.png',Buffer.from(infraShot.data,'base64'));
 await evaluate(`location.hash='#/controlplane'`);await wait(`document.body?.innerText.includes('控制面成员')`);
 const desktop=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:false});fs.writeFileSync(cfg.report+'.controlplane.png',Buffer.from(desktop.data,'base64'));
 await evaluate(`{const select=document.querySelector('.language-switch');select.value='en';select.dispatchEvent(new Event('change',{bubbles:true}))}`);
 await wait(`document.body?.innerText.includes('Control plane members')`);
 await assert(`!/[\u3400-\u9fff]/.test(document.querySelector('.page-content').innerText)`,'untranslated control plane interface text');
 // A temporary failure keeps the saved session and gates all business content until retry succeeds.
 await send('Page.addScriptToEvaluateOnNewDocument',{source:`window.__authFault=503`});
 await send('Page.reload');await wait(`document.body?.innerText.includes('Restoring session')&&document.querySelector('[role="alert"]')`);
 await assert(`!!sessionStorage.getItem('converyn.session')&&!document.querySelector('.sidebar')&&window.__restoreReads.every(p=>p.endsWith('/identity/me'))`,'temporary identity failure cleared or bypassed session validation');
 await evaluate(`window.__authFault=0`);await click('Retry');await wait(`document.body?.innerText.includes('Control plane members')`);
 // A rejected credential is removed and never restored on a later refresh.
 await evaluate(`window.__authFault=401`);await wait(`!!document.querySelector('input[name="token"]')`);
 await assert(`!sessionStorage.getItem('converyn.session')`,'rejected credentials remained stored');
}
if(errors.length)throw Error('Runtime exceptions: '+errors.join(', '));
const report={passed:true,embedded_assets:true,real_mtls_relay:true,named_login:true,parameter_form:true,parameter_empty_state:true,parameter_keyboard_add:true,parameter_error_focus:true,parameter_unified_reset:true,parameter_mobile_layout:true,workflow_steps_before_preview:true,registered_script_body:true,temporary_file_configuration:!!cfg.inventory_log_fixture,parameter_constraints:!!cfg.inventory_log_fixture,preview_invalidation:true,configuration_defaults:true,step_details_and_selection:true,wide_script_reader:true,run_dialog_restores_scroll_and_focus:true,log_follow_is_explicit:true,nonretryable_sse_stops:true,audit_activity_default:true,collapsed_node_ips:true,group_delete_error_retains_input:true,group_revision_conflict_retains_draft:true,elapsed_time:true,clipboard_feedback:true,empty_stderr:true,receipt_tail_fallback:true,actual_gui_run:run,person,node_detail_direct_route:true,late_node_response_isolated:true,live_log_fixture:!!cfg.inventory_log_fixture,log_reconnect_and_download:!!cfg.inventory_log_fixture,authorized_navigation:true,access_management_mutations:true,run_dates_newest_first:true,no_overlapping_polls:true,mobile:true,tab_session_restoration:true,logout_clears_session:true,locale_switch_and_persistence:true,structured_controlplane:!!cfg.admin_token,restore_failure_and_revocation:!!cfg.admin_token,no_persistent_token_storage:true};
fs.writeFileSync(cfg.report,JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));socket.close();
