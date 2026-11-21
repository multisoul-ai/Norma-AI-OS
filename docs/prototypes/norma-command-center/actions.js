function refresh(){
 const scrolls=$$('[data-scroll]').map(el=>[el.dataset.scroll,el.scrollTop]);$('#stage').innerHTML=variants[current]();
 scrolls.forEach(([key,y])=>$(`[data-scroll="${key}"]`)?.scrollTo(0,y));
}
function notify(text){const el=$('#toast');el.textContent=text;if(!el.matches(':popover-open'))el.showPopover();clearTimeout(notify.timer);notify.timer=setTimeout(()=>{if(el.matches(':popover-open'))el.hidePopover();},4000);}
function openAgent(id){
 const a=byId(id);if(!a)return;openAgentId=id;const c=context(a),d=$('#agent-dialog');
 d.innerHTML=modalHead(`${av(a)} ${a.agent}`, 'agent-title',`${spaceName(a.space)} / ${projects[a.project].name} · ${t('sameSession')}`)+`<div class="agent-detail-title"><h3>${esc(l(a.title))}</h3>${status(a)}</div><div class="agent-conversation"><div class="message user">${esc(l(a.initial))}</div><div class="message agent"><b>${a.agent}</b><p>${esc(l(a.answer))}</p></div>${a.recommendation&&a.status==='needs'?`<div class="agent-recommend">${orb}<strong>${tr('Norma 的建议','Norma recommends')}</strong><p>${esc(l(a.recommendation))}</p><button class="button primary" data-accept-agent="${a.id}">${t(a.id==='navigation'?'review':'apply')}</button></div>`:''}${c.messages.map(m=>`<div class="message ${m.role}">${m.role!=='user'?`<b>${a.agent} · ${tr('示例回复','Sample reply')}</b>`:''}<p>${esc(m.text)}</p></div>`).join('')}</div><form data-agent-form><label>${tr('继续原会话','Continue the original session')}</label><div class="agent-input"><textarea data-testid="agent-input" rows="2" placeholder="${tr('继续和','Continue with')} ${a.agent}…">${esc(c.draft)}</textarea><button class="send-circle" type="submit" aria-label="${t('send')}">${icon('up')}</button></div></form><footer class="modal-footer"><button class="button" data-action="agent-locate" data-testid="agent-locate">${icon('out')}${t('locate')}</button><span>${t('local')}</span></footer>`;
 if(!d.open)d.showModal();
}
function openDispatch(ids=[],text='',options={}){
 dispatchSpec={ids:[...ids],text,source:options.source||null,room:options.room||null,helper:!!options.helper};renderDispatch();$('#dispatch-dialog').showModal();
}
function renderDispatch(){
 const d=$('#dispatch-dialog'),spec=dispatchSpec,source=spec.source?byId(spec.source):null,targets=spec.ids.map(byId).filter(Boolean);
 d.innerHTML=modalHead(tr('把方向交给正确的人','Give direction to the right agents'),'dispatch-title',tr('确认目标和内容，每位 Agent 在原卡片中接收指令。','Review recipients and content. Every command goes to the original card.'))+`${source?`<div class="handoff-source">${icon('link')}<div><small>${tr('上下文来自','Context from')}</small><b>${source.agent} · ${esc(l(source.title))}</b><small>${spaceName(source.space)} / ${projects[source.project].name}</small><p>${esc(l(source.summary))}</p></div></div>`:''}${spec.helper?`<label class="field-label">${tr('由谁协助','Who should help')}<select id="helper-target"><option value="">${t('pickAgents')}</option>${S().agents.filter(a=>a.id!==spec.source).map(a=>`<option value="${a.id}" ${spec.ids.includes(a.id)?'selected':''}>${a.agent} · ${spaceName(a.space)} / ${projects[a.project].name}</option>`).join('')}</select></label>`:`<div class="dispatch-scope"><span>${t('pickAgents')}</span><button class="text-button" data-action="toggle-recipients">${tr('修改接收者','Edit recipients')}</button></div><div class="recipient-options" ${spec.ids.length?'hidden':''}>${scopeAgents().map(a=>`<label>${av(a)}<span>${a.agent}<small>${spaceName(a.space)} / ${projects[a.project].name}</small></span><input type="checkbox" data-recipient="${a.id}" ${spec.ids.includes(a.id)?'checked':''}></label>`).join('')}</div>`}<div class="dispatch-targets">${targets.map(a=>`<div data-target="${a.id}">${av(a)}<div><b>${a.agent}</b><span>${spaceName(a.space)} / ${projects[a.project].name}</span></div>${icon('check')}</div>`).join('')}</div><label class="field-label">${t('goal')}<textarea rows="3" data-testid="dispatch-input">${esc(spec.text)}</textarea></label><div class="dispatch-explanation">${orb}<p>${tr('这是一次明确的派工。只发送给上面列出的卡片，不会广播到同名 Agent。','This dispatch goes only to the cards listed above, never to other agents with the same name.')}</p></div><footer class="modal-footer"><span>${targets.length} ${tr('个接收者','recipients')} · ${tr('本地演示','Local simulation')}</span><button class="button primary" data-action="confirm-dispatch" data-testid="confirm-dispatch" ${targets.length&&spec.text.trim()?'':'disabled'}>${t('dispatch')}${icon('arrow')}</button></footer>`;
}
function confirmDispatch(){
 const spec=dispatchSpec,targets=spec.ids.map(byId).filter(Boolean);if(!targets.length||!spec.text.trim())return;
 const text=spec.text.trim(),source=spec.source?byId(spec.source):null;
 const reference=source?{identity:identity(source),agent:source.agent,title:l(source.title),summary:l(source.summary)}:null;
 const deliveredText=source?`${tr('上下文引用','Context reference')}: ${source.agent} · ${spaceName(source.space)} / ${projects[source.project].name}\n${l(source.title)}\n${l(source.summary)}\n\n${text}`:text;
 for(const a of targets){context(a).messages.push({role:'user',text:deliveredText,source:reference},{role:'agent',text:tr('指令已记录在这张卡片的原会话中。这里为本地交互演示。','Direction recorded in this card’s original session. This is a local prototype.')});a.lastCommand=text;}
 S().dispatches.push({ids:targets.map(a=>a.id),text,source:reference});
 if(spec.room){S().roomStates[spec.room].events.push({names:targets.map(a=>a.agent).join(', '),text});S().roomStates[spec.room].draft='';}
 else S().draft='';
 $('#dispatch-dialog').innerHTML=modalHead(tr('方向已送达，逐位可追踪。','Direction recorded. Each recipient is traceable.'),'dispatch-title',tr('以下为本地模拟回执，未启动真实执行。','These are local receipts. No real execution has started.'))+`<div class="receipt-summary">${icon('check')}<div><h3>${targets.length}/${targets.length} ${t('ack')}</h3><p>${esc(text)}</p></div></div><div class="receipt-list">${targets.map(a=>`<div data-receipt="${a.id}">${av(a)}<div><b>${a.agent}</b><small>${spaceName(a.space)} / ${projects[a.project].name}</small></div><span>${t('ack')}</span></div>`).join('')}</div><footer class="modal-footer"><span>${tr('工作仍保留各自的执行状态','Execution state remains with each card')}</span><button class="button primary" data-action="close-dialog">${tr('回到指挥页','Back to command')}${icon('arrow')}</button></footer>`;refresh();
}
function prepareCommand(){openDispatch(selectedAgents().map(a=>a.id),S().draft);}
function applyDecision(id){const a=byId(id);if(a.status!=='needs')return;a.status=id==='navigation'?'merge':'running';a.summary=id==='navigation'?['验收已记录，等待合并','Review recorded, awaiting merge']:['方向已明确，继续执行','Direction set, continuing'];context(a).messages.push({role:'user',text:l(a.recommendation)||tr('请继续推进','Please continue')});S().decisionResult=`${a.agent} · ${t(a.status)}`;S().decision=decisionQueue()[0]?.id;refresh();}
function openQueue(id,lane){const a=byId(id),q=S().queues[id];const d=$('#utility-dialog');d.dataset.queue=id;d.innerHTML=modalHead(`${a.agent} · ${tr('安排下一步','Plan the next step')}`,'queue-title',tr('这里调整计划，不改变当前执行状态。','This edits a plan, not the current execution state.'))+`<label class="field-label">${tr('下一项工作','Next work item')}<textarea id="queue-text" rows="3">${esc(q?l(q.text):'')}</textarea></label><div class="schedule-options">${['next','later'].map(k=>`<button class="button ${k===(lane||q?.lane)?'primary':''}" data-schedule="${k}">${icon('clock')}${t(k)}</button>`).join('')}</div><p class="small-note">${tr('点击“接下来”或“稍后”保存计划。派发仍需你的明确操作。','Choose Next or Later to save. Dispatch still needs an explicit command.')}</p>`;d.showModal();}
function runtimeControl(action){
 const results=selectedAgents().map(a=>{const eligible=action==='pause'?a.status==='running':a.status==='paused';if(eligible)a.status=action==='pause'?'paused':'running';return `${a.agent} · ${eligible?t(a.status):tr('跳过：当前状态不适用','Skipped: not applicable')}`;});
 refresh();notify(results.join('  /  '));
}
function showCanvas(id=null){
 canvasSource={agent:$('#agent-dialog').open?openAgentId:null};if($('#agent-dialog').open)$('#agent-dialog').close();
 const d=$('#canvas-dialog'),list=scopeAgents(),a=id?byId(id):null;if(a&&!list.includes(a))list.push(a);
 d.innerHTML=`<header class="canvas-header">${orb}<strong>${tr('原画布 · Agent 执行现场','Original canvas · Agent execution')}</strong><button class="button" data-action="canvas-return" data-testid="canvas-return">${icon('undo')}${t('return')}</button></header><div class="canvas-view"><div class="canvas-world">${list.map((w,i)=>`<article class="original-card ${w.id===id?'focused':''}" data-canvas-card="${w.id}" style="left:${(i%3)*470}px;top:${Math.floor(i/3)*350}px"><header><span>Agent</span><b>${w.agent}</b><small>${spaceName(w.space)} / ${projects[w.project].name}</small>${status(w)}</header><h3>${esc(l(w.title))}</h3><div class="original-terminal">${(w.terminal||[['text',l(w.answer)],['muted',l(w.summary)]]).map(([c,text])=>`<p class="${c}">${esc(text)}</p>`).join('')}</div><button class="button" data-canvas-chat="${w.id}">${t('chat')}${icon('chat')}</button></article>`).join('')}</div></div>`;d.showModal();
 if(a){const node=$(`[data-canvas-card="${id}"]`),viewport=$('.canvas-view');$('.canvas-world').style.transform=`translate(${(viewport.clientWidth-440)/2-parseFloat(node.style.left)}px,${Math.max(35,(viewport.clientHeight-320)/2)-parseFloat(node.style.top)}px)`;}
}
function returnCanvas(){const source=canvasSource;$('#canvas-dialog').close();if(source?.agent)openAgent(source.agent);}
function compare(){
 const d=$('#utility-dialog');d.innerHTML=modalHead(tr('五种形态，五种指挥方式。','Five forms. Five ways to direct.'),'compare-title',tr('用同一组 Agent 比较布局与交互，选更符合你习惯的一种。','Compare the same agents through different layouts and interactions.'))+`<div class="comparison-list">${variantInfo.map((v,i)=>`<button data-preview-variant="${i}"><span>0${i+1}</span><div><h3>${l(v.name)}</h3><b>${l(v.axis)}</b><p>${l(v.fit)}</p><small>${tr('取舍：','Tradeoff: ')}${l(v.cost)}</small></div>${icon('arrow')}</button>`).join('')}</div>`;d.showModal();
}
function choose(){preference=current+1;try{localStorage.setItem('norma-command-preference',preference);}catch{}refresh();notify(tr('已标记你喜欢的方向：','Preference saved: ')+l(variantInfo[current].name));}
function openRoster(){const d=$('#utility-dialog');d.innerHTML=modalHead(tr('所有 Agent，随时接管。','Every agent, within reach.'),'roster-title')+`<div class="full-roster">${scopeAgents().map(a=>rosterRow(a)).join('')}</div>`;d.showModal();}
function toggleSelection(id){S().selection.has(id)?S().selection.delete(id):S().selection.add(id);refresh();}
function closeDialog(el){el.closest('dialog')?.close();}
function initActions(){
 document.addEventListener('click',e=>{
  const el=e.target.closest('button');if(!el)return;
  if(el.dataset.chat){if($('#utility-dialog').open)$('#utility-dialog').close();openAgent(el.dataset.chat);return;}
  if(el.dataset.review){openAgent(el.dataset.review);return;}
  if(el.dataset.mapNode){S().map=el.dataset.mapNode;S().dependencies=false;refresh();return;}
  if(el.dataset.restoreDecision){const id=el.dataset.restoreDecision;S().deferred.delete(id);S().decision=id;S().decisionResult='';if(S().lastDeferred===id)S().lastDeferred=null;refresh();return;}
  if(el.dataset.decision){S().decision=el.dataset.decision;S().decisionResult='';refresh();return;}
  if(el.dataset.editDecision){openDispatch([el.dataset.editDecision],l(byId(el.dataset.editDecision).recommendation));return;}
  if(el.dataset.acceptAgent){applyDecision(el.dataset.acceptAgent);openAgent(el.dataset.acceptAgent);return;}
  if(el.dataset.handoff){openDispatch([],tr('请接续相关上下文，协助推进这项工作。','Please take the linked context and help move this work forward.'),{source:el.dataset.handoff,helper:true});return;}
  if(el.dataset.locate){showCanvas(el.dataset.locate);return;}
  if(el.dataset.toggleTarget){toggleSelection(el.dataset.toggleTarget);return;}
  if(el.dataset.agentFilter){S().filter=el.dataset.agentFilter;refresh();return;}
  if(el.dataset.queue){openQueue(el.dataset.queue);return;}
  if(el.dataset.addQueue){openQueue(el.dataset.addQueue,el.dataset.lane);return;}
  if(el.dataset.schedule){const id=$('#utility-dialog').dataset.queue,text=$('#queue-text').value.trim();if(!text){$('#queue-text').focus();return;}S().queues[id]={lane:el.dataset.schedule,text};$('#utility-dialog').close();refresh();notify(tr('计划已保存，未改变运行状态','Plan saved; execution state unchanged'));return;}
  if(el.dataset.room){S().room=el.dataset.room;refresh();return;}
  if(el.dataset.roomTarget){S().roomStates[S().room].target=el.dataset.roomTarget;refresh();$('[data-testid="room-input"]').focus();return;}
  if(el.dataset.previewVariant){$('#utility-dialog').close();setActive(Number(el.dataset.previewVariant));return;}
  if(el.dataset.canvasChat){$('#canvas-dialog').close();openAgent(el.dataset.canvasChat);return;}
  const a=el.dataset.action;if(!a)return;
  if(a==='compare')compare();if(a==='choose')choose();
  if(a==='language'){lang=lang==='zh'?'en':'zh';$('.proto-picker').setAttribute('aria-label',tr('原型方案','Prototype forms'));$('#canvas-dialog').setAttribute('aria-label',tr('画布现场','Canvas view'));items.forEach((b,i)=>b.textContent=l(variantInfo[i].name));moveHighlight();refresh();}
  if(a==='video'){const v=$('#wallpaper');v.paused?v.play().catch(()=>{}):v.pause();}
  if(a==='reset'){statePool[current]=createState();refresh();notify(tr('本方案已重置','This form has been reset'));}
  if(a==='new-command'||a==='selection-command')prepareCommand();
  if(a==='agent-locate')showCanvas(openAgentId);if(a==='canvas')showCanvas();if(a==='canvas-return')returnCanvas();
  if(a==='close-dialog')closeDialog(el);if(a==='confirm-dispatch')confirmDispatch();
  if(a==='toggle-recipients')$('.recipient-options').hidden=!$('.recipient-options').hidden;
  if(a==='dependencies'){S().dependencies=!S().dependencies;refresh();}
  if(a==='help')openDispatch([],tr('请协助对齐上游结果，让当前工作可以继续。','Help align the upstream result so this work can continue.'),{source:S().map,helper:true});
  if(a==='accept-decision'){const w=decisionAgent();if(w)applyDecision(w.id);}
  if(a==='defer-decision'){const w=decisionAgent();if(w){S().deferred.add(w.id);S().lastDeferred=w.id;S().decision=decisionQueue()[0]?.id;refresh();}}
  if(a==='undo-defer'){S().deferred.delete(S().lastDeferred);S().decision=S().lastDeferred;S().lastDeferred=null;refresh();}
  if(a==='pause-selected')runtimeControl('pause');if(a==='resume-selected')runtimeControl('resume');if(a==='room-roster')openRoster();
 });
 document.addEventListener('change',e=>{
  if(e.target.matches('[data-select]')){toggleSelection(e.target.dataset.select);return;}
  if(e.target.matches('[data-select-all]')){visibleAgents().forEach(a=>e.target.checked?S().selection.add(a.id):S().selection.delete(a.id));refresh();}
  if(e.target.id==='scope-space'){S().space=e.target.value;S().project='all';S().selection.clear();S().filter='all';refresh();}
  if(e.target.id==='scope-project'){S().project=e.target.value;S().selection.clear();refresh();}
  if(e.target.id==='table-filter'){S().filter=e.target.value;refresh();}
  if(e.target.matches('[data-recipient]')){const id=e.target.dataset.recipient;dispatchSpec.ids=e.target.checked?[...dispatchSpec.ids,id]:dispatchSpec.ids.filter(x=>x!==id);renderDispatch();$('.recipient-options').hidden=false;}
  if(e.target.id==='helper-target'){dispatchSpec.ids=e.target.value?[e.target.value]:[];renderDispatch();}
 });
 document.addEventListener('input',e=>{
  if(e.target.matches('[data-global-input]'))S().draft=e.target.value;
  if(e.target.matches('[data-testid="agent-input"]'))context(byId(openAgentId)).draft=e.target.value;
  if(e.target.matches('[data-testid="dispatch-input"]')){dispatchSpec.text=e.target.value;$('[data-testid="confirm-dispatch"]').disabled=!dispatchSpec.text.trim()||!dispatchSpec.ids.length;}
  if(e.target.matches('[data-testid="room-input"]'))S().roomStates[S().room].draft=e.target.value;
  if(e.target.matches('[data-table-search]')){const value=e.target.value,pos=e.target.selectionStart;S().search=value;refresh();const input=$('[data-table-search]');input.focus();input.setSelectionRange(pos,pos);}
 });
 document.addEventListener('submit',e=>{e.preventDefault();
  if(e.target.matches('[data-command-form]'))prepareCommand();
  if(e.target.matches('[data-agent-form]')){const a=byId(openAgentId),c=context(a);if(!c.draft.trim())return;c.messages.push({role:'user',text:c.draft.trim()},{role:'agent',text:t('reply')});c.draft='';openAgent(a.id);$('.agent-conversation').scrollTop=$('.agent-conversation').scrollHeight;$('[data-testid="agent-input"]').focus();}
  if(e.target.matches('[data-room-form]')){const rs=S().roomStates[S().room],ids=rs.target==='norma'?roomMembers().map(a=>a.id):roomMembers().filter(a=>a.id===rs.target).map(a=>a.id);openDispatch(ids,rs.draft,{room:S().room});}
 });
 document.addEventListener('keydown',e=>{if(e.isComposing)return;if((e.metaKey||e.ctrlKey)&&e.key==='Enter'&&e.target.closest('form')){e.preventDefault();e.target.closest('form').requestSubmit();}});
 let dragId=null;document.addEventListener('dragstart',e=>{const el=e.target.closest('[data-queue]');if(el){dragId=el.dataset.queue;e.dataTransfer.setData('text/plain',dragId);}});
 document.addEventListener('dragover',e=>{const cell=e.target.closest('[data-drop-agent]');if(cell&&cell.dataset.dropAgent===dragId){e.preventDefault();cell.classList.add('drag-target');}});
 document.addEventListener('dragleave',e=>e.target.closest('[data-drop-agent]')?.classList.remove('drag-target'));
 document.addEventListener('drop',e=>{const cell=e.target.closest('[data-drop-agent]');if(cell&&cell.dataset.dropAgent===dragId){e.preventDefault();S().queues[dragId].lane=cell.dataset.dropLane;refresh();notify(tr('仅调整了下一步计划','Only the next-step plan changed'));}dragId=null;});
 $('#canvas-dialog').addEventListener('cancel',e=>{e.preventDefault();returnCanvas();});
 const syncVideo=()=>{const b=$('[data-action="video"]'),v=$('#wallpaper');if(b){b.innerHTML=icon(v.paused?'play':'pause');b.setAttribute('aria-pressed',String(!v.paused));}};
 $('#wallpaper').addEventListener('play',syncVideo);$('#wallpaper').addEventListener('pause',syncVideo);
 const reduce=matchMedia('(prefers-reduced-motion: reduce)');const sync=()=>{if(reduce.matches)$('#wallpaper').pause();};reduce.addEventListener('change',sync);sync();
}
