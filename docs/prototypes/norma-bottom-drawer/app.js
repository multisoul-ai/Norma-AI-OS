const $=s=>document.querySelector(s),esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const state={lang:'zh',space:'product',project:'all',active:'login',newDraft:'',canvasSpace:'product',canvasProject:'norma',camera:{x:0,y:0,s:1},focus:null,returnContext:null,expanded:false,theme:'dark',last:{},query:'',filter:'all'};
const t=k=>copy[state.lang][k]||k,l=a=>Array.isArray(a)?a[state.lang==='zh'?0:1]:a;
const icons={up:'M12 19V5m-6 6 6-6 6 6',down:'m6 9 6 6 6-6',close:'m6 6 12 12M6 18 18 6',arrow:'M7 17 17 7M7 7h10v10',grid:'M3 3h7v7H3zm11 0h7v7h-7zM3 14h7v7H3zm11 0h7v7h-7z',cube:'m12 3 9 5v8l-9 5-9-5V8Zm0 0v18M3 8l9 5 9-5',mic:'M9 5a3 3 0 0 1 6 0v7a3 3 0 0 1-6 0zM6 10v2a6 6 0 0 0 12 0v-2M12 18v4m-3 0h6',sun:'M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1 1m12 12 1 1M5 19l1-1M18 6l1-1M16 12a4 4 0 1 1-8 0 4 4 0 0 1 8 0',panel:'M3 5h18v14H3zm6 0v14m3-10 3 3-3 3',search:'M15 15l5 5M17 10a7 7 0 1 1-14 0 7 7 0 0 1 14 0',check:'m5 12 4 4L19 6',clock:'M12 7v5l3 2M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0',pause:'M8 5v14M16 5v14',play:'m8 4 12 8-12 8z',expand:'M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5',back:'m14 6-6 6 6 6',folder:'M3 6h7l2 2h9v12H3z',plus:'M12 5v14M5 12h14',spark:'m12 2 2.5 7.5L22 12l-7.5 2.5L12 22l-2.5-7.5L2 12l7.5-2.5z',terminal:'m5 7 5 5-5 5m8 0h6',bell:'M5 17h14l-2-3V8a5 5 0 0 0-10 0v6zm5 3h4',link:'m9 15 6-6m-5-3 2-2a5 5 0 0 1 7 7l-2 2M7 11l-2 2a5 5 0 0 0 7 7l2-2'};
const icon=n=>`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.55" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="${icons[n]||icons.spark}"/></svg>`;
const btn=(action,label,i,cls='icon-button',extra='')=>`<button class="${cls}" data-action="${action}" aria-label="${esc(label)}" title="${esc(label)}" ${extra}>${icon(i)}</button>`;
const orb='<span class="orb" aria-hidden="true"><i></i></span>';
const contexts=new Map();
function key(w){return [w.space,w.project,w.card||w.id,w.session].join('/');}
function ctx(w){const k=key(w);if(!contexts.has(k))contexts.set(k,{draft:'',messages:[],scroll:0});return contexts.get(k);}
const active=()=>works.find(w=>w.id===state.active),spaceName=id=>l(spaces.find(s=>s.id===id).name),projectName=id=>projects[id].name;
const filtered=()=>works.filter(w=>w.space===state.space&&(state.project==='all'||w.project===state.project));
const count=(status,space=state.space)=>works.filter(w=>w.space===space&&w.status===status).length;
function avatar(w){return `<span class="avatar" style="--avatar:${w.color}">${esc(w.agent.slice(0,1))}</span>`;}
function status(w){return `<span class="status ${w.status}"><i></i>${t(w.status)}</span>`;}

function renderTop(){
 $('#topbar').innerHTML=`<div class="brand-trail">${icon('panel')}${orb}<span>${spaceName(state.canvasSpace)}</span><b>/</b><strong>${projectName(state.canvasProject)}</strong></div><div class="top-actions"><span class="prototype-tag">${t('demo')}</span><span class="runtime-status"><i></i>${count('running',state.canvasSpace)} ${t('active')}<span class="separator"></span>${count('waiting',state.canvasSpace)} ${t('waiting')}</span>${btn('open',t('work'),'panel')}</div>`;
}
function renderDocks(){
 const dock=(inside)=>`<nav class="dock" aria-label="${t('canvas')}">${btn('overview',t('overview'),'cube')}${btn('new',t('newTask'),'plus')}<span class="dock-divider"></span><button class="dock-command ${inside?'selected':''}" data-action="${inside?'close':'open'}" ${inside?'':'data-testid="dock-open"'} aria-expanded="${inside}" aria-controls="work-drawer">${orb}<span>${inside?'Norma':t('command')}</span><span class="dock-count">${count('needs',state.canvasSpace)}</span><kbd>⌘ E</kbd></button>${btn('voice',t('voice'),'mic')}<span class="dock-divider"></span><button class="project-number ${state.canvasProject==='norma'?'selected':''}" data-action="project-one">01</button><button class="project-number ${state.canvasProject==='site'?'selected':''}" data-action="project-two">02</button>${btn('overview',t('overview'),'grid')}${btn('theme',t('theme'),'sun')}</nav>`;
 $('#base-dock').innerHTML=dock(false);$('#drawer-dock').innerHTML=dock(true);
}
function renderHeader(){
 $('#sheet-header').innerHTML=`<div class="sheet-brand">${orb}<h1 id="drawer-title">Norma</h1><span>${t('work')}</span></div><div class="scope-controls"><button class="scope-button" data-action="workspace" aria-haspopup="dialog" aria-expanded="false" aria-controls="scope-popover">${icon('cube')}<span>${spaceName(state.space)}</span>${icon('down')}</button><span class="slash">/</span><button class="scope-button" data-action="project" aria-haspopup="dialog" aria-expanded="false" aria-controls="scope-popover"><span>${state.project==='all'?t('all'):projectName(state.project)}</span>${icon('down')}</button></div><div class="window-controls">${btn('language',t('language'),'', 'language-button').replace(icon(''),'')}${btn('expand',t(state.expanded?'compact':'expand'),'expand')}${btn('close',t('close'),'down')}</div>`;
 $('.language-button').textContent=t('language');$('.handle').ariaLabel=t('close');
}
const listScroll=new Map();let renderedScope=null;
function renderList(){
 if(renderedScope)listScroll.set(renderedScope,$('#work-list').scrollTop);
 renderedScope=`${state.space}/${state.project}`;
 $('#work-list').innerHTML=`<div class="intro"><h2>${t('newTitle')}</h2><p>${t('newHint')}</p></div><form id="new-form" class="new-composer"><label for="new-work-input">${t('newWorkLabel')}</label><textarea id="new-work-input" data-testid="new-input" aria-label="${t('newTask')}" placeholder="${t('newPlaceholder')}" rows="2">${esc(state.newDraft)}</textarea><div class="new-composer-tools"><span class="coordinator">${orb}<span>${t('coordinator')}</span></span><div><span class="target-hint">${t('targetLabel')}</span><button type="submit" class="send-button" data-testid="new-send" aria-label="${t('newTask')}" ${state.newDraft.trim()?'':'disabled'}>${icon('up')}</button></div></div></form>${workToolbar()}<div class="task-groups" id="task-groups">${workRows()}</div>`;
 $('#work-list').scrollTop=listScroll.get(renderedScope)||0;
}

function messageHTML(w){
 const c=ctx(w);return `<div class="thread-date">${t('today')}<span>09:41</span></div><div class="user-message">${esc(l(w.initial))}</div><div class="agent-message"><div class="message-author">${avatar(w)}<strong>${esc(w.agent)}</strong><span>${esc(w.engine)}</span></div><p>${esc(l(w.answer))}</p><details class="execution"><summary>${icon('terminal')}<span>${t('steps')}</span><small>${w.progress}/${w.total}</small>${icon('down')}</summary><div class="step-list">${w.steps.map((s,i)=>`<div class="step ${i<w.progress?'complete':''}"><span>${i<w.progress?icon('check'):'<i></i>'}</span>${esc(l(s))}</div>`).join('')}</div></details></div>${w.recommendation&&w.status==='needs'?`<div class="recommendation"><div class="recommendation-title">${orb}<strong>${t('recommend')}</strong></div><p>${esc(l(w.recommendation))}</p><div class="recommendation-actions"><button class="primary" data-action="accept" data-testid="accept">${icon('check')}${t(w.id==='navigation'?'review':'approve')}</button><button class="text-button" data-action="edit-recommendation">${t('editRecommendation')}${icon('arrow')}</button></div></div>`:''}${c.messages.map(m=>m.role==='user'?`<div class="user-message">${esc(m.text)}</div>`:`<div class="agent-message"><div class="message-author">${avatar(w)}<strong>${esc(w.agent)}</strong><span>${t('replyTag')}</span></div><p>${esc(m.text)}</p></div>`).join('')}`;
}
function composer(w,surface){const c=ctx(w);return `<form class="reply-composer" data-chat-form="${surface}"><div class="reply-context">${icon('link')}<span>${t('route')} <b>${esc(w.agent)}</b> · ${esc(projectName(w.project))}</span><span class="draft-label">${c.draft?t('draft'):''}</span></div><div class="reply-input-row"><textarea rows="2" data-chat="${w.id}" data-testid="${surface==='canvas'?'canvas-input':'thread-input'}" placeholder="${state.lang==='zh'?`继续和 ${w.agent} 沟通…`:`Continue with ${w.agent}…`}" aria-label="${t('continue')}">${esc(c.draft)}</textarea><button class="send-button" type="submit" data-testid="${surface==='canvas'?'canvas-send':'thread-send'}" aria-label="${t('send')}" ${c.draft.trim()?'':'disabled'}>${icon('up')}</button></div></form>`;}
function saveScroll(){if(active()&&$('#thread .messages'))ctx(active()).scroll=$('#thread .messages').scrollTop;if(renderedScope)listScroll.set(renderedScope,$('#work-list').scrollTop);}
function renderThread(bottom=false){
 const w=active();if(!w){$('#thread').innerHTML=`<div class="empty">${t('empty')}</div>`;return;}
 $('#thread').innerHTML=`<header class="thread-header">${btn('list',t('back'),'back','icon-button mobile-back')}<div class="thread-heading"><div><span class="stage-pill ${w.status}" data-testid="thread-status">${t(w.status)}</span><i>·</i><span>${esc(w.agent)} / ${esc(w.engine)}</span></div><h2 data-testid="thread-title" tabindex="-1">${esc(l(w.title))}</h2></div><button class="locate-button" data-action="locate" data-testid="locate" title="${t(w.card?'locate':'notAssigned')}" aria-label="${t('locate')}" ${w.card?'':'disabled'}>${icon('arrow')}<span>${t('locateShort')}</span></button></header><div class="session-line">${icon('link')}<span>${t('same')}</span><small>${spaceName(w.space)} / ${projectName(w.project)}</small></div><div class="thread-filter-hint" data-testid="filtered-conversation" hidden><span>${t('outsideFilter')}</span><button data-action="clear-filters">${t('showCurrent')}</button></div><div class="messages">${messageHTML(w)}</div><div class="reply-area">${composer(w,'thread')}<div class="reply-caption"><span>${t('local')}</span><span>⌘ ↵ ${t('send')}</span></div></div>`;
 updateConversationHint();
 const m=$('#thread .messages');m.scrollTop=bottom?m.scrollHeight:ctx(w).scroll;
}
function renderFooter(){
 $('#sheet-footer').innerHTML=`<span class="footer-status"><i></i>${count('needs')} ${t('pending')}<span>·</span>${count('running')} ${t('active')}</span><span class="footer-note">${t('footer')}</span><div>${btn('event',t('event'),'bell','icon-button','data-testid="simulate-event"')}${btn('video',t($('#wallpaper').paused?'play':'pause'),$('#wallpaper').paused?'play':'pause')}<kbd>esc</kbd></div>`;
}
function renderSheet(bottom=false){renderHeader();renderList();renderThread(bottom);renderFooter();renderDocks();}
let animToken=0,closing=false;
function openDrawer(instant=false){
 if(state.returnContext){restoreCanvas();}
 const d=$('#work-drawer');if(d.open&&!closing)return;closing=false;renderSheet();d.classList.toggle('instant',instant);if(!d.open)d.showModal();document.body.dataset.open='true';
 const token=++animToken;requestAnimationFrame(()=>{if(token===animToken)d.classList.add('is-open');});
 (innerWidth<=850&&!$('.sheet').classList.contains('show-list')?$('[data-testid="thread-title"]'):$('#work-list textarea')).focus({preventScroll:true});
}
function closeDrawer(instant=false){
 const d=$('#work-drawer');if(!d.open)return;saveScroll();const pop=$('#scope-popover');if(pop.matches(':popover-open'))pop.hidePopover();
 ++animToken;closing=true;d.classList.toggle('instant',instant);d.classList.remove('is-open');delete document.body.dataset.open;
 const token=animToken;const done=()=>{if(token!==animToken)return;closing=false;d.close();$('#base-dock [data-action="open"]')?.focus({preventScroll:true});};
 if(instant||matchMedia('(prefers-reduced-motion: reduce)').matches)done();else setTimeout(done,240);
}
function selectWork(id){saveScroll();state.active=id;state.last[`${state.space}/${state.project}`]=id;$('.sheet').classList.remove('show-list');renderList();renderThread();(innerWidth<=850?$('[data-testid="thread-title"]'):$(`[data-work="${id}"]`))?.focus({preventScroll:true});}
function setScope(kind,value){
 saveScroll();state.query='';state.filter='all';if(kind==='workspace'){state.space=value;state.project='all';}else state.project=value;
 const last=state.last[`${state.space}/${state.project}`];state.active=filtered().find(w=>w.id===last)?.id||filtered()[0]?.id;renderSheet();
}

function startNew(){
 if(!state.newDraft.trim()){openDrawer();$('#new-form textarea').focus();return;}
 const d=$('#target-dialog');d.innerHTML=`<div class="target-orb">${orb}</div><h2 id="target-title">${t('targetTitle')}</h2><p>${t('targetHint')}</p><blockquote>${esc(state.newDraft)}</blockquote><label>${t('targetWork')}<select id="target-workspace">${spaces.map(s=>`<option value="${s.id}" ${s.id===state.space?'selected':''}>${spaceName(s.id)}</option>`).join('')}</select></label><label>${t('targetProject')}<select id="target-project"></select></label><small>${t('targetNote')}</small><div class="dialog-actions"><button class="secondary" data-action="cancel-new">${t('cancel')}</button><button class="primary" data-action="confirm-new" data-testid="confirm-create">${t('create')}${icon('arrow')}</button></div>`;
 updateTargetProjects();d.showModal();
}
function updateTargetProjects(){const s=spaces.find(s=>s.id===$('#target-workspace').value);$('#target-project').innerHTML=`<option value="" disabled ${state.project==='all'?'selected':''}>${t('targetProject')}…</option>`+s.projects.map(id=>`<option value="${id}" ${id===state.project?'selected':''}>${projectName(id)}</option>`).join('');$('#target-dialog .primary').disabled=!$('#target-project').value;}
function createWork(){
 const project=$('#target-project').value,space=$('#target-workspace').value;if(!project)return;
 const id='local-'+Date.now();works.unshift({id,space,project,card:null,session:id,agent:'Norma',engine:'Super-Agent',color:'#c2b6e6',status:'planning',progress:0,total:2,title:state.newDraft.trim(),summary:[copy.zh.planning,copy.en.planning],initial:state.newDraft.trim(),answer:[copy.zh.planResponse,copy.en.planResponse],steps:[['明确交付物与验收标准','Clarify deliverables and acceptance'],['安排项目中的 Agent','Assign project agents']]});
 state.space=space;state.project=project;state.active=id;state.newDraft='';state.query='';state.filter='all';$('#target-dialog').close();renderSheet();notice(t('createdFeedback'));$('[data-testid="thread-input"]').focus();
}
function send(w,surface){
 const c=ctx(w),text=c.draft.trim();if(!text)return;c.messages.push({role:'user',text},{role:'agent',text:t('reply')});c.draft='';
 if(surface==='canvas'){renderCanvas();const el=$('[data-testid="canvas-input"]');el?.focus();const m=$('.focused .canvas-messages');if(m)m.scrollTop=m.scrollHeight;}
 else{renderList();renderThread(true);$('[data-testid="thread-input"]').focus();}
 notice(t('sentFeedback').replace('{agent}',w.agent));
}
function accept(){const w=active();if(w.status!=='needs')return;ctx(w).messages.push({role:'user',text:l(w.recommendation)},{role:'agent',text:t(w.id==='navigation'?'reviewed':'accepted')});w.status=w.id==='navigation'?'merge':'running';w.summary=w.id==='navigation'?['验收已记录 · 等待合并','Review recorded · Awaiting merge']:['已记录选择 · 继续执行','Choice recorded · Continuing'];renderSheet(true);renderCanvas();renderTop();notice(t(w.id==='navigation'?'reviewFeedback':'decisionFeedback'),{action:t('viewProgress'),onAction:()=>showWork(w.id)});$('[data-testid="thread-input"]').focus({preventScroll:true});}
function positions(){const width=innerWidth-28,gap=20,cols=width>1120?3:2,cw=(width-gap*(cols-1))/cols,ch=Math.min(368,(innerHeight-160)/2);return {cols,cw,ch,gap};}
function renderCanvas(){
 const p=positions();const list=works.filter(w=>w.space===state.canvasSpace&&w.project===state.canvasProject&&w.card);$('#world').innerHTML=list.map((w,i)=>{
 const focus=w.id===state.focus,x=(i%p.cols)*(p.cw+p.gap)+14,y=Math.floor(i/p.cols)*(p.ch+20)+28;
 return `<article class="canvas-card ${focus?'focused':''} ${w.id==='drawer'?'current':''}" data-node="${w.id}" style="left:${x}px;top:${y}px;width:${focus?Math.min(650,innerWidth-50):p.cw}px;height:${focus?Math.min(700,innerHeight-180):p.ch}px"><header><button class="card-title" data-focus="${w.id}"><span class="agent-pill">• Agent</span><b>${w.agent}</b><span>— ${projectName(w.project)} · ${esc(l(w.title))}</span></button><small class="${w.status}">${w.status==='running'?t('running'):w.status==='needs'?t('needs'):w.status==='waiting'?t('waiting'):t('done')}</small>${btn(focus?'unfocus':'card-open',focus?t('reset'):t('continue'),focus?'close':'arrow','card-action',`data-card="${w.id}"`)}</header>${focus?`<div class="canvas-messages">${messageHTML(w)}</div><div class="canvas-composer">${composer(w,'canvas')}</div>`:`<div class="terminal-content">${(w.terminal||[['text',l(w.answer)],['muted',l(w.summary)]]).map(([cls,text])=>`<div class="${cls}">${esc(state.lang==='en'?(cls==='muted'?l(w.summary):text):text)}</div>`).join('')}</div><button class="terminal-prompt" data-focus="${w.id}"><span>›</span> ${w.engine==='Codex'?'Ask Codex to do anything':'What would you like to do next?'}</button><div class="terminal-model"><span>${w.engine==='Codex'?'GPT-6.1-Sol xhigh':'full $1.50 · 20m'}</span><span>${esc(projects[w.project].path)}</span></div>`}</article>`;
 }).join('');
 const {x,y,s}=state.camera;$('#world').style.transform=`translate(${x}px,${y}px) scale(${s})`;
 $('#return-bar').innerHTML=state.returnContext?`<button class="return-button" data-action="return" data-testid="return">${icon('back')}${t('return')}<span>· ${esc(active().agent)}</span><kbd>esc</kbd></button>`:state.focus?`<button class="return-button" data-action="unfocus">${icon('back')}${t('reset')}</button>`:'';
}
function focusCard(id,fromDrawer=false){
 if(fromDrawer&&!state.returnContext)state.returnContext={space:state.canvasSpace,project:state.canvasProject,camera:{...state.camera},focus:state.focus};
 const w=works.find(w=>w.id===id);if(!w.card)return;
 state.canvasSpace=w.space;state.canvasProject=w.project;state.focus=id;state.camera={x:0,y:0,s:1};renderCanvas();
 const el=$(`[data-node="${id}"]`);const x=parseFloat(el.style.left),y=parseFloat(el.style.top);state.camera={x:(innerWidth-el.offsetWidth)/2-x,y:(innerHeight-135-el.offsetHeight)/2-y,s:1};renderCanvas();renderTop();renderDocks();if(fromDrawer)closeDrawer(true);$('[data-testid="canvas-input"]')?.focus();
}
function restoreCanvas(){const r=state.returnContext;if(!r)return;state.canvasSpace=r.space;state.canvasProject=r.project;state.camera=r.camera;state.focus=r.focus;state.returnContext=null;renderCanvas();renderTop();renderDocks();}
function overview(){state.focus=null;state.camera={x:0,y:0,s:1};state.returnContext=null;renderCanvas();renderTop();renderDocks();}
function renderAll(){document.documentElement.lang=state.lang==='zh'?'zh-CN':'en';document.body.dataset.theme=state.theme;renderTop();renderCanvas();renderDocks();if($('#work-drawer').open)renderSheet();}

document.addEventListener('click',e=>{
 const option=e.target.closest('[data-option]');if(option){chooseScope(option.dataset.option);return;}
 const work=e.target.closest('[data-work]');if(work){selectWork(work.dataset.work);return;}
 const focus=e.target.closest('[data-focus]');if(focus){focusCard(focus.dataset.focus);return;}
 const el=e.target.closest('[data-action]');if(!el)return;const action=el.dataset.action,instant=e.detail===0;
 if(action==='open')openDrawer(instant);
 if(action==='close'){if(closing)openDrawer(instant);else closeDrawer(instant);}
 if(action==='workspace'||action==='project')openScope(action,el);
 if(action==='expand'){state.expanded=!state.expanded;$('.sheet').classList.toggle('expanded',state.expanded);renderHeader();}
 if(action==='list'){$('.sheet').classList.add('show-list');$(`[data-work="${state.active}"]`)?.focus();}
 if(action==='language'){saveScroll();state.lang=state.lang==='zh'?'en':'zh';renderAll();}
 if(action==='theme'){state.theme=state.theme==='dark'?'light':'dark';renderAll();}
 if(action==='new'){openDrawer(instant);$('.sheet').classList.add('show-list');$('#new-form textarea').focus();}
 if(action==='locate')focusCard(active().id,true);
 if(action==='return'){restoreCanvas();openDrawer(instant);}
 if(action==='unfocus'||action==='overview'){if($('#work-drawer').open)closeDrawer(instant);overview();}
 if(action==='project-one'||action==='project-two'){if($('#work-drawer').open)closeDrawer(instant);state.canvasProject=action==='project-one'?'norma':'site';overview();}
 if(action==='card-open'){state.space=works.find(w=>w.id===el.dataset.card).space;state.project='all';state.active=el.dataset.card;openDrawer(instant);}
 if(action==='voice')notice(t('voiceHint'));
 if(action==='video'){const v=$('#wallpaper');if(v.paused)v.play().catch(()=>{});else v.pause();renderFooter();}
 if(action==='accept')accept();
 if(action==='edit-recommendation'){ctx(active()).draft=l(active().recommendation);renderThread();$('[data-testid="thread-input"]').focus();}
 if(action==='event'){const w=works.find(w=>w.id==='drawer');w.status='needs';w.summary=[copy.zh.newEvent,copy.en.newEvent];renderList();renderFooter();renderDocks();notice(t('eventNote'),{action:t('show'),onAction:()=>showWork(w.id)});}
 if(action==='cancel-new')$('#target-dialog').close();
 if(action==='confirm-new')createWork();
});
document.addEventListener('input',e=>{
 if(e.target.matches('[data-chat]')){const w=works.find(w=>w.id===e.target.dataset.chat);ctx(w).draft=e.target.value;const form=e.target.closest('form');form.querySelector('.send-button').disabled=!e.target.value.trim();form.querySelector('.draft-label').textContent=e.target.value?t('draft'):'';}
 if(e.target.matches('[data-testid="new-input"]')){state.newDraft=e.target.value;$('[data-testid="new-send"]').disabled=!state.newDraft.trim();}
});
document.addEventListener('change',e=>{if(e.target.id==='target-workspace')updateTargetProjects();if(e.target.id==='target-project')$('#target-dialog .primary').disabled=!e.target.value;});
document.addEventListener('submit',e=>{e.preventDefault();if(e.target.id==='new-form')startNew();else if(e.target.dataset.chatForm){const w=works.find(w=>w.id===e.target.querySelector('[data-chat]').dataset.chat);send(w,e.target.dataset.chatForm);}});
document.addEventListener('keydown',e=>{
 if(e.isComposing)return;
 if((e.metaKey||e.ctrlKey)&&e.key.toLowerCase()==='e'){e.preventDefault();if($('#target-dialog').open)return;$('#work-drawer').open&&!closing?closeDrawer(true):openDrawer(true);}
 if((e.metaKey||e.ctrlKey)&&e.key==='Enter'&&e.target.closest('form')){e.preventDefault();e.target.closest('form').requestSubmit();}
 if(e.key==='Escape'&&!$('#work-drawer').open&&!$('#target-dialog').open&&state.returnContext){e.preventDefault();restoreCanvas();openDrawer(true);}
});
$('#work-drawer').addEventListener('cancel',e=>{e.preventDefault();closeDrawer(true);});
let backdropPress=false;
$('#work-drawer').addEventListener('pointerdown',e=>{backdropPress=e.target===$('#work-drawer');});
$('#work-drawer').addEventListener('click',e=>{if(backdropPress&&e.target===$('#work-drawer'))closeDrawer();backdropPress=false;});
const reduced=matchMedia('(prefers-reduced-motion: reduce)');function syncReduced(){if(reduced.matches)$('#wallpaper').pause();}reduced.addEventListener('change',syncReduced);syncReduced();
window.addEventListener('resize',()=>{if(state.focus){const w=state.focus;focusCard(w);}else renderCanvas();});
initializeMaterial();renderAll();if(new URLSearchParams(location.search).has('open'))openDrawer(true);
