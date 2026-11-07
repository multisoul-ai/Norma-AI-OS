// Material interactions: committed values are separate from temporary keyboard focus.
const menuState={kind:null,choices:[],visible:[],current:null,browsing:null};
const snackbarState={timer:null,remaining:0,started:0,action:null};
function visibleWorks(){
 const q=state.query.trim().toLocaleLowerCase();
 return filtered().filter(w=>(state.filter==='all'||w.status===state.filter)&&(!q||[l(w.title),l(w.summary),w.agent,projectName(w.project)].join(' ').toLocaleLowerCase().includes(q)));
}
function workToolbar(){
 return `<div class="work-tools"><div class="search-field">${icon('search')}<input type="search" data-testid="work-search" aria-label="${t('searchWork')}" placeholder="${t('searchWork')}" value="${esc(state.query)}">${btn('clear-search',t('clearSearch'),'close','search-clear',state.query?'':'hidden')}</div><div class="filter-chips" role="group" aria-label="${t('filterStatus')}">${filterButtons()}</div></div>`;
}
function filterButtons(){
 return ['all','needs','running','done'].map(f=>`<button class="filter-chip" data-filter="${f}" aria-pressed="${state.filter===f}">${state.filter===f?icon('check'):''}<span>${t(f==='all'?'allWork':f)}</span><small>${filtered().filter(w=>f==='all'||w.status===f).length}</small></button>`).join('');
}
function workRows(){
 const visible=visibleWorks();
 if(!visible.length)return `<div class="work-empty" data-testid="work-empty">${icon('search')}<strong>${t('noWorkResults')}</strong><p>${t('tryFilter')}</p><button class="text-button" data-action="clear-filters">${t('clearFilters')}</button></div>`;
 return ['needs','running','waiting','planning','merge','done'].map(g=>{
  const list=visible.filter(w=>w.status===g);if(!list.length)return '';
  return `<section class="task-group"><h3><span class="group-dot ${g}"></span>${t(g)}<span class="group-count">${list.length}</span></h3>${list.map(w=>`<button class="work-row ${state.active===w.id?'selected':''}" data-work="${w.id}" aria-pressed="${state.active===w.id}"><span class="row-status ${w.status}">${icon(w.status==='done'||w.status==='merge'?'check':w.status==='needs'?'bell':w.status==='waiting'?'clock':'spark')}</span><span class="row-content"><strong>${esc(l(w.title))}</strong><small>${esc(w.agent)}<span>·</span>${esc(projectName(w.project))}<span class="row-summary">· ${esc(l(w.summary))}</span></small></span><span class="row-trailing">${ctx(w).draft?`<small class="draft-badge">${t('draft')}</small>`:w.status==='running'?`<small class="step-count" title="${t('steps')}">${w.progress}/${w.total}</small>`:''}${state.active===w.id?`<span class="selection-check" aria-label="${t('selected')}">${icon('check')}</span>`:avatar(w)}</span></button>`).join('')}</section>`;
 }).join('');
}
function syncTaskFilter(){
 $('#task-groups').innerHTML=workRows();$('.filter-chips').innerHTML=filterButtons();
 $('.search-clear').hidden=!state.query;
 updateConversationHint();
}
function updateConversationHint(){
 const hint=$('.thread-filter-hint');if(!hint)return;
 hint.hidden=!active()||visibleWorks().some(w=>w.id===state.active);
}
function clearTaskFilters(){state.query='';state.filter='all';const input=$('[data-testid="work-search"]');input.value='';syncTaskFilter();}
function renderScopeResults(query=''){
 const q=query.trim().toLocaleLowerCase();menuState.visible=menuState.choices.filter(c=>(c.title+' '+c.sub).toLocaleLowerCase().includes(q));
 menuState.browsing=menuState.visible.find(o=>o.id===menuState.current)?.id||menuState.visible[0]?.id||null;
 $('#scope-options').innerHTML=menuState.visible.map(o=>`<button role="option" id="scope-option-${o.id}" tabindex="-1" data-option="${o.id}" data-kind="${menuState.kind}" class="scope-option ${menuState.current===o.id?'selected':''}" aria-selected="${menuState.current===o.id}"><span class="option-leading">${icon(menuState.kind==='workspace'?'cube':'folder')}</span><span>${esc(o.title)}<small>${esc(o.sub)}</small></span>${menuState.current===o.id?icon('check'):''}</button>`).join('')||`<div class="menu-empty" data-testid="scope-empty">${icon('search')}<strong>${t('noScopeResults')}</strong><span>${t('trySearch')}</span></div>`;
 syncMenuBrowsing();
}
function syncMenuBrowsing(){
 const input=$('.scope-search');
 document.querySelectorAll('.scope-option').forEach(el=>el.classList.toggle('browsing',el.dataset.option===menuState.browsing));
 if(menuState.browsing)input.setAttribute('aria-activedescendant','scope-option-'+menuState.browsing);else input.removeAttribute('aria-activedescendant');
 $(`#scope-option-${menuState.browsing}`)?.scrollIntoView({block:'nearest'});
}
function openScope(kind,trigger){
 const p=$('#scope-popover');if(p.matches(':popover-open')){const same=menuState.kind===kind;dismissScope(false);if(same)return;}
 menuState.kind=kind;menuState.current=kind==='workspace'?state.space:state.project;
 menuState.choices=kind==='workspace'?spaces.map(s=>({id:s.id,title:spaceName(s.id),sub:s.projects.map(projectName).join(' · ')})):[{id:'all',title:t('all'),sub:spaceName(state.space)},...spaces.find(s=>s.id===state.space).projects.map(id=>({id,title:projectName(id),sub:projects[id].path}))];
 p.setAttribute('role','dialog');p.setAttribute('aria-label',t(kind==='workspace'?'workspace':'project'));
 p.innerHTML=`<div class="scope-search-wrap">${icon('search')}<input class="scope-search" role="combobox" aria-autocomplete="list" aria-controls="scope-options" aria-expanded="true" aria-label="${t('search')}" placeholder="${t('search')}"></div><div id="scope-options" class="scope-options" role="listbox" aria-label="${t(kind==='workspace'?'workspace':'project')}"></div><p><span>↑ ↓ ${t('browse')}</span><span>↵ ${t('confirm')}</span><span>esc ${t('cancel')}</span></p>`;
 renderScopeResults();
 const b=trigger.getBoundingClientRect(),width=Math.min(352,innerWidth-24);p.style.width=width+'px';p.style.left=Math.max(12,Math.min(b.right-width,innerWidth-width-12))+'px';p.style.top=Math.min(b.bottom+8,innerHeight-300)+'px';
 p.showPopover();trigger.setAttribute('aria-expanded','true');p.querySelector('input').focus();
}
function dismissScope(restoreFocus=true){
 const kind=menuState.kind;const p=$('#scope-popover');if(p.matches(':popover-open'))p.hidePopover();
 const trigger=$(`[data-action="${kind}"]`);trigger?.setAttribute('aria-expanded','false');if(restoreFocus)trigger?.focus({preventScroll:true});
}
function chooseScope(id){
 if(!menuState.visible.some(c=>c.id===id))return;if(id===menuState.current){dismissScope();return;}const kind=menuState.kind;dismissScope(false);setScope(kind,id);$(`[data-action="${kind}"]`)?.focus({preventScroll:true});
}
function stopSnackbarClock(){
 if(snackbarState.timer){clearTimeout(snackbarState.timer);snackbarState.remaining-=performance.now()-snackbarState.started;snackbarState.timer=null;}
}
function startSnackbarClock(){
 if(!$('#notice').classList.contains('visible'))return;stopSnackbarClock();snackbarState.started=performance.now();snackbarState.timer=setTimeout(hideSnackbar,Math.max(1,snackbarState.remaining));
}
function hideSnackbar(){stopSnackbarClock();$('#notice').classList.remove('visible');$('#notice').inert=true;}
function notice(text,options={}){
 stopSnackbarClock();const n=$('#notice'),host=$('#work-drawer').open?$('#work-drawer'):document.body;host.append(n);n.dataset.place=$('#work-drawer').open?'drawer':'canvas';
 n.innerHTML=`<span class="snackbar-icon">${icon('check')}</span><span class="snackbar-message">${esc(text)}</span>${options.action?`<button data-snackbar-action>${esc(options.action)}</button>`:''}<button class="snackbar-close" data-snackbar-close aria-label="${t('dismiss')}">${icon('close')}</button>`;
 snackbarState.action=options.onAction||null;snackbarState.remaining=options.action?8000:4500;n.inert=false;n.classList.add('visible');startSnackbarClock();
}
function showWork(id){
 const w=works.find(w=>w.id===id);state.space=w.space;state.project='all';state.query='';state.filter='all';state.active=id;
 renderSheet();$('.sheet').classList.remove('show-list');$('[data-testid="thread-input"]')?.focus({preventScroll:true});
}
function initializeMaterial(){
 $('#notice').inert=true;
 $('#notice').addEventListener('pointerenter',stopSnackbarClock);$('#notice').addEventListener('pointerleave',startSnackbarClock);
 $('#notice').addEventListener('focusin',stopSnackbarClock);$('#notice').addEventListener('focusout',e=>{if(!$('#notice').contains(e.relatedTarget))startSnackbarClock();});
 document.addEventListener('visibilitychange',()=>document.hidden?stopSnackbarClock():startSnackbarClock());
 $('#scope-popover').addEventListener('toggle',e=>{if(e.newState==='closed')$(`[data-action="${menuState.kind}"]`)?.setAttribute('aria-expanded','false');});
 document.addEventListener('keydown',e=>{
  if(e.isComposing)return;
  if($('#scope-popover').matches(':popover-open')){
   if(['Escape','Tab'].includes(e.key)){e.preventDefault();e.stopImmediatePropagation();dismissScope();return;}
   if(['ArrowDown','ArrowUp','Home','End'].includes(e.key)){
    e.preventDefault();e.stopImmediatePropagation();const a=menuState.visible;if(!a.length)return;
    const i=a.findIndex(o=>o.id===menuState.browsing);const next=e.key==='Home'?0:e.key==='End'?a.length-1:(i+(e.key==='ArrowDown'?1:a.length-1)+a.length)%a.length;
    menuState.browsing=a[next].id;syncMenuBrowsing();return;
   }
   if(e.key==='Enter'){e.preventDefault();e.stopImmediatePropagation();if(menuState.browsing)chooseScope(menuState.browsing);return;}
  }
  if(e.target.matches('[data-action="workspace"],[data-action="project"]')&&e.key==='ArrowDown'){e.preventDefault();openScope(e.target.dataset.action,e.target);return;}
  if(e.target.matches('[data-work]')&&['ArrowDown','ArrowUp','Home','End'].includes(e.key)){
   e.preventDefault();const rows=[...document.querySelectorAll('[data-work]')],i=rows.indexOf(e.target);
   const next=e.key==='Home'?0:e.key==='End'?rows.length-1:Math.max(0,Math.min(rows.length-1,i+(e.key==='ArrowDown'?1:-1)));rows[next].focus();
  }
 },true);
 document.addEventListener('click',e=>{
  const filter=e.target.closest('[data-filter]');if(filter){state.filter=filter.dataset.filter;syncTaskFilter();$(`[data-filter="${state.filter}"]`).focus({preventScroll:true});}
  const a=e.target.closest('[data-action]')?.dataset.action;
  if(a==='clear-search'){state.query='';$('[data-testid="work-search"]').value='';syncTaskFilter();$('[data-testid="work-search"]').focus();}
  if(a==='clear-filters'){clearTaskFilters();if(innerWidth<=850)$('.sheet').classList.add('show-list');$('[data-testid="work-search"]').focus();}
  if(e.target.closest('[data-snackbar-action]')){const action=snackbarState.action;hideSnackbar();action?.();}
  if(e.target.closest('[data-snackbar-close]')){hideSnackbar();if($('#work-drawer').open)$('[data-testid="thread-input"]')?.focus({preventScroll:true});}
 });
 document.addEventListener('input',e=>{
  if(e.target.matches('.scope-search'))renderScopeResults(e.target.value);
  if(e.target.matches('[data-testid="work-search"]')){state.query=e.target.value;syncTaskFilter();}
 });
}
