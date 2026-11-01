let settings={...DEFAULT_SETTINGS};
const ui={lang:'zh',theme:'dark',step:'input',dialog:null,toastTimer:null,demo:{input:'Bruno，继续刚才的登录任务',proposal:interpretDemo('Bruno，继续刚才的登录任务'),receipt:false}};
try {ui.theme=localStorage.getItem('voice-prototype-theme')==='light'?'light':'dark';ui.lang=localStorage.getItem('voice-prototype-lang')==='en'?'en':'zh';} catch {}
if(ui.lang==='en'){ui.demo.input='Bruno, continue the login task';ui.demo.proposal=interpretDemo(ui.demo.input);}
document.documentElement.dataset.theme=ui.theme;
document.documentElement.lang=ui.lang==='en'?'en':'zh-CN';
const variants=[renderGrouped,renderWorkflow,renderOverview];
const editor=document.getElementById('editor');
const popover=document.getElementById('popover');
let menuState=null;
let restoreFocus=null;

function repaint(){closeMenu(false);mount(current);updateHarness();}
function updateHarness(){
  const host=document.getElementById('harness');
  host.innerHTML=`<span class="badge">${t('prototype')}</span><span class="harness-label">Material · Norma</span><span class="harness-axis">${t('axis'+current)}</span><div class="harness-controls"><a href="../../exec-plans/2026-10-31-voice-command-redesign-plan.html" target="_blank" rel="noopener">${icon('file')}${t('plan')}</a><button data-action="language" title="${t('language')}">${icon('globe')}<span>${t('language')}</span></button><button data-action="theme" title="${t('theme')}" aria-label="${t('theme')}">${icon(ui.theme==='dark'?'sun':'moon')}</button><label class="status-demo"><span class="faint">${t('statusDemo')}</span> <select id="demo-status" aria-label="${t('statusDemo')}" style="background:var(--surface-high);color:var(--text);border:1px solid var(--line);border-radius:6px;padding:5px 6px;font-size:11px">${['ready','preparing','failed'].map(key=>`<option value="${key}" ${key===settings.status?'selected':''}>${t(key)}</option>`).join('')}</select></label></div>`;
  document.getElementById('prototype-note').textContent=t('prototypeNote');
  document.querySelectorAll('.proto-picker-item:not(.proto-picker-replay)').forEach((item,i)=>item.textContent=t(['grouped','workflow','overview'][i]));
  document.title=t('title')+' · '+t('prototype');
  requestAnimationFrame(()=>moveHighlight());
}
function notify(key){
  const host=document.getElementById('snackbar');
  host.textContent=t(key);clearTimeout(ui.toastTimer);
  ui.toastTimer=setTimeout(()=>{host.textContent='';},2200);
}
function populateDialog(){
  closeMenu(false);
  const info=dialogContents(ui.dialog);
  editor.className=info.drawer?'drawer':'';
  editor.innerHTML=`<header class="dialog-head"><div class="grow"><h2 id="dialog-title">${info.title}</h2><p>${info.desc}</p></div><button class="icon-button" data-action="close" aria-label="${t('close')}">${icon('close')}</button></header><div class="dialog-body">${info.body}</div><footer class="dialog-footer"><span class="grow faint" style="font-size:10px;align-self:center">${t('simulatedOnly')}</span><button class="text-button" data-action="close">${t('done')}</button></footer>`;
}
function openDialog(name){
  ui.dialog=name;
  populateDialog();
  if(!editor.open)editor.showModal();
  editor.querySelector('button')?.focus();
}
function closeDialog(){
  const previousPanel=ui.dialog;closeMenu(false);ui.dialog=null;editor.close();
  restoreFocus=`[data-action="open"][data-panel="${previousPanel}"]`;repaint();
}
function menuOptions(field){
  return ({interaction:[['single','single'],['continuous','continuous']],trigger:[['hold','hold'],['toggle','toggle']],microphone:[['builtin','builtin'],['airpods','airpods'],['system','system']],clarify:[['target','targetOnly'],['whole','wholeCommand']]})[field] || [];
}
function closeMenu(focus=true){
  if(!menuState)return;
  const anchor=menuState.anchor;anchor.setAttribute('aria-expanded','false');
  menuState=null;popover.hidden=true;document.body.appendChild(popover);
  if(focus&&anchor.isConnected)anchor.focus();
}
function renderOptions(){
  const all=menuOptions(menuState.field);
  const query=popover.querySelector('input').value.toLowerCase();
  menuState.filtered=all.filter(([,label])=>t(label).toLowerCase().includes(query));
  menuState.index=Math.max(0,Math.min(menuState.index,menuState.filtered.length-1));
  const list=popover.querySelector('.popover-list');
  list.innerHTML=menuState.filtered.length?menuState.filtered.map(([value,label],index)=>`<button role="option" id="choice-${index}" class="menu-option" data-action="choose" data-value="${value}" aria-selected="${settings[menuState.field]===value}" ${index===menuState.index?'data-browse':''}>${t(label)}${settings[menuState.field]===value?icon('check'):''}</button>`).join(''):`<div class="empty-menu">${t('noResults')}</div>`;
  const input=popover.querySelector('input');
  if(menuState.filtered.length)input.setAttribute('aria-activedescendant','choice-'+menuState.index);else input.removeAttribute('aria-activedescendant');
  list.querySelector('[data-browse]')?.scrollIntoView({block:'nearest'});
}
function openMenu(anchor){
  if(menuState?.anchor===anchor){closeMenu();return;}
  closeMenu(false);
  const field=anchor.dataset.field;
  menuState={anchor,field,index:Math.max(0,menuOptions(field).findIndex(([value])=>value===settings[field])),filtered:[]};
  (anchor.closest('dialog')||document.body).appendChild(popover);
  popover.innerHTML=`<div class="popover-search"><input aria-label="${t('search')}" role="combobox" aria-controls="voice-options" aria-expanded="true" placeholder="${t('search')}" autocomplete="off"></div><div class="popover-list" id="voice-options" role="listbox" aria-label="${t(field)}"></div>`;
  const rect=anchor.getBoundingClientRect();
  const width=Math.min(292,innerWidth-24);
  popover.style.width=width+'px';
  popover.style.left=Math.max(12,Math.min(innerWidth-width-12,rect.right-width))+'px';
  popover.style.top=Math.max(12,Math.min(innerHeight-310,rect.bottom+7))+'px';
  popover.hidden=false;anchor.setAttribute('aria-expanded','true');renderOptions();popover.querySelector('input').focus();
}
function changeSetting(field,value,focusKey=null){
  if(!Object.hasOwn(DEFAULT_SETTINGS,field))return;
  settings[field]=value;ui.demo.receipt=false;
  if(ui.dialog){populateDialog();} else {restoreFocus=focusKey;repaint();}
  notify('changed');
}
function choose(value){
  const field=menuState.field;
  if(!menuOptions(field).some(([candidate])=>candidate===value))return;
  closeMenu();changeSetting(field,value,`button[data-action="menu"][data-field="${field}"]`);
  if(editor.open)editor.querySelector(`button[data-field="${field}"]`)?.focus();
}

document.addEventListener('pointerdown',event=>{
  if(menuState&&!popover.contains(event.target)&&!menuState.anchor.contains(event.target))closeMenu(false);
});
document.addEventListener('input',event=>{
  if(event.target.matches('[data-demo-input]')){
    ui.demo.input=event.target.value;ui.demo.receipt=false;
    const panel=event.target.closest('.demo-panel');
    panel.querySelector('[data-action="demo-submit"]').disabled=true;
    panel.querySelector('.demo-result').innerHTML=`<div class="result-note warn">${t('pendingPreview')}</div>`;
  }
  if(menuState&&event.target===popover.querySelector('input')){menuState.index=0;renderOptions();}
  if(event.target.matches('input[data-action="range"]')){settings.volume=Number(event.target.value);event.target.nextElementSibling.textContent=settings.volume+'%';}
});
document.addEventListener('change',event=>{
  if(event.target.id==='demo-status'){settings.status=event.target.value;ui.demo.receipt=false;repaint();}
  if(event.target.matches('input[data-action="range"]'))notify('changed');
});
document.addEventListener('click',event=>{
  const button=event.target.closest('[data-action]');if(!button||button.disabled)return;
  const {action}=button.dataset;
  if(action==='open')openDialog(button.dataset.panel);
  else if(action==='close')closeDialog();
  else if(action==='menu')openMenu(button);
  else if(action==='choose')choose(button.dataset.value);
  else if(action==='set')changeSetting(button.dataset.field,button.dataset.value,`[data-action="set"][data-field="${button.dataset.field}"][data-value="${button.dataset.value}"]`);
  else if(action==='toggle')changeSetting(button.dataset.field,!settings[button.dataset.field],`[data-action="toggle"][data-field="${button.dataset.field}"]`);
  else if(action==='step'){ui.step=button.dataset.step;repaint();}
  else if(action==='sample'){ui.demo.input=t('command'+button.dataset.index);ui.demo.proposal=interpretDemo(ui.demo.input);ui.demo.receipt=false;repaint();}
  else if(action==='analyze'){ui.demo.proposal=interpretDemo(ui.demo.input);ui.demo.receipt=false;repaint();}
  else if(action==='clarify'){ui.demo.proposal=interpretDemo(ui.demo.input,button.dataset.target);ui.demo.receipt=false;repaint();}
  else if(action==='demo-submit'){if(canSubmit(ui.demo.proposal,settings,ui.demo.input)){ui.demo.receipt=true;repaint();notify('simulatedReceipt');}}
  else if(action==='configure-cloud'){settings.cloudConfigured=true;settings.status='ready';closeDialog();notify('changed');}
  else if(action==='heard')notify('testPassed');
  else if(action==='retry'){settings.status='ready';repaint();notify('simulatedOnly');}
  else if(action==='reset'){settings={...DEFAULT_SETTINGS};ui.demo={input:t('command1'),proposal:interpretDemo(t('command1')),receipt:false};repaint();notify('resetHint');}
  else if(action==='theme'){ui.theme=ui.theme==='dark'?'light':'dark';document.documentElement.dataset.theme=ui.theme;try{localStorage.setItem('voice-prototype-theme',ui.theme);}catch{}updateHarness();}
  else if(action==='language'){ui.lang=ui.lang==='zh'?'en':'zh';document.documentElement.lang=ui.lang==='zh'?'zh-CN':'en';ui.demo={input:t('command1'),proposal:interpretDemo(t('command1')),receipt:false};try{localStorage.setItem('voice-prototype-lang',ui.lang);}catch{}repaint();if(editor.open)populateDialog();}
});
editor.addEventListener('cancel',event=>{event.preventDefault();closeDialog();});
editor.addEventListener('click',event=>{if(event.target===editor){const box=editor.getBoundingClientRect();if(event.clientX<box.left||event.clientX>box.right||event.clientY<box.top||event.clientY>box.bottom)closeDialog();}});
document.addEventListener('keydown',event=>{
  if(menuState){
    if(event.key==='Escape'){event.preventDefault();event.stopImmediatePropagation();closeMenu();return;}
    if(event.key==='Tab'){closeMenu();return;}
    if(['ArrowDown','ArrowUp','Enter'].includes(event.key)){
      event.preventDefault();event.stopImmediatePropagation();
      if(event.key==='Enter'){const option=menuState.filtered[menuState.index];if(option)choose(option[0]);}
      else {const size=menuState.filtered.length;if(size){menuState.index=(menuState.index+(event.key==='ArrowDown'?1:-1)+size)%size;renderOptions();}}
      return;
    }
  }
  if(editor.open){
    if(ui.dialog==='shortcut'&&(event.metaKey||event.ctrlKey||event.altKey)&&event.key.length===1){
      event.preventDefault();event.stopImmediatePropagation();
      settings.shortcut=[event.metaKey?'⌘':'',event.ctrlKey?'⌃':'',event.altKey?'⌥':'',event.shiftKey?'⇧':'',event.key.toUpperCase()].filter(Boolean).join(' ');populateDialog();return;
    }
    if(event.key==='Escape'){event.preventDefault();event.stopImmediatePropagation();closeDialog();return;}
    if(!/^(INPUT|TEXTAREA|SELECT)$/.test(event.target.tagName))event.stopImmediatePropagation();
    return;
  }
  if(event.key==='ArrowDown'&&event.target.matches('button[data-action="menu"]')){event.preventDefault();event.stopImmediatePropagation();openMenu(event.target);}
},true);
window.addEventListener('resize',()=>closeMenu(false));
new MutationObserver(()=>{
  if(restoreFocus){document.querySelector('#stage '+restoreFocus)?.focus({preventScroll:true});restoreFocus=null;}
  updateHarness();
}).observe(document.getElementById('stage'),{childList:true});
