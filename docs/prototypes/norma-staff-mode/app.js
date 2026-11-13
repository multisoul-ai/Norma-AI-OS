const $ = s => document.querySelector(s);
const esc = s => String(s).replace(/[&<>"]/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const I = {
 bell:'<path d="M6 8a6 6 0 0 1 12 0c0 7 3 8 3 8H3s3-1 3-8M10 20a2 2 0 0 0 4 0"/>',
 spark:'<path d="M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8z"/>',
 clock:'<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
 merge:'<circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="12" r="2"/><path d="M6 7v10M8 5h4a4 4 0 0 1 4 4v1"/>',
 check:'<path d="M5 12.5l4.5 4.5L19 7.5"/>',plus:'<path d="M12 5v14M5 12h14"/>',search:'<circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/>',
 chev:'<path d="M6 9l6 6 6-6"/>',full:'<path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5"/>',unfull:'<path d="M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5"/>',
 out:'<path d="M7 17L17 7M8 7h9v9"/>',file:'<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5"/>',up:'<path d="M12 19V5M6 11l6-6 6 6"/>',
 cube:'<path d="M12 3l8 4.5v9L12 21l-8-4.5v-9zM12 12l8-4.5M12 12v9M12 12L4 7.5"/>',close:'<path d="M6 6l12 12M18 6L6 18"/>'};
const ico = (n, c='') => `<svg class="${c}" viewBox="0 0 24 24">${I[n]}</svg>`;
const STATUS = {needs:'需要你',running:'处理中',waiting:'等待中',merge:'待合并',done:'已完成'};
const GICON = {needs:'bell',running:'spark',waiting:'clock',merge:'merge',done:'check'};
const WST = {Active:'执行中',Reclaimed:'已回收',Failed:'失败',Interrupted:'已中断'};
const WCLS = {Active:'running',Reclaimed:'reclaimed',Failed:'failed',Interrupted:'interrupted'};
const PHASE = {Active:'进行中',Merging:'待合并',Done:'已完成'};
const alive = t => t.workers.filter(w => w.state !== 'Reclaimed');
const hasAsk = t => t.messages.some(m => m.ask && !m.resolved);
// 展示状态由 phase 与存活 Worker 推导，Reclaimed 不参与
function derive(t){
  if (t.phase === 'Done') return 'done';
  if (t.phase === 'Merging') return 'merge';
  const w = alive(t).map(w => w.state);
  return w.includes('Failed') || w.includes('Interrupted') || hasAsk(t) ? 'needs' : w.includes('Active') ? 'running' : 'waiting';
}
let seq = 100;
const reopen = t => { if (t.phase === 'Done') { t.phase = 'Active'; t.messages.push({who:'event',text:'已重新打开：任务回到进行中。'}); } };
const st = {tabs:{}, filesOpen:false, mode:'staff', full:false, sel:'t1', file:null, q:'', f:'all', collapsed:new Set(['done']), drafts:{}};
const task = () => tasks.find(t => t.id === st.sel);
const av = (a, c='') => `<i class="av ${c}" style="background:${AGENTS[a].color}">${a[0]}</i>`;
const orb = c => `<span class="orb ${c||''}"></span>`;
const prog = t => t.workers.length ? `${t.workers.filter(w => w.state === 'Active').length}/${t.workers.length}` : '';

function render(){
  document.body.dataset.mode = st.mode; document.body.classList.toggle('full', st.full);
  const need = tasks.filter(t => derive(t) === 'needs').length, run = tasks.filter(t => derive(t) === 'running').length;
  $('#topbar').innerHTML = `<div class="crumb">${orb()}<span>Norma OS</span><i>/</i><strong>Norma-AI-OS</strong></div><div class="grow"></div>
   <div class="seg" role="tablist" aria-label="视图"><button role="tab" aria-selected="${st.mode==='canvas'}" data-act="mode" data-v="canvas">画布</button>
   <button role="tab" aria-selected="${st.mode==='staff'}" data-act="mode" data-v="staff">幕僚${need?`<b>${need}</b>`:''}</button></div>
   <div class="status">${run} 个处理中 <span style="color:var(--faint)">·</span> ${need} 件需要你</div>`;
  $('#canvas').innerHTML = [...tasks.flatMap(t => t.workers.filter(w => w.cardId).map(w => [w, t])), ...orphans.map(o => [o, null])].map(([c, t], i) => {
    return `<button class="tcard" style="left:${24+(i%3)*494}px;top:${22+Math.floor(i/3)*252}px" data-act="open" data-t="${t?t.id:'orphans'}"><header><span class="pill">Agent</span><b>${c.agent}</b><span>— ${t?esc(t.title):'未归属'}</span><small>${t?WST[c.state]:'未归属'}</small></header><pre><span class="m">&gt; ${esc(t?t.messages[0].text.slice(0,40):c.engine)}</span>\n<span class="p">1.</span> ${esc(c.role)}\n${esc(c.note)}</pre></button>`;
  }).join('');
  $('#dock').innerHTML = `<button class="dk ${st.mode==='staff'?'on':''}" data-act="mode" data-v="${st.mode==='staff'?'canvas':'staff'}">${orb('sm')}Norma 幕僚${need?`<b>${need}</b>`:''}<kbd>⌘ E</kbd></button>`;
  renderStaff(need, run);
}
function renderStaff(need, run){
  const orph = st.sel === 'orphans', t = orph ? {id:'orphans',files:[],messages:[],workers:[],project:'Norma-AI-OS'} : task(), q = st.q.trim().toLowerCase(), s = orph ? 'waiting' : derive(t);
  const match = x => (!q || (x.title + x.workers.map(w => w.agent)).toLowerCase().includes(q)) && (st.f === 'all' || derive(x) === st.f);
  const vis = tasks.filter(match);
  const groups = ORDER.map(k => {
    const items = vis.filter(x => derive(x) === k); if (!items.length) return '';
    const open = !st.collapsed.has(k) || q || st.f !== 'all';
    return `<button class="gh needs-${k} ${k}" data-act="toggle" data-k="${k}" aria-expanded="${!!open}"><i class="gd"></i>${STATUS[k]}<small>${items.length}</small>${ico('chev')}</button>` +
     (open ? items.map(x => `<button class="row ${x.id===st.sel?'sel':''} ${k}" data-act="open" data-t="${x.id}"><span class="ic">${ico(GICON[k])}</span><span class="rt"><strong>${esc(x.title)}</strong><small>${x.workers.length ? x.workers.map(w=>w.agent).join(' · ') : '等待拆解'} · ${x.project}</small></span><span class="prog">${prog(x)}</span><span class="stack">${x.workers.slice(0,3).map(w=>av(w.agent)).join('')}</span></button>`).join('') : '');
  }).join('') || '<p class="empty">没有找到匹配的任务</p>';
  const chips = [['all','全部',tasks.length],['needs','需要你',need],['running','处理中',run]].map(([k,l,n]) => `<button class="chip" data-act="filter" data-k="${k}" aria-pressed="${st.f===k}">${l}<small>${n}</small></button>`).join('');
  const tabs = (st.tabs[t.id] ||= t.files.slice(0, 1).map(f => f.path));
  if (!tabs.includes(st.file)) st.file = tabs[0] || null;
  const name = p => p.split('/').pop();
  const tabBar = tabs.map(p => `<button class="tab ${p===st.file?'on':''}" data-act="tab" data-p="${esc(p)}">${ico('file')}<span>${esc(name(p))}</span><i class="x" data-act="untab" data-p="${esc(p)}" role="button" aria-label="关闭 ${esc(name(p))}">${ico('close')}</i></button>`).join('');
  const list = t.files.map(f => `<button class="fi ${tabs.includes(f.path)?'open':''}" data-act="openfile" data-p="${esc(f.path)}">${ico('file')}<span><b>${esc(name(f.path))}</b><small>${esc(f.path)}</small></span><em class="badge ${f.tag==='新增'?'add':'mod'}">${f.tag}</em></button>`).join('') || '<p class="empty">此任务没有文件改动</p>';
  const body = st.file ? renderFile(st.file) : '<p class="empty">点击 + 或「文件」打开任务改动的文件</p>';

  $('#staff').innerHTML = `
  <header class="sh">${orb()}<span class="name">Norma</span><span class="sub">幕僚</span><div class="grow"></div>
   <button class="pillb">${ico('cube')}Norma OS${ico('chev')}</button><button class="pillb">Norma-AI-OS${ico('chev')}</button>
   <button class="ib" data-act="full" aria-pressed="${st.full}" title="${st.full?'退出全屏 (Esc)':'全屏 (⌃⌘F)'}" aria-label="全屏">${ico(st.full?'unfull':'full')}</button>
   <button class="ib" data-act="mode" data-v="canvas" title="回到画布" aria-label="回到画布">${ico('close')}</button></header>
  <aside class="left col"><button class="new" data-act="new">${ico('plus')}把下一件事，交给 Norma<kbd>⌘N</kbd></button>
   <label class="search">${ico('search')}<input type="search" placeholder="搜索任务或 Agent" value="${esc(st.q)}" aria-label="搜索任务"></label>
   <div class="chips" role="group" aria-label="筛选状态">${chips}</div><div class="groups">${groups}</div><button class="row orph ${orph?'sel':''}" data-act="open" data-t="orphans"><span class="ic">${ico('cube')}</span><span class="rt"><strong>未归属卡片</strong><small>没有任务认领的画布卡片</small></span><span class="prog">${orphans.length}</span></button></aside>
  <section class="mid col">${orph ? orphanView() : `
   <div class="mh"><div class="tx"><span class="tag ${s}">${STATUS[s]}</span><h1>${esc(t.title)}</h1><p>${t.project} · ${t.workers.length} 个 Worker · ${t.updated}更新<label class="phase">阶段<select data-act="phase" aria-label="任务阶段">${Object.keys(PHASE).map(k => `<option value="${k}" ${t.phase===k?'selected':''}>${PHASE[k]}</option>`).join('')}</select></label></p></div>
    <button class="outl" data-act="canvas">${ico('out')}在画布中定位</button></div>
   ${t.workers.length ? `<div class="cards">${t.workers.map((w, i) => workerCard(w, i)).join('')}</div>` : '<p class="nowk">还没有 Worker：等 Norma 给出拆解提议</p>'}
   <div class="msgs" id="msgs"><div class="date">今天 09:41</div>${t.messages.map(m => msg(t, m)).join('')}</div>
   <form class="composer" data-act="send"><div class="to">${ico('up','')}发送到 <b>Norma</b> · ${t.project}</div><div class="box"><textarea rows="1" placeholder="${t.phase==='Done'?'发送新消息会重新打开任务…':'继续和 Norma 沟通…'}" aria-label="给 Norma 的消息">${esc(st.drafts[t.id]||'')}</textarea><button class="send" type="submit" aria-label="发送" ${st.drafts[t.id]?.trim()?'':'disabled'}>${ico('up')}</button></div></form>
   <div class="hint"><span>消息仅用于原型演示</span><span><kbd>⌘</kbd> <kbd>↵</kbd> 发送</span></div>`}</section>
  <aside class="right col"><div class="tabbar"><button class="ib" aria-label="切换侧栏" title="文件列表" data-act="files">${ico('cube')}</button>${tabBar}<button class="ib add" data-act="files" aria-label="打开文件" title="打开文件">${ico('plus')}</button></div>
   ${st.file ? `<div class="fsub">${ico('file')}<span>${esc(name(st.file))}</span><button class="chipf" data-act="files" aria-expanded="${st.filesOpen}">文件 ${t.files.length}</button></div>` : ''}
   ${st.filesOpen ? `<div class="flist">${list}</div>` : ''}<div class="doc">${body}</div></aside>
  <footer class="foot"><i class="dotm"></i>${need} 件需要你<span>·</span>${run} 个处理中<div class="grow"></div><span>各任务独立推进 · 对话留在任务中</span><kbd>esc</kbd></footer>`;
  const m = $('#msgs'); if (m) m.scrollTop = m.scrollHeight;
}
function workerCard(w, i){
  const act = {Active:[['reclaim','回收']], Reclaimed:[['resume','唤起']], Failed:[['redispatch','重派'],['reclaim','回收']], Interrupted:[['redispatch','重派'],['reclaim','回收']]}[w.state];
  return `<div class="cc ${WCLS[w.state]}" data-w="${i}"><span class="t">${av(w.agent,'lg')}${w.agent}<small>${w.engine}</small><em class="st ${WCLS[w.state]}">${WST[w.state]}</em></span><span class="r">${esc(w.role)}</span><span class="n">${esc(w.note)}</span><span class="n mono">${w.cardId ? '卡片 ' + w.cardId : '无卡片'} · ${w.dispatchId} · ${w.engineSessionId}</span><span class="wa">${act.map(([k, l]) => `<button class="wb" data-act="${k}" data-w="${i}">${l}</button>`).join('')}</span></div>`;
}
function orphanView(){
  return `<div class="mh"><div class="tx"><span class="tag waiting">未归属</span><h1>未归属卡片</h1><p>这些卡片不在任何任务的 Worker 里，Norma 不会收到它们的结果</p></div></div>
   <div class="cards orphans">${orphans.map(o => `<div class="cc waiting"><span class="t">${av(o.agent,'lg')}${o.agent}<small>${o.engine}</small></span><span class="r">${esc(o.role)}</span><span class="n">${esc(o.note)}</span><span class="n mono">${o.cardId}</span><span class="wa"><button class="wb" data-act="adopt">归入任务…</button><button class="wb" data-act="canvas">在画布中定位</button></span></div>`).join('')}</div>
   <p class="nowk">归入任务与自动分组属 M3；原型仅示意入口</p>`;
}
function renderFile(p){
  const src = preview[p] || '// 原型示例：此处显示文件预览';
  if (p.endsWith('.md')) return '<article class="md">' + src.split('\n').map(l => l.startsWith('## ') ? `<h2>${esc(l.slice(3))}</h2>` : l.startsWith('# ') ? `<h1>${esc(l.slice(2))}</h1>` : /^\d+\. /.test(l) ? `<p class="li">${esc(l)}</p>` : l.startsWith('- ') ? `<p class="li">• ${esc(l.slice(2))}</p>` : l.trim() ? `<p>${esc(l)}</p>` : '').join('') + '</article>';
  return '<div class="code">' + src.split('\n').map((l, i) => `<div class="${l[0]==='+'?'a':l[0]==='-'?'d':''}"><i>${i+1}</i>${esc(l)}</div>`).join('') + '</div>';
}
function msg(t, m){
  if (m.who === 'you') return `<div class="you"><div>${esc(m.text)}</div></div>`;
  if (m.who === 'event') return `<div class="ev">${m.agent ? av(m.agent) : orb('sm')}<span>${esc(m.text)}</span><small>${m.agent || '事件'}</small></div>`;
  const plan = m.plan ? `<div class="plan">${m.plan.map(a => { const w = t.workers.find(w => w.agent === a); return `<div>${av(a)}<span><b>${a}</b> <small>${esc(w.role)}</small></span><em class="st ${WCLS[w.state]}">${WST[w.state]}</em></div>`; }).join('')}</div>` : '';
  if (m.ask && !m.resolved) return `<div class="rec"><div class="who">${orb('sm')}Norma 的建议<small>需要你决定</small></div><p>${esc(m.text)}</p><div class="acts"><button class="fill" data-act="decide">${ico('check')}采用建议</button><button class="txt" data-act="edit">修改后发送${ico('out')}</button></div></div>`;
  return `<div class="nm"><div class="who">${orb('sm')}Norma <small>协调</small></div><p>${esc(m.text)}</p>${plan}${m.done||m.resolved?`<div class="done-note">${ico('check')}已记录</div>`:''}</div>`;
}
function workerAct(t, w, a){
  const ev = text => t.messages.push({who:'event', agent:w.agent, text});
  if (a === 'reclaim') { w.state = 'Reclaimed'; w.cardId = null; ev(`${w.agent} 已回收，会话 ${w.engineSessionId} 已保留。`); }
  else {
    reopen(t); seq++; w.state = 'Active'; w.cardId = 'card-' + seq; w.dispatchId = 'd-' + seq + '-1';
    ev(a === 'resume' ? `${w.agent} 已唤起，恢复会话 ${w.engineSessionId}，新 dispatch ${w.dispatchId}。` : `${w.agent} 已重派，新 dispatch ${w.dispatchId}。`);
  }
}
function toast(s){ const e = $('#toast'); e.textContent = s; e.classList.add('on'); clearTimeout(toast.t); toast.t = setTimeout(() => e.classList.remove('on'), 2600); }
const setMode = v => { st.mode = v; if (v === 'canvas') exitFull(); render(); };
function exitFull(){ st.full = false; if (document.fullscreenElement) document.exitFullscreen().catch(()=>{}); }
function toggleFull(){ st.full = !st.full; if (st.full) { st.mode = 'staff'; document.documentElement.requestFullscreen?.().catch(()=>{}); } else exitFull(); render(); }
document.addEventListener('click', e => {
  const b = e.target.closest('[data-act]'); if (!b || b.matches('select,form')) return; const a = b.dataset.act;
  if (a === 'mode') setMode(b.dataset.v);
  else if (a === 'full') toggleFull();
  else if (a === 'canvas') setMode('canvas');
  else if (a === 'open') { st.sel = b.dataset.t; st.file = null; st.filesOpen = false; st.mode = 'staff'; render(); }
  else if (a === 'toggle') { const k = b.dataset.k; st.collapsed.has(k) ? st.collapsed.delete(k) : st.collapsed.add(k); render(); }
  else if (a === 'filter') { st.f = b.dataset.k; render(); }
  else if (a === 'tab') { st.file = b.dataset.p; render(); }
  else if (a === 'untab') { e.stopPropagation(); const t = task(); st.tabs[t.id] = st.tabs[t.id].filter(p => p !== b.dataset.p); render(); }
  else if (a === 'files') { st.filesOpen = !st.filesOpen; render(); }
  else if (a === 'openfile') { const t = task(); if (!st.tabs[t.id].includes(b.dataset.p)) st.tabs[t.id].push(b.dataset.p); st.file = b.dataset.p; st.filesOpen = false; render(); }
  else if (a === 'decide') { const t = task(); t.messages.filter(m => m.ask).forEach(m => m.resolved = true); t.workers.filter(w => w.state === 'Failed' || w.state === 'Interrupted').forEach(w => w.state === 'Failed' ? workerAct(t, w, 'redispatch') : workerAct(t, w, 'reclaim')); t.messages.push({who:'you',text:'采用建议。'},{who:'norma',text:'已记录，按建议处理相关 Worker。（原型演示，未连接真实 Agent）',done:true}); render(); toast('已采用建议 · 原型演示'); }
  else if (['reclaim','resume','redispatch'].includes(a)) { const t = task(); workerAct(t, t.workers[b.dataset.w], a); render(); }
  else if (a === 'adopt') toast('归入任务属 M3 · 原型仅示意');
  else if (a === 'edit') { const ta = $('.composer textarea'); ta.value = task().messages.filter(m => m.ask).pop().text; ta.dispatchEvent(new Event('input', {bubbles:true})); ta.focus(); }
  else if (a === 'new') { const t = {id:'t'+(++seq),project:'Norma-AI-OS',title:'新任务 · 等待拆解',updated:'刚刚',phase:'Active',workers:[],files:[],messages:[{who:'norma',text:'想推进什么？说说目标，我来拆成 Worker 并安排 Agent。'}]}; tasks.unshift(t); st.sel = t.id; render(); $('.composer textarea').focus(); }
});
document.addEventListener('submit', e => { e.preventDefault(); const v = e.target.querySelector('textarea').value.trim(); if (!v) return; const t = task(); reopen(t); t.messages.push({who:'you',text:v},{who:'norma',text:'收到。我会沿用本任务的上下文协调相关 Worker。（示例回复）'}); st.drafts[t.id] = ''; render(); });
document.addEventListener('change', e => { if (e.target.matches('select[data-act=phase]')) { const t = task(); t.phase = e.target.value; render(); } });
document.addEventListener('input', e => {
  if (e.target.matches('.search input')) { st.q = e.target.value; const p = e.target.selectionStart; render(); const s = $('.search input'); s.focus(); s.setSelectionRange(p, p); }
  else if (e.target.matches('.composer textarea')) { st.drafts[st.sel] = e.target.value; $('.send').disabled = !e.target.value.trim(); e.target.style.height = 'auto'; e.target.style.height = e.target.scrollHeight + 'px'; }
});
document.addEventListener('keydown', e => {
  const k = e.key.toLowerCase();
  if ((e.metaKey || e.ctrlKey) && !e.shiftKey && k === 'e') { e.preventDefault(); setMode(st.mode === 'staff' ? 'canvas' : 'staff'); }
  else if (e.ctrlKey && e.metaKey && k === 'f') { e.preventDefault(); toggleFull(); }
  else if ((e.metaKey || e.ctrlKey) && k === 'enter' && e.target.matches('.composer textarea')) $('.composer').requestSubmit();
  else if (k === 'escape' && st.full) { exitFull(); render(); }
});
document.addEventListener('fullscreenchange', () => { if (!document.fullscreenElement && st.full) { st.full = false; render(); } });
if (new URLSearchParams(location.search).get('mode') === 'canvas') st.mode = 'canvas';
render();
