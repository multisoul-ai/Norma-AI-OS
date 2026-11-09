const ICONS = {
  mic:'<rect x="9" y="2" width="6" height="12" rx="3"/><path d="M5 10v2a7 7 0 0 0 14 0v-2M12 19v3M8 22h8"/>',
  sliders:'<path d="M4 6h5m4 0h7M4 12h10m4 0h2M4 18h3m4 0h9"/><circle cx="11" cy="6" r="2"/><circle cx="16" cy="12" r="2"/><circle cx="9" cy="18" r="2"/>',
  brain:'<path d="M12 5a3 3 0 0 0-6-1 4 4 0 0 0-3 6 4 4 0 0 0 1 7 4 4 0 0 0 8 2V5Zm0 0a3 3 0 0 1 6-1 4 4 0 0 1 3 6 4 4 0 0 1-1 7 4 4 0 0 1-8 2M7 7l2 2M5 13h4M8 18l1-3M17 7l-2 2M19 13h-4M16 18l-1-3"/>',
  arrow:'<path d="m5 12 14 0m-5-5 5 5-5 5"/>', chevron:'<path d="m9 5 7 7-7 7"/>', down:'<path d="m6 9 6 6 6-6"/>',
  check:'<path d="m5 12 4 4L19 6"/>', checkCircle:'<circle cx="12" cy="12" r="9"/><path d="m8 12 3 3 5-6"/>',
  cloud:'<path d="M6 18a5 5 0 0 1-1-10 7 7 0 0 1 13-1 5.5 5.5 0 0 1 0 11Z"/>',
  laptop:'<rect x="4" y="3" width="16" height="13" rx="2"/><path d="m4 16-2 5h20l-2-5M9 19h6"/>',
  speaker:'<path d="m11 4-6 5H2v6h3l6 5V4ZM15 8a6 6 0 0 1 0 8M18 5a10 10 0 0 1 0 14"/>',
  target:'<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="5"/><circle cx="12" cy="12" r="1"/>',
  grid:'<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
  help:'<circle cx="12" cy="12" r="9"/><path d="M9 9a3 3 0 0 1 6 0c0 2-3 2-3 4M12 17h.01"/>',
  settings:'<circle cx="12" cy="12" r="3"/><path d="m9 3-1 3-3 1-2 5 2 5 3 1 1 3h6l1-3 3-1 2-5-2-5-3-1-1-3Z"/>',
  moon:'<path d="M20 15A9 9 0 0 1 9 4a9 9 0 1 0 11 11Z"/>', sun:'<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M2 12h2M20 12h2M5 5l1 1M18 18l1 1M5 19l1-1M18 6l1-1"/>',
  globe:'<circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/>',
  close:'<path d="m6 6 12 12M6 18 18 6"/>', reset:'<path d="M4 10a8 8 0 1 1 2 8M4 4v6h6"/>',
  lock:'<rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V6a4 4 0 0 1 8 0v4"/>',
  file:'<path d="M13 2H5v20h14V8Z M13 2v6h6M8 13h8M8 17h6"/>',
};
function icon(name, cls='') { return `<svg class="${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[name] || ICONS.grid}</svg>`; }
function avatar(agent, large=false) { return `<span class="avatar ${agent.color === 'green' ? 'green' : ''} ${large ? 'large' : ''}">${agent.name[0]}</span>`; }
function selectControl(field, label, menu=field) { return `<button class="select-control" data-action="menu" data-field="${field}" data-menu="${menu}" aria-haspopup="listbox" aria-expanded="false">${esc(label)}${icon('down')}</button>`; }
function row(title, desc, control) { return `<div class="setting-row"><div class="grow"><div class="row-title">${t(title)}</div>${desc ? `<div class="row-desc">${desc}</div>` : ''}</div><div class="row-control">${control}</div></div>`; }
function section(key, number, body) { return `<section class="setting-section"><h2 class="section-title"><span class="section-number">0${number}</span>${t(key)}</h2><div class="section-box">${body}</div></section>`; }
function sourceButtons() { return `<div class="segmented" aria-label="${t('source')}">${['local','cloud'].map(value=>`<button data-action="set" data-field="source" data-value="${value}" aria-pressed="${settings.source===value}">${settings.source===value?icon('check'):''}${t(value)}</button>`).join('')}</div>`; }
function switchControl(field, label) { return `<button class="switch" role="switch" aria-label="${t(label)}" aria-checked="${settings[field]}" data-action="toggle" data-field="${field}"></button>`; }
function inputRows() {
  return row('source',t(settings.source==='local'?'sourceHint':'cloudHint'),sourceButtons())
    +(settings.source==='cloud' ? row('cloudConfigure',t(settings.cloudConfigured?'cloudReadyHint':'unconfigured'),`<button class="text-button" data-action="open" data-panel="cloud">${t('edit')}${icon('chevron')}</button>`) : '')
    +row('interaction','',selectControl('interaction',t(settings.interaction)))
    +(settings.interaction==='single'?row('trigger','',selectControl('trigger',t(settings.trigger))):`<div class="engine-note">${icon('checkCircle')}${t('continuousHint')}</div>`)
    +row('microphone',t('micHint'),selectControl('microphone',t(settings.microphone))+`<button class="icon-button" data-action="open" data-panel="mic-test" aria-label="${t('testMic')}" title="${t('testMic')}">${icon('mic')}</button>`)
    +row('shortcut','',`<button class="select-control" data-action="open" data-panel="shortcut"><kbd>${esc(settings.shortcut)}</kbd>${icon('chevron')}</button>`);
}
function understandingRows() {
  return row('model',t('modelHint'),`<button class="text-button" data-action="open" data-panel="model">${t('modelValue')}${icon('chevron')}</button>`)
    +row('target',t('targetHint'),`<button class="text-button" data-action="open" data-panel="targets"><span class="mini-avatars">${DEMO_AGENTS.map(a=>avatar(a)).join('')}</span>${t('targetValue')}${icon('chevron')}</button>`);
}
function dispatchRows() {
  return row('scope',t('scopeHint'),`<span class="row-value">${t('scopeValue')}</span>`)
    +row('clarify','',selectControl('clarify',t(settings.clarify==='target'?'targetOnly':'wholeCommand')))
    +row('noNew',t('noNewHint'),`<span class="locked">${icon('lock')}${t('guarded')}</span>`);
}
function replyRows() {
  return row('spokenReply',t('spokenHint'),switchControl('spokenReply','spokenReply'))
    +(settings.spokenReply ? row('currentVoice','',`<span class="row-value">${t('followVoice')}</span>`)
      +row('volume','',`<input class="volume" data-field="volume" data-action="range" type="range" min="0" max="100" value="${settings.volume}" aria-label="${t('volume')}"><output class="percent">${settings.volume}%</output>`) : '');
}
const GROUPS=['input','understanding','dispatch','reply'];
function groupRows(name) { return ({input:inputRows,understanding:understandingRows,dispatch:dispatchRows,reply:replyRows})[name](); }
function statusStrip() {
  const runtime=deriveRuntime(settings);
  const stateKey=settings.source==='cloud'&&!settings.cloudConfigured?'unconfigured':settings.status;
  const hint=stateKey==='unconfigured'?'cloudConfigNote':stateKey==='ready'?(settings.source==='local'?'readyHint':'cloudReadyHint'):stateKey==='preparing'?'preparingHint':'failedHint';
  return `<div class="status-strip ${runtime.ready?'':'warn'}">${icon(runtime.ready?'checkCircle':'help')}<div class="grow"><strong>${t(settings.source)}${ui.lang==='zh'?'识别 · ':' recognition · '}${t(stateKey)}</strong><small>${t(hint)}</small></div><button class="text-button" data-action="${stateKey==='failed'?'retry':'open'}" data-panel="advanced">${t(stateKey==='failed'?'retry':'manage')}</button></div>`;
}
function advancedLink() { return `<button class="advanced-link" data-action="open" data-panel="advanced">${icon('sliders')}${t('advanced')}${icon('chevron')}</button>`; }
function renderResult() {
  const proposal=ui.demo.proposal;
  if(proposal.input!==ui.demo.input)return `<div class="demo-result"><div class="result-note warn">${t('pendingPreview')}</div></div>`;
  let target=t('unknown');
  const agent=DEMO_AGENTS.find(a=>a.id===proposal.target);
  if(agent) target=avatar(agent)+esc(agent.name);
  else if(proposal.targetKind==='self') target='Norma';
  else if(proposal.targetKind==='outside') target=t('outside');
  const note=proposal.kind==='handoff'?t('roleHint'):proposal.targetKind==='outside'?t('outsideHint'):proposal.kind==='unsupported'?t('unsupportedHint'):proposal.route==='main'?t('routeHint'):'';
  return `<div class="demo-result"><div class="result-row"><span class="result-label">${t('action')}</span><span class="result-value">${t('action_'+proposal.kind)}</span></div><div class="result-row"><span class="result-label">${t('destination')}</span><span class="result-value">${target}</span></div>${proposal.kind==='clarify'&&proposal.targetKind!=='outside'?`<div class="result-note warn">${t('chooseTarget')}<div class="clarify-actions">${DEMO_AGENTS.map(a=>`<button class="outline-button" data-action="clarify" data-target="${a.id}">${a.name}</button>`).join('')}</div></div>`:''}${note?`<div class="result-note ${proposal.targetKind==='outside'?'warn':''}">${note}</div>`:''}${ui.demo.receipt?`<div class="receipt-note">${icon('checkCircle')}<span>${t('simulatedReceipt')}</span></div>`:''}</div>`;
}
function renderDemo() {
  return `<aside class="demo-panel"><div class="demo-top"><div class="demo-badge"><span class="dot"></span>${t('demoLabel')}</div><h2>${t('testTitle')}</h2><p>${t('testHint')}</p></div><div class="demo-input-wrap"><textarea class="demo-textarea" data-demo-input aria-label="${t('testTitle')}" spellcheck="false">${esc(ui.demo.input)}</textarea><div class="demo-presets">${[1,2,3,4,5].map(i=>`<button class="example-chip" data-action="sample" data-index="${i}">${t('sample'+i)}</button>`).join('')}</div><button class="filled-button" data-action="analyze">${icon('brain')}${t('analyze')}</button></div>${renderResult()}<div class="demo-submit"><button class="outline-button" data-action="demo-submit" ${canSubmit(ui.demo.proposal,settings,ui.demo.input)?'':'disabled'}>${icon('arrow')}${t('simulateSend')}</button></div><div class="demo-canvas"><div class="demo-canvas-heading"><span>${t('currentCanvas')}</span><span>${t('project')}</span></div>${DEMO_AGENTS.map(a=>`<button class="canvas-agent" data-action="open" data-panel="agent-${a.id}">${avatar(a)}<span><span class="name">${a.name}</span><span class="task" style="display:block">${t(a.id==='bruno'?'taskBruno':'taskHenry')}</span></span><span class="dot"></span></button>`).join('')}</div><p class="demo-foot">${t('selectedHint')}</p></aside>`;
}
function pageShell(body) {
  return `<div class="window"><div class="window-bar"><div class="traffic" aria-hidden="true"><span></span><span></span><span></span></div><span class="window-name">Norma · ${t('settings')}</span></div><div class="window-main"><aside class="sidebar"><div class="brand">${icon('brain','brand-symbol')}<span>Norma</span></div><div class="sidebar-group">${t('general')}</div>${[['settings','settings'],['sun','appearance'],['grid','workspace']].map(([i,k])=>`<div class="nav-item" aria-disabled="true">${icon(i)}<span>${t(k)}</span></div>`).join('')}<div class="sidebar-group">${t('assistant')}</div>${[['mic','voice'],['brain','memory'],['target','agents'],['sliders','keyboard']].map(([i,k])=>`<div class="nav-item ${k==='voice'?'active':''}" ${k==='voice'?'aria-current="page"':'aria-disabled="true"'}>${icon(i)}<span>${t(k)}</span></div>`).join('')}<div class="sidebar-foot"><div class="sidebar-project"><span class="project-square">N</span>${t('project')}</div>${t('navNote')}</div></aside><main class="page"><header class="page-head"><div class="grow"><div class="eyebrow">${t('settings')} / ${t('voice')}</div><h1>${t('title')}</h1><p class="subtitle">${t('subtitle')}</p></div><div class="page-tools"><button class="icon-button" data-action="open" data-panel="help" title="${t('help')}" aria-label="${t('help')}">${icon('help')}</button><button class="icon-button" data-action="reset" title="${t('reset')}" aria-label="${t('reset')}">${icon('reset')}</button></div></header><div class="mode-caption">${icon('mic')}<strong>${t('title')}</strong><span>·</span><span>${t(settings.interaction)}</span><kbd>${esc(settings.shortcut)}</kbd></div>${body}</main></div></div>`;
}
function renderGrouped() {
  return pageShell(`${statusStrip()}<div class="settings-layout"><div class="settings-column">${GROUPS.map((name,i)=>section(name,i+1,groupRows(name))).join('')}${advancedLink()}</div>${renderDemo()}</div>`);
}
function renderWorkflow() {
  const index=GROUPS.indexOf(ui.step), name=GROUPS[index];
  return pageShell(`<div class="flow-layout"><nav class="step-rail" aria-label="${t('title')}">${GROUPS.map((key,i)=>`<button class="step-item ${key===name?'active':''}" data-action="step" data-step="${key}" ${key===name?'aria-current="step"':''}><span class="step-index">${i+1}</span><strong>${t(key)}</strong></button>`).join('')}<p class="step-rail-note">${t('simulatedOnly')}<br>${t('targetHint')}</p></nav><div><div class="step-heading"><div class="eyebrow">0${index+1} / 04</div><h2>${t(name)}</h2><p>${t(name+'Desc')}</p></div>${name==='input'?statusStrip():''}<div class="section-box">${groupRows(name)}</div>${advancedLink()}<div class="step-buttons"><button class="outline-button" data-action="step" data-step="${GROUPS[Math.max(0,index-1)]}" ${index===0?'disabled':''}>${t('stepPrev')}</button><button class="filled-button" data-action="step" data-step="${GROUPS[Math.min(3,index+1)]}" ${index===3?'disabled':''}>${t('stepNext')}${icon('arrow')}</button></div></div>${renderDemo()}</div>`);
}
function renderOverview() {
  const summaries=[`${t(settings.source)} · ${t(settings.interaction)} · ${t(settings.microphone)}`,`${t('modelValue')} · Bruno / Henry`,`${t('scopeValue')} · ${t(settings.clarify==='target'?'targetOnly':'wholeCommand')}`,settings.spokenReply?`${t('followVoice')} · ${settings.volume}%`:t('spokenReply')+' · '+(ui.lang==='zh'?'关闭':'Off')];
  return pageShell(`${statusStrip()}<div class="overview-layout"><div><div class="overview-kicker">${t('summaryHint')}</div><div class="overview-card">${GROUPS.map((key,i)=>`<button class="overview-row" data-action="open" data-panel="${key}"><span class="overview-icon">${icon(['mic','brain','target','speaker'][i])}</span><span class="grow"><strong>${t(key)}</strong><p>${esc(summaries[i])}</p></span>${icon('chevron','chevron')}</button>`).join('')}</div><div class="overview-footer">${advancedLink()}<span class="badge">${t('simulatedOnly')}</span></div></div>${renderDemo()}</div>`);
}
function dialogContents(name) {
  if(GROUPS.includes(name)) return {title:t(name),desc:t(name+'Desc'),body:`<div class="section-box">${groupRows(name)}</div>`,drawer:true};
  if(name==='targets') return {title:t('target'),desc:t('targetDialogHint'),body:DEMO_AGENTS.map(a=>`<div class="agent-detail">${avatar(a,true)}<div class="grow"><strong>${a.name}</strong><small>${a.alias} · ${t(a.id==='bruno'?'taskBruno':'taskHenry')}</small></div><span class="badge green">${t('idle')}</span></div>`).join('')+`<div class="role-annotation">${t('targetHint')}<br>${t('scopeHint')}<br>${t('selectedHint')}</div>`};
  if(name.startsWith('agent-')) {const agent=DEMO_AGENTS.find(a=>a.id===name.slice(6));return {title:agent.name,desc:t('currentCanvas')+' · '+t('project'),body:`<div class="agent-detail">${avatar(agent,true)}<div><strong>${t(agent.id==='bruno'?'taskBruno':'taskHenry')}</strong><small>${t('idle')}</small></div></div><p class="dialog-copy">${t('selectedHint')}</p>`};}
  if(name==='advanced'||name==='model') {const runtime=deriveRuntime(settings);return {title:t(name==='model'?'model':'advanced'),desc:t('engineHint'),body:`<div class="inspector-line"><span>${t('engine')}</span><strong>${settings.source==='cloud'?'Azure Speech':t(runtime.engine==='a1'?'previewFinal':'compatibility')}</strong></div><div class="inspector-line"><span>${t('model')}</span><strong>${t('modelValue')} <span>(${t('demoLabel')})</span></strong></div><div class="inspector-line"><span>${t('jevStatus')}</span><strong>${t('notIntegrated')}</strong></div><div class="role-annotation">${t('jevHint')}<br>${t('latencyHint')}</div><button class="text-button" data-action="open" data-panel="targets">${t('target')}${icon('arrow')}</button>`};}
  if(name==='cloud') return {title:t('cloudConfigure'),desc:t('cloudConfigNote'),body:`<div class="row-title">${t('region')}</div><div class="cloud-fake-field">East Asia · ${t('demoLabel')}</div><div class="row-title">API Key</div><div class="cloud-fake-field">${t('placeholderSecret')}</div><button class="filled-button" data-action="configure-cloud">${t('simulateConfig')}</button>`};
  if(name==='mic-test') return {title:t('testMic'),desc:t('micTestNote'),body:`<div class="wave" aria-hidden="true">${Array.from({length:31},(_,i)=>`<span style="height:${9+Math.round((Math.sin(i*.9)+1)*11+(i%7)*2)}px"></span>`).join('')}</div><p class="dialog-copy" style="text-align:center">${t(settings.microphone)} · ${t('demoLabel')}</p><div style="text-align:center;margin-top:20px"><button class="filled-button" data-action="heard">${t('heard')}</button></div>`};
  if(name==='shortcut') return {title:t('shortcut'),desc:t('shortcutHint'),body:`<div class="shortcut-capture"><kbd>${esc(settings.shortcut)}</kbd><p>${t('recordShortcut')}</p></div>`};
  return {title:t('help'),desc:t('prototype'),body:`<div class="dialog-copy"><p>${t('helpText')}</p><p>${t('prototypeNote')}</p><a href="../../exec-plans/2026-10-31-voice-command-redesign-plan.html" target="_blank" rel="noopener">${icon('file')}${t('plan')}</a></div>`};
}
