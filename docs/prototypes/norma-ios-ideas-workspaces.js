(() => {
  // Local interaction data for the visual prototype; nothing is sent to a server.
  const data={
    nextId:10,
    ideas:[
      {id:'idea-connect',text:'第一次连接 Mac 时，希望用户知道连接是否安全。',source:'text',time:'今天 09:41'},
      {id:'idea-handoff',text:'从 Mac 切到 iPhone 时，首屏信息太多。',source:'voice',time:'今天 09:20'}
    ],
    machines:[
      {id:'alan',name:'Alan Studio',online:true,projects:['ios','api','web']},
      {id:'build',name:'Build Mac',online:false,projects:['release']}
    ],
    projects:[
      {id:'ios',name:'iOS App',machineId:'alan'},
      {id:'api',name:'Norma API',machineId:'alan'},
      {id:'web',name:'Web Portal',machineId:'alan'},
      {id:'release',name:'Release Ops',machineId:'build'}
    ],
    agents:[
      {id:'norma',name:'Norma',detail:'Chief'},
      {id:'codex',name:'Codex',detail:'可用'},
      {id:'design',name:'Design',detail:'可用'},
      {id:'research',name:'Research',detail:'可用'}
    ],
    workspaces:[
      {id:'mobile',name:'移动端体验',projectIds:['ios','api'],agentIds:['norma','codex','design'],threadIds:['handoff','mobile-check']},
      {id:'quality',name:'发布质量',projectIds:['release'],agentIds:['norma','codex'],threadIds:['release-check']}
    ],
    threads:[
      {id:'handoff',workspaceId:'mobile',title:'改进首屏交接',sourceIdeaId:'idea-handoff',status:'running',goal:'让用户从 Mac 切到 iPhone 时，一眼看懂当前任务。',messages:[],tasks:[{agentId:'codex',title:'核对 iOS 与 API 状态',projectIds:['ios','api'],status:'running'}],artifact:'首屏信息层级预览'},
      {id:'mobile-check',workspaceId:'mobile',title:'检查移动端异常提示',sourceIdeaId:null,status:'idle',goal:'让关键异常有清楚的恢复入口。',messages:[],tasks:[],artifact:null},
      {id:'release-check',workspaceId:'quality',title:'部署前检查',sourceIdeaId:null,status:'blocked',goal:'在测试环境完成发布前验证。',messages:[],tasks:[{agentId:'codex',title:'验证发布前检查',projectIds:['release'],status:'blocked'}],artifact:null}
    ]
  };
  const app=document.getElementById('app');
  const ui={view:'ideas',workspaceId:'mobile',threadId:'handoff',ideaId:'idea-connect',sheet:null,notice:'',animate:false,recording:true,voiceText:'从 Mac 切到 iPhone 时，首屏信息太多。',ideaDraft:''};
  const images={
    ideas:'../product-specs/norma-ios-ideas-workspaces-2026-09-28/P01-idea-library.png',
    voice:'../product-specs/norma-ios-ideas-workspaces-2026-09-28/P02-voice-idea.png',
    workspaces:'../product-specs/norma-ios-ideas-workspaces-2026-09-28/P03-workspaces-drawer.png',
    settings:'../product-specs/norma-ios-ideas-workspaces-2026-09-28/P04-workspace-settings.png',
    thread:'../product-specs/norma-ios-ideas-workspaces-2026-09-28/P05-work-thread.png',
    devices:'../product-specs/norma-ios-prototypes-2026-09-26/W02-connected-computers.png',
    task:'../product-specs/norma-ios-prototypes-2026-09-26/W06-agent-task.png',
    call:'../product-specs/norma-ios-prototypes-2026-09-26/W07-norma-call.png',
    inbox:'../product-specs/norma-ios-prototypes-2026-09-26/W08-inbox.png',
    artifact:'../product-specs/norma-ios-prototypes-2026-09-26/W09-acceptance.png'
  };
  const esc=value=>String(value??'').replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const idea=id=>data.ideas.find(item=>item.id===id);
  const workspace=()=>data.workspaces.find(item=>item.id===ui.workspaceId)||data.workspaces[0];
  const thread=()=>data.threads.find(item=>item.id===ui.threadId)||data.threads[0];
  const project=id=>data.projects.find(item=>item.id===id);
  const agent=id=>data.agents.find(item=>item.id===id);
  const machine=id=>data.machines.find(item=>item.id===id);
  const button=(label,action,extra='')=>`<button type="button" data-action="${action}" ${extra}>${esc(label)}</button>`;
  const spot=(label,action,[x,y,w,h],extra='')=>`<button class="hotspot" type="button" aria-label="${esc(label)}" data-action="${action}" ${extra} style="left:${x/10.24}%;top:${y/15.36}%;width:${w/10.24}%;height:${h/15.36}%"></button>`;
  const sourceThreads=id=>data.threads.filter(item=>item.sourceIdeaId===id);

  function hotspots(){
    if(ui.view==='ideas')return [
      spot('打开更多模块','workspaces',[192,170,94,90]),
      spot('记录想法','new-idea',[615,260,215,98]),
      ...(data.ideas.length===2?[
        spot('查看文字想法','open-idea',[200,525,628,300],'data-idea-id="idea-connect"'),
        spot('查看语音想法','open-idea',[200,850,628,278],'data-idea-id="idea-handoff"')
      ]:[]),
      spot('前往工作空间','workspaces',[240,1320,157,120]),
      spot('前往 Norma','call',[424,1320,175,120]),
      spot('前往收件箱','inbox',[620,1320,170,120])
    ].join('');
    if(ui.view==='voice')return [
      spot('返回想法库','ideas',[196,165,86,92]),
      spot('关闭语音记录','ideas',[751,165,80,92]),
      spot(ui.recording?'停止录音':'继续录音','voice-toggle',[410,960,200,210]),
      spot('保存语音想法','save-voice',[215,1252,595,135])
    ].join('');
    if(ui.view==='workspaces')return [
      spot('打开更多模块','toggle-drawer',[195,160,100,90]),
      spot('新建工作空间','new-workspace',[610,250,220,95]),
      ...(data.workspaces.length===2?[
        spot('打开移动端体验设置','open-settings',[200,392,630,160],'data-workspace-id="mobile"'),
        spot('打开改进首屏交接','open-thread',[222,550,592,78],'data-thread-id="handoff"'),
        spot('打开发布质量设置','open-settings',[200,735,630,155],'data-workspace-id="quality"'),
        spot('打开部署前检查','open-thread',[220,880,592,78],'data-thread-id="release-check"')
      ]:[]),
      ...(ui.drawer?[
        spot('关闭更多模块','toggle-drawer',[749,1035,82,82]),
        spot('打开想法库','ideas',[205,1140,630,105]),
        spot('打开已连接电脑','devices',[205,1250,630,105]),
        spot('打开设置','global-settings',[205,1350,630,105])
      ]:[
        spot('打开想法库','ideas',[205,1320,200,105]),
        spot('前往 Norma','call',[420,1320,185,105]),
        spot('前往收件箱','inbox',[620,1320,200,105])
      ])
    ].join('');
    if(ui.view==='settings')return ui.workspaceId==='mobile'&&!workspace().changed?[
      spot('返回工作空间列表','workspaces',[198,165,93,90]),
      spot('移除 iOS App','remove-project',[696,407,108,85],'data-id="ios"'),
      spot('移除 Norma API','remove-project',[696,510,108,85],'data-id="api"'),
      spot('添加已有项目','add-project',[215,610,595,95]),
      spot('移除 Codex','remove-agent',[696,970,108,85],'data-id="codex"'),
      spot('移除 Design','remove-agent',[696,1075,108,85],'data-id="design"'),
      spot('添加 Agent','add-agent',[215,1195,595,95])
    ].join(''):'';
    if(ui.view==='thread')return ui.threadId!=='handoff'?'':[
      spot('返回工作空间列表','workspaces',[185,128,90,100]),
      spot('工作线程操作','thread-menu',[751,130,80,105]),
        spot('查看事项目标与决定','thread-brief',[188,230,646,122]),
        spot('查看来源想法','source-idea',[188,357,646,133]),
        spot('查看 Codex 任务','task',[188,995,650,137]),
        spot('查看成果','artifact',[188,1175,650,123]),
        spot('发送消息或 @Agent','compose',[270,1340,398,100]),
        spot('提及 Agent','mention',[675,1340,74,100]),
        spot('与 Norma 通话','call',[755,1340,80,100])
    ].join('');
    if(ui.view==='devices')return [spot('返回工作空间列表','workspaces',[205,178,95,96]),spot('扫码添加电脑','pair',[220,1215,590,130])].join('');
    if(ui.view==='task')return [spot('返回工作线程','thread',[195,175,100,110]),spot('在工作线程 @Codex','mention',[510,1290,315,100])].join('');
    if(ui.view==='artifact')return [spot('返回工作线程','thread',[195,170,100,100]),spot('查看证据','evidence',[225,945,600,160])].join('');
    if(ui.view==='call')return [spot('结束对话','thread',[420,1110,190,160]),spot('说一句话给 Norma','voice-demo',[225,965,580,140]),spot('前往工作空间','workspaces',[205,1280,210,160]),spot('前往收件箱','inbox',[615,1280,210,160])].join('');
    if(ui.view==='inbox')return [spot('前往工作空间','workspaces',[205,1280,210,160]),spot('前往 Norma','call',[420,1280,195,160]),spot('查看成果','artifact',[220,795,595,380])].join('');
    return '';
  }

  function ideaList(){
    if(ui.view!=='ideas'||data.ideas.length===2)return '';
    return `<section class="dynamic-idea-list" aria-label="想法列表">${data.ideas.map(item=>`<button type="button" data-action="open-idea" data-idea-id="${esc(item.id)}"><small>${item.source==='voice'?'语音转写':'文字'} · ${esc(item.time)}</small><strong>${esc(item.text)}</strong><span>${sourceThreads(item.id).length?'已用于：'+sourceThreads(item.id).map(t=>workspaceName(t.workspaceId)+' · '+t.title).join('、'):'仅记录，尚未执行'}</span></button>`).join('')}</section>`;
  }
  function workspaceName(id){return data.workspaces.find(item=>item.id===id)?.name||'工作空间';}
  function workspaceList(){
    if(ui.view!=='workspaces'||data.workspaces.length===2)return '';
    return `<section class="dynamic-workspace-list" aria-label="工作空间列表">${data.workspaces.map(item=>`<div class="dynamic-workspace-card"><button type="button" data-action="open-settings" data-workspace-id="${esc(item.id)}"><strong>${esc(item.name)}</strong><small>${item.projectIds.length} 个项目 · ${item.agentIds.length} 位 Agent · ${item.threadIds.length} 件事项</small></button>${item.threadIds.map(id=>{const work=data.threads.find(t=>t.id===id);return `<button type="button" data-action="open-thread" data-thread-id="${esc(id)}">${esc(work?.title||'工作线程')} ›</button>`;}).join('')}</div>`).join('')}</section>`;
  }
  function drawerClosed(){
    if(ui.view!=='workspaces'||ui.drawer)return '';
    return `<div class="drawer-closed"><p>每个工作空间都可设置自己的项目与 Agent。</p><div>${button('想法库','ideas')}${button('已连接电脑','devices')}</div><nav>${button('工作空间','workspaces')}${button('Norma','call')}${button('收件箱','inbox')}</nav></div>`;
  }
  function settingsContent(){
    if(ui.view!=='settings'||ui.workspaceId==='mobile'&&!workspace().changed)return '';
    const item=workspace();
    const rows=(ids,lookup,type)=>ids.map(id=>{const value=lookup(id);if(!value)return '';const meta=type==='project'?`${machine(value.machineId)?.name||'已连接电脑'} · ${machine(value.machineId)?.online?'在线':'离线'}`:value.detail;return `<div class="config-row"><span><b>${esc(value.name)}</b><small>${esc(meta)}</small></span>${id==='norma'?'<em>负责人</em>':`<button type="button" data-action="remove-${type}" data-id="${esc(id)}" aria-label="移除 ${esc(value.name)}">移除</button>`}</div>`;}).join('');
    return `<section class="settings-content" aria-label="工作空间配置"><header>${button('‹','workspaces','aria-label="返回工作空间列表"')}<div><h1>工作空间设置</h1><small>${esc(item.name)}</small></div></header><div class="settings-scroll"><section class="config-group"><h2>关联项目</h2><p>工作空间可使用的已有项目</p>${rows(item.projectIds,project,'project')}${button('＋ 添加已有项目','add-project','class="add-row" aria-label="添加已有项目"')}</section><section class="config-group"><h2>参与 Agent</h2><p>加入后可使用此工作空间关联的项目</p>${rows(item.agentIds,agent,'agent')}${button('＋ 添加 Agent','add-agent','class="add-row" aria-label="添加 Agent"')}</section><p class="settings-note">关联仅影响此工作空间，不删除项目或历史记录。</p></div></section>`;
  }
  function voiceEditor(){
    if(ui.view!=='voice')return '';
    return `<div class="voice-editor"><label for="voice-transcript">转写文字</label><textarea id="voice-transcript" aria-label="转写文字" maxlength="500">${esc(ui.voiceText)}</textarea><small>${ui.recording?'正在转写 · 可修改后保存':'录音已停止 · 可修改后保存'}</small></div>`;
  }
  function threadContent(){
    if(ui.view!=='thread')return '';
    const item=thread();
    if(item.id==='handoff')return item.messages.length?`<section class="thread-updates" aria-label="最新对话"><b>最新对话</b>${item.messages.slice(-4).map(message=>`<p><strong>${esc(message.author)}：</strong>${esc(message.text)}</p>`).join('')}</section>`:'';
    const source=item.sourceIdeaId?idea(item.sourceIdeaId):null;
    return `<section class="dynamic-thread"><header>${button('‹','workspaces','aria-label="返回工作空间列表"')}<div><h1>${esc(item.title)}</h1><small>${esc(workspaceName(item.workspaceId))} · ${item.status==='running'?'执行中':'待推进'}</small></div></header><button type="button" data-action="thread-brief" class="thread-info">事项目标与决定<br><small>${esc(item.goal)}</small></button>${source?`<button type="button" data-action="source-idea" class="thread-info">来源想法：${esc(source.text)}</button>`:''}<div class="dynamic-thread-messages">${item.messages.map(message=>`<p><b>${esc(message.author)}</b>　${esc(message.text)}</p>`).join('')||'<p>在这里与 Norma 讨论，明确执行意图后再派工。</p>'}</div><div class="dynamic-thread-compose">${button('发送消息或 @Agent','compose')}${button('与 Norma 通话','call')}</div></section>`;
  }
  function sheet(){
    if(!ui.sheet)return '';
    let title='',body='';
    if(ui.sheet==='new-idea'){
      title='记录想法';body=`<form id="idea-form"><label for="idea-text">想法内容</label><textarea id="idea-text" name="text" required maxlength="500" placeholder="随时记下一个念头">${esc(ui.ideaDraft)}</textarea><div class="sheet-actions">${button('语音输入','voice') }<button type="submit" class="primary">保存到想法库</button></div><p>保存后只记录想法，不会开始执行。</p></form>`;
    }else if(ui.sheet==='idea-detail'||ui.sheet==='source-idea'){
      const item=idea(ui.ideaId);title=ui.sheet==='source-idea'?'来源想法':'想法';body=`<p class="sheet-idea">${esc(item?.text||'')}</p><p>${item?.source==='voice'?'语音转写':'文字'} · ${esc(item?.time||'刚刚')}</p>${sourceThreads(item?.id).length?`<p>已用于：${esc(sourceThreads(item.id).map(t=>workspaceName(t.workspaceId)+' · '+t.title).join('、'))}</p>`:''}${button('在工作空间使用','use-idea')}`;
    }else if(ui.sheet==='use-idea'){
      title='选择工作空间';body='<p>原始想法仍保留在想法库。这里只建立一条工作线程。</p>'+data.workspaces.map(item=>button(item.name,'place-idea',`data-workspace-id="${esc(item.id)}"`)).join('');
    }else if(ui.sheet==='new-workspace'){
      title='新建工作空间';body='<form id="workspace-form"><label for="workspace-name">工作空间名称</label><input id="workspace-name" name="name" required maxlength="32" placeholder="例如：网站体验"><p>创建后可分别设置项目与 Agent。</p><button type="submit" class="primary">创建工作空间</button></form>';
    }else if(ui.sheet==='add-project'){
      title='添加已有项目';body='<p>只关联到当前工作空间，不复制或删除项目。</p>'+data.projects.map(item=>{const host=machine(item.machineId),selected=workspace().projectIds.includes(item.id);return button(`${item.name} · ${host?.name||'电脑'}${selected?' · 已关联':host?.online?'':' · 离线'}`,'link-project',`data-id="${esc(item.id)}" aria-label="关联 ${esc(item.name)}" ${selected||!host?.online?'disabled':''}`);}).join('');
    }else if(ui.sheet==='add-agent'){
      title='添加 Agent';body='<p>加入后可在当前工作空间使用关联项目。</p>'+data.agents.filter(item=>item.id!=='norma').map(item=>button(`${item.name}${workspace().agentIds.includes(item.id)?' · 已加入':''}`,'link-agent',`data-id="${esc(item.id)}" aria-label="加入 ${esc(item.name)}" ${workspace().agentIds.includes(item.id)?'disabled':''}`)).join('');
    }else if(ui.sheet==='compose'){
      title='发送工作线程消息';body=`<form id="message-form"><label for="thread-message">消息内容</label><textarea id="thread-message" name="text" required maxlength="500">${esc(ui.messageDraft||'')}</textarea><button type="submit" class="primary">发送消息</button></form>`;
    }else if(ui.sheet==='thread-brief'){
      title='事项目标与决定';body=`<p>${esc(thread().goal)}</p><p>当前工作空间：${esc(workspaceName(thread().workspaceId))}</p>`;
    }else if(ui.sheet==='thread-menu'){
      title='工作线程操作';body=button('查看工作空间设置','open-settings',`data-workspace-id="${esc(thread().workspaceId)}"`)+button('查看事项目标与决定','thread-brief');
    }else if(ui.sheet==='global-settings'){
      title='设置';body='<p>当前原型重点展示想法、工作空间及工作线程。</p>';
    }else if(ui.sheet==='pair'){
      title='扫码添加电脑';body='<p>在 Mac 上打开 Norma 的配对二维码后，用手机扫描。</p>';
    }else if(ui.sheet==='evidence'){
      title='验证证据';body='<p>可从成果返回原工作线程，继续追问 Agent。</p>';
    }
    return `<button class="scrim" type="button" data-action="close-sheet" aria-label="关闭弹层"></button><section class="sheet" role="dialog" aria-modal="true" aria-label="${esc(title)}"><div class="sheet-handle" aria-hidden="true"></div><button type="button" class="sheet-close" data-action="close-sheet" aria-label="完成">完成</button><h2>${esc(title)}</h2>${body}</section>`;
  }
  function render(){
    const image=images[ui.view]||images.ideas;
    app.innerHTML=`<div class="frame"><div class="artboard" data-view="${esc(ui.view)}"><img id="reference-screen" class="${ui.animate?'screen-enter':''}" src="${image}" alt="${esc(ui.view)} 参考设计"><div class="hotspots" ${ui.view==='workspaces'?'aria-label="工作空间列表"':''}>${hotspots()}</div>${ideaList()}${workspaceList()}${drawerClosed()}${settingsContent()}${voiceEditor()}${threadContent()}<div class="notice${ui.notice?' visible':''}" role="status" aria-live="polite">${esc(ui.notice)}</div>${sheet()}</div></div>`;
    ui.animate=false;
  }
  function show(view){ui.animate=view!==ui.view;ui.view=view;ui.sheet=null;ui.notice='';render();}
  function notify(message){ui.notice=message;render();clearTimeout(notify.timer);notify.timer=setTimeout(()=>{ui.notice='';const notice=app.querySelector('.notice');if(notice){notice.classList.remove('visible');notice.textContent='';}},3000);}
  function openSheet(name){ui.sheet=name;render();requestAnimationFrame(()=>(app.querySelector('.sheet textarea,.sheet input,.sheet button')||app.querySelector('.sheet'))?.focus());}

  document.addEventListener('click',event=>{
    const target=event.target.closest('button[data-action]');if(!target||target.disabled)return;
    const action=target.dataset.action;
    if(action==='close-sheet'){ui.sheet=null;render();return;}
    if(['ideas','workspaces','devices','task','artifact','call','inbox','thread'].includes(action)){
      if(action==='workspaces')ui.drawer=true;
      show(action);return;
    }
    if(action==='new-idea'||action==='new-workspace'||action==='add-project'||action==='add-agent'||action==='compose'||action==='thread-brief'||action==='thread-menu'||action==='global-settings'||action==='pair'||action==='evidence'||action==='use-idea'){openSheet(action);return;}
    if(action==='toggle-drawer'){ui.drawer=!ui.drawer;render();return;}
    if(action==='open-idea'){ui.ideaId=target.dataset.ideaId;openSheet('idea-detail');return;}
    if(action==='source-idea'){ui.ideaId=thread().sourceIdeaId;openSheet('source-idea');return;}
    if(action==='open-settings'){ui.workspaceId=target.dataset.workspaceId||ui.workspaceId;show('settings');return;}
    if(action==='open-thread'){ui.threadId=target.dataset.threadId;ui.workspaceId=thread().workspaceId;show('thread');return;}
    if(action==='voice'){ui.ideaDraft=app.querySelector('#idea-text')?.value||'';ui.voiceText=ui.ideaDraft||'从 Mac 切到 iPhone 时，首屏信息太多。';ui.recording=true;show('voice');return;}
    if(action==='voice-toggle'){ui.voiceText=app.querySelector('#voice-transcript')?.value||ui.voiceText;ui.recording=!ui.recording;notify(ui.recording?'继续录音':'录音已停止，可编辑转写');return;}
    if(action==='save-voice'){const value=app.querySelector('#voice-transcript')?.value.trim();if(!value){notify('先写下转写文字');return;}data.ideas.unshift({id:'idea-'+data.nextId++,text:value,source:'voice',time:'刚刚'});ui.voiceText=value;show('ideas');notify('已记录到想法库 · 尚未执行');return;}
    if(action==='place-idea'){const item=idea(ui.ideaId),space=data.workspaces.find(entry=>entry.id===target.dataset.workspaceId);if(!item||!space)return;const id='thread-'+data.nextId++;const title=item.text.replace(/[。！!？?].*$/,'').slice(0,16);data.threads.unshift({id,workspaceId:space.id,title,sourceIdeaId:item.id,status:'idle',goal:item.text,messages:[],tasks:[],artifact:null});space.threadIds.unshift(id);ui.workspaceId=space.id;ui.threadId=id;show('thread');notify('已在 '+space.name+' 建立工作线程 · 尚未执行');return;}
    if(action==='link-project'){const id=target.dataset.id;if(!workspace().projectIds.includes(id)){workspace().projectIds.push(id);workspace().changed=true;}ui.sheet=null;notify(project(id).name+' 已关联到 '+workspace().name);return;}
    if(action==='link-agent'){const id=target.dataset.id;if(!workspace().agentIds.includes(id)){workspace().agentIds.push(id);workspace().changed=true;}ui.sheet=null;notify(agent(id).name+' 已加入 '+workspace().name);return;}
    if(action==='remove-project'||action==='remove-agent'){const id=target.dataset.id,key=action==='remove-project'?'projectIds':'agentIds';const used=action==='remove-project'?data.threads.some(item=>item.workspaceId===workspace().id&&item.tasks.some(task=>task.status==='running'&&task.projectIds.includes(id))):data.threads.some(item=>item.workspaceId===workspace().id&&item.tasks.some(task=>task.status==='running'&&task.agentId===id));if(used){notify('有进行中的任务，完成后再移除');return;}workspace()[key]=workspace()[key].filter(value=>value!==id);workspace().changed=true;notify('已从当前工作空间移除关联');return;}
    if(action==='mention'){ui.messageDraft='@Codex ';openSheet('compose');return;}
    if(action==='voice-demo'){notify('这是通话交互演示；语音输入请从想法库进入');return;}
  });
  document.addEventListener('submit',event=>{
    const form=event.target;if(!['idea-form','workspace-form','message-form'].includes(form.id))return;event.preventDefault();const values=new FormData(form);
    if(form.id==='idea-form'){const value=String(values.get('text')||'').trim();if(!value)return;data.ideas.unshift({id:'idea-'+data.nextId++,text:value,source:'text',time:'刚刚'});ui.ideaDraft='';ui.sheet=null;notify('已记录到想法库 · 尚未执行');return;}
    if(form.id==='workspace-form'){const name=String(values.get('name')||'').trim();if(!name)return;const id='workspace-'+data.nextId++;data.workspaces.unshift({id,name,projectIds:[],agentIds:['norma'],threadIds:[]});ui.workspaceId=id;ui.sheet=null;show('settings');notify('工作空间已创建 · 可设置项目与 Agent');return;}
    if(form.id==='message-form'){const text=String(values.get('text')||'').trim();if(!text)return;thread().messages.push({author:'你',text});const mentioned=text.match(/@(Norma|Codex|Design|Research)\b/i)?.[1]||'Norma';const name=mentioned[0].toUpperCase()+mentioned.slice(1).toLowerCase();thread().messages.push({author:name,text:name==='Norma'?'收到。我会在当前工作线程继续协调。':'收到。我会在当前工作线程继续处理并回报。'});ui.messageDraft='';ui.sheet=null;notify(name+' 已回应');}
  });
  document.addEventListener('keydown',event=>{if(event.key!=='Escape')return;if(ui.sheet){ui.sheet=null;render();return;}if(ui.view==='thread'||ui.view==='settings')show('workspaces');else if(ui.view!=='ideas')show('ideas');});
  render();
})();
