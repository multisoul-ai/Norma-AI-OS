// Standalone window coordination is isolated from the original embedded prototype.
const windowParams=new URLSearchParams(location.search);
const windowMode=['tasks','canvas'].includes(windowParams.get('window'))?windowParams.get('window'):'embedded';
const demoId=windowParams.get('demo')||'shared';
const sharedKey='norma-inbox-windows-v1:'+demoId;
const viewKey=sharedKey+':view:'+windowMode;
const peerHandles=new Map(),pendingPatches=[],pendingWrites=new Set();
let sharedBase=null,lastSnapshot=null,sharedReady=false,windowChannel=null;
window.__pendingSyncCount=0;

function sharedSnapshot(){
 const contexts={};
 for(const [key,c] of state.contexts){c.messages.forEach(m=>{if(!m.id)m.id=crypto.randomUUID();});contexts[key]={draft:c.draft,attachment:c.attachment,messages:structuredClone(c.messages)};}
 return {items:structuredClone(state.items),contexts,eventAdded:state.eventAdded};
}
function dataDiff(before,after){
 const patch={id:crypto.randomUUID(),items:{},added:[],contexts:{}};
 for(const item of after.items){const old=before.items.find(a=>a.id===item.id);if(!old){patch.added.push(item);continue;}const fields={};for(const [key,value] of Object.entries(item)){if(JSON.stringify(old[key])!==JSON.stringify(value))fields[key]=value;}if(Object.keys(fields).length)patch.items[item.id]=fields;}
 for(const [key,c] of Object.entries(after.contexts)){const old=before.contexts[key]||{draft:'',attachment:null,messages:[]},fields={};for(const k of ['draft','attachment'])if(JSON.stringify(old[k])!==JSON.stringify(c[k]))fields[k]=c[k];const ids=new Set(old.messages.map(m=>m.id)),added=c.messages.filter(m=>!ids.has(m.id));if(added.length)fields.messages=added;if(Object.keys(fields).length)patch.contexts[key]=fields;}
 if(before.eventAdded!==after.eventAdded)patch.eventAdded=after.eventAdded;
 return Object.keys(patch.items).length||patch.added.length||Object.keys(patch.contexts).length||'eventAdded'in patch?patch:null;
}
function applySharedPatch(data,patch){
 const next=structuredClone(data);
 for(const item of patch.added)if(!next.items.some(a=>a.id===item.id))next.items.unshift(item);
 for(const [id,fields] of Object.entries(patch.items)){const item=next.items.find(a=>a.id===id);if(item)Object.assign(item,fields);}
 for(const [key,fields] of Object.entries(patch.contexts)){const c=next.contexts[key]||(next.contexts[key]={draft:'',messages:[],attachment:null});if('draft'in fields)c.draft=fields.draft;if('attachment'in fields)c.attachment=fields.attachment;if(fields.messages){const ids=new Set(c.messages.map(m=>m.id));c.messages.push(...fields.messages.filter(m=>!ids.has(m.id)));}}
 if('eventAdded'in patch)next.eventAdded=patch.eventAdded;return next;
}
function loadRecord(){try{const record=JSON.parse(localStorage.getItem(sharedKey));return record?.version===1?record:null;}catch{return null;}}
function installData(data){
 state.items=structuredClone(data.items);state.eventAdded=data.eventAdded;
 for(const [key,value] of Object.entries(data.contexts)){const old=state.contexts.get(key);state.contexts.set(key,{...structuredClone(value),scroll:old?.scroll||0});}
}
function receiveRecord(record){
 if(!record)return;
 if(record.rev>=sharedBase.rev)sharedBase=record;let data=structuredClone(sharedBase.data);for(const patch of pendingPatches)data=applySharedPatch(data,patch);
 const changed=JSON.stringify(sharedSnapshot())!==JSON.stringify(data);if(changed){remember();installData(data);renderFromPeer();}
 lastSnapshot=sharedSnapshot();
}
function renderFromPeer(){
 const focused=document.activeElement,testid=focused?.dataset.testid,start=focused?.selectionStart,end=focused?.selectionEnd;
 const scrolls=$$('.canvas-terminal').map(el=>[el.closest('[data-card]').dataset.card,el.scrollTop]);
 render();for(const [id,y] of scrolls)$(`[data-card="${id}"] .canvas-terminal`)?.scrollTo(0,y);
 if(testid){const el=$(`[data-testid="${testid}"]`);if(el){el.focus({preventScroll:true});if(start!=null&&el.setSelectionRange)el.setSelectionRange(start,end);}}
 if(state.menu&&$('#chooser').matches(':popover-open'))$(`[data-menu="${state.menu.kind}"]`)?.setAttribute('aria-expanded','true');
}
function saveWindowView(){
 if(windowMode==='embedded')return;
 const value={active:state.active,scope:state.scope,destination:state.destination,recipient:state.recipient,newDraft:state.newDraft,archiveOpen:state.archiveOpen,leftScroll:state.leftScroll,canvasId:state.canvasId,camera:state.camera,lang};
 try{localStorage.setItem(viewKey,JSON.stringify(value));}catch{}
}
function publishSharedChanges(){
 saveWindowView();if(!sharedReady)return;
 const after=sharedSnapshot(),patch=dataDiff(lastSnapshot,after);lastSnapshot=after;if(!patch)return;
 pendingPatches.push(patch);window.__pendingSyncCount=pendingPatches.length;
 const write=navigator.locks.request(sharedKey,()=>{const base=loadRecord()||sharedBase;const record={version:1,rev:base.rev+1,data:applySharedPatch(base.data,patch)};localStorage.setItem(sharedKey,JSON.stringify(record));return record;}).then(record=>{const i=pendingPatches.findIndex(p=>p.id===patch.id);if(i>=0)pendingPatches.splice(i,1);window.__pendingSyncCount=pendingPatches.length;receiveRecord(record);}).catch(()=>{notice(tr('这次同步没有保存成功，请保留窗口后重试','This update could not be saved. Keep the window open and try again.'));}).finally(()=>pendingWrites.delete(write));
 pendingWrites.add(write);
}
function initSharedWindows(){
 document.body.dataset.window=windowMode;if(windowMode==='embedded')return;
 window.name='norma-'+windowMode+'-'+demoId;
 if(!navigator.locks||!location.protocol.startsWith('http')){notice(tr('双窗口同步请通过本地 HTTP 预览地址打开','Use the local HTTP preview for shared windows'));return;}
 const record=loadRecord();sharedBase=record||{version:1,rev:0,data:sharedSnapshot()};installData(sharedBase.data);
 try{const view=JSON.parse(localStorage.getItem(viewKey));if(view){for(const key of ['active','scope','destination','recipient','newDraft','archiveOpen','leftScroll','canvasId','camera'])if(key in view)state[key]=view[key];lang=view.lang||lang;}}catch{}
 if(state.active&&!byId(state.active))state.active=null;if(state.canvasId&&!byId(state.canvasId))state.canvasId=null;
 lastSnapshot=sharedSnapshot();sharedReady=true;
 window.addEventListener('storage',e=>{if(e.key===sharedKey&&e.newValue){try{receiveRecord(JSON.parse(e.newValue));}catch{}}});
 windowChannel=new BroadcastChannel(sharedKey+':navigation');windowChannel.onmessage=e=>receiveNavigation(e.data);
 window.addEventListener('message',e=>{if(e.origin===location.origin)receiveNavigation(e.data);});
 for(const event of ['click','input','change','submit','keydown','pointerup'])document.addEventListener(event,publishSharedChanges);
 window.addEventListener('pagehide',saveWindowView);
}
function receiveNavigation(message){
 if(message?.kind!=='norma-window-navigation'||message.demo!==demoId||message.role!==windowMode)return;
 if(windowMode==='tasks'){if(message.id&&byId(message.id))openWork(message.id);window.focus();}
 else if(message.id&&byId(message.id)){focusCanvasCard(message.id);window.focus();}
}
function showPeer(role,id=null){
 publishSharedChanges();const name='norma-'+role+'-'+demoId;let peer=peerHandles.get(role);
 if(!peer||peer.closed)peer=window.open('',name,role==='tasks'?'popup=yes,width=1380,height=940,resizable=yes,scrollbars=yes':'popup=yes,width=1500,height=980,resizable=yes,scrollbars=yes');
 if(!peer){notice(tr('请允许此预览地址打开新窗口，再点击一次','Allow pop-up windows for this preview, then try again'));return;}
 peerHandles.set(role,peer);const url=new URL(location.href);url.searchParams.set('window',role);url.searchParams.set('demo',demoId);url.searchParams.delete('scene');url.searchParams.delete('task');if(id)url.searchParams.set('task',id);
 if(peer.location.href==='about:blank')peer.location.replace(url.href);else{const message={kind:'norma-window-navigation',demo:demoId,role,id};peer.postMessage(message,location.origin);}
 peer.focus();
}
async function closeTaskWindow(){
 remember();closeMenu(false);publishSharedChanges();await Promise.all([...pendingWrites]);window.close();
 if(!window.closed)notice(tr('请用浏览器的窗口关闭按钮关闭；画布会继续保留','Close this browser window using its window control. The canvas stays open.'));
}
