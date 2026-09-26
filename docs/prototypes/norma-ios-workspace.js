(() => {
  const {initialState,act,channelById,inboxItems}=window.NormaModel;
  const model=initialState();
  const assets='../product-specs/norma-ios-prototypes-2026-09-26/';
  const screens={
    list:'W01-workspace-list.png',devices:'W02-connected-computers.png',projects:'W03-project-picker.png',
    chat:'W04-workspace-chat.png',brief:'W05-work-brief.png',task:'W06-agent-task.png',
    call:'W07-norma-call.png',inbox:'W08-inbox.png',acceptance:'W09-acceptance.png'
  };
  const screenDescriptions={
    list:'工作空间列表：首屏交接执行中，新人首次连接尚未执行',
    devices:'已连接电脑：Alan Studio 在线，Build Mac 离线',
    projects:'工作空间项目选择：iOS App 与 Norma API 已关联，Release Ops 离线',
    chat:'工作空间对话：用户指令、Norma 派工、Agent 回执和任务卡',
    brief:'工作简报：当前目标、关联项目、硬约束与决定',
    task:'Codex 任务详情：执行进度、证据与项目范围',
    call:'Norma 实时对话：虚拟人、字幕、静音与结束按钮',
    inbox:'收件箱：需要决定的事项和待验收成果',
    acceptance:'成果验收：预览、证据、退回修改与验收通过'
  };
  const ui={view:'list',workspaceId:'ios-handoff',projectReturn:'chat',sheet:null,notice:'',filter:'all',inboxQuery:'',voiceMuted:false,inboxTouched:false,animateNext:false};
  const app=document.getElementById('app');
  const esc=value=>String(value??'').replace(/[&<>"']/g,char=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
  const workspace=()=>channelById(model,ui.workspaceId)||model.channels[0];
  const spot=(label,action,[x,y,w,h],data='')=>`<button class="hotspot" type="button" aria-label="${esc(label)}" title="${esc(label)}" data-action="${action}" ${data} style="left:${x/10.24}%;top:${y/15.36}%;width:${w/10.24}%;height:${h/15.36}%"></button>`;
  const link=(label,action,kind='')=>`<button type="button" class="${kind}" data-action="${action}">${esc(label)}</button>`;
  const hasDynamicList=()=>ui.filter!=='all'||model.channels.length!==3||model.channels.some(item=>['accepted','archived','review'].includes(item.status));

  function hotspots(){
    if(ui.view==='list')return [
      spot('已连接电脑','devices',[222,185,360,84]),spot('保存新想法','new-idea',[723,188,93,86]),
      spot('搜索工作空间','search',[224,372,574,88]),
      ...(hasDynamicList()?[]:[
        spot('显示全部工作空间','filter',[224,473,142,78],'data-filter="all"'),
        spot('显示执行中','filter',[370,473,139,78],'data-filter="running"'),
        spot('显示构思中','filter',[510,473,144,78],'data-filter="idea"'),
        spot('显示已归档','filter',[654,473,146,78],'data-filter="archived"'),
        spot('打开改进 iOS 首屏交接','open-ios',[221,558,592,336]),
        spot('打开新人首次连接体验','open-first',[221,911,592,287])
      ]),
      spot('前往 Norma','persona',[413,1280,194,160]),spot('前往收件箱','inbox',[613,1280,195,160])
    ].join('');
    if(ui.view==='devices')return [
      spot('返回工作空间列表','list',[208,185,95,90]),
      spot('查看 iOS App 项目','project-info',[252,473,523,110],'data-project="iOS App"'),
      spot('查看 Norma API 项目','project-info',[252,592,523,128],'data-project="Norma API"'),
      spot('查看 Release Ops 项目','project-info',[251,932,523,110],'data-project="Release Ops"'),
      spot('扫码添加电脑','scan',[224,1230,579,124])
    ].join('');
    if(ui.view==='projects')return [
      spot('关闭项目选择','done-projects',[196,180,85,85]),
      spot('完成项目选择','done-projects',[720,696,111,96]),
      ...(ui.workspaceId==='ios-handoff'?[
        spot('切换 iOS App 关联','toggle-project',[221,882,588,108],'data-project="iOS App"'),
        spot('切换 Norma API 关联','toggle-project',[222,999,586,111],'data-project="Norma API"'),
        spot('Release Ops 所在电脑离线','offline-project',[222,1190,587,113])
      ]:[])
    ].join('');
    if(ui.view==='chat'){
      if(ui.workspaceId!=='ios-handoff')return '';
      return [
        spot('返回工作空间列表','list',[208,177,90,101]),
        spot('工作空间设置','settings',[728,177,91,101]),
        spot('打开工作简报','brief',[209,280,607,108]),
        spot('查看 Codex 任务','task',[213,1044,607,181]),
        spot('与 Norma 通话','call',[582,1224,234,87]),
        spot('添加附件','attachments',[217,1314,94,96]),
        spot('发送消息或 @Agent','compose',[311,1314,404,99]),
        spot('使用语音输入','call',[721,1314,94,97])
      ].join('');
    }
    if(ui.view==='brief'&&ui.workspaceId!=='ios-handoff')return '';
    if(ui.view==='brief')return [
      spot('返回工作空间对话','chat',[207,201,91,96]),
      spot('管理关联项目','projects',[227,555,571,201]),
      spot('查看决定来源：首屏状态','decision-source',[240,1023,550,109]),
      spot('查看决定来源：主操作','decision-source',[240,1146,550,106]),
      spot('纠正目标与限制','edit-brief',[230,1280,579,101])
    ].join('');
    if(ui.view==='task')return [
      spot('返回工作空间对话','chat',[202,189,89,97]),
      spot('任务操作','task-menu',[728,190,87,88]),
      spot('查看首屏入口变更预览','evidence',[246,1033,546,69],'data-evidence="首屏入口变更预览"'),
      spot('查看 API 状态字段检查记录','evidence',[246,1102,546,79],'data-evidence="API 状态字段检查记录"'),
      spot('回到工作空间','chat',[223,1305,279,91]),
      spot('在频道 @Codex','mention-codex',[509,1305,296,91])
    ].join('');
    if(ui.view==='call')return [
      spot('切换当前工作空间','workspace-picker',[216,185,593,113]),
      spot('说一句话给 Norma','voice-demo',[222,963,581,138]),
      spot(ui.voiceMuted?'取消静音':'静音','voice-mute',[247,1117,163,159]),
      spot('结束对话','call-end',[430,1117,169,159]),
      spot('输入文字','voice-type',[614,1117,170,159]),
      spot('前往工作空间列表','list',[215,1280,194,158]),
      spot('前往收件箱','inbox',[609,1280,194,158])
    ].join('');
    if(ui.view==='inbox')return [
      spot('搜索收件箱','search-inbox',[719,201,99,96]),
      ...(ui.inboxTouched?[]:[
        spot('查看原因与建议','decision',[218,363,596,418]),
        spot('查看成果与证据','acceptance',[218,798,595,380])
      ]),
      spot('前往工作空间列表','list',[219,1280,193,160]),
      spot('前往 Norma','persona',[414,1280,191,160])
    ].join('');
    if(ui.view==='acceptance')return [
      spot('返回收件箱','inbox',[205,175,90,90]),
      spot('查看 iOS 首屏预览','preview',[218,254,591,383]),
      spot('查看入口状态证据','evidence',[239,944,557,97],'data-evidence="入口状态"'),
      spot('查看 API 字段证据','evidence',[239,1048,557,89],'data-evidence="API 字段"'),
      spot('查看关键流程证据','evidence',[239,1138,557,91],'data-evidence="关键流程"'),
      spot('退回修改','revise',[214,1320,295,107]),
      spot('验收通过','accept',[515,1320,296,107])
    ].join('');
    return '';
  }

  function newWorkspace(){
    if(ui.view!=='chat'||ui.workspaceId==='ios-handoff')return '';
    const item=workspace();
    const status=({running:'执行中',idea:'尚未执行',idle:'待继续',review:'待验收',accepted:'已验收',archived:'已归档',failed:'执行受阻'})[item.status]||item.status;
    const nextAction=item.status==='archived'?link('重开工作空间','reopen','new-action'):item.status==='accepted'?link('归档工作空间','archive','new-action'):item.status==='review'?link('查看待验收成果','acceptance','new-action'):item.tasks.length?`<div class="new-message">${item.tasks.length} 张任务卡 · ${status}</div>`:link('让 Norma 开始','start','new-action');
    const messages=item.messages.slice(-4).map(message=>`<div class="new-message"><b>${esc(message.author)}</b> · ${esc(message.time)}<br>${esc(message.text)}</div>`).join('');
    return `<section class="new-workspace" aria-label="新工作空间"><div class="new-top"><button type="button" data-action="list" aria-label="返回工作空间列表">‹</button><div><h1>${esc(item.title)}</h1><span class="muted">${status}</span></div><button type="button" data-action="settings" aria-label="工作空间设置">•••</button></div><button type="button" class="new-brief" data-action="brief"><b>工作简报</b><br>${esc(item.goal)}<br><span class="muted">${item.projects.length?esc(item.projects.join(' · ')):'尚未关联项目'}</span></button><div class="new-thread">${messages}</div>${nextAction}<div class="new-composer">${link('＋','attachments')} ${link('发消息或 @Agent','compose')}</div></section>`;
  }

  function listResults(){
    if(ui.view!=='list'||!hasDynamicList())return '';
    const labels={all:'全部',running:'执行中',idea:'构思中',archived:'已归档'};
    const filters=Object.entries(labels).map(([key,label])=>`<button type="button" data-action="filter" data-filter="${key}" aria-pressed="${ui.filter===key}">${label}</button>`).join('');
    const items=model.channels.filter(item=>ui.filter==='archived'?item.status==='archived':ui.filter==='running'?item.status==='running':ui.filter==='idea'?['idea','idle'].includes(item.status):item.status!=='archived');
    const cards=items.map(item=>`<button type="button" class="result-card" data-action="open-workspace" data-id="${esc(item.id)}" aria-label="打开工作空间：${esc(item.title)}"><em>${esc(({running:'执行中',idea:'尚未执行',idle:'待继续',review:'待验收',accepted:'已验收',archived:'已归档',failed:'执行受阻'})[item.status])}</em><strong>${esc(item.title)}</strong><small>${esc(item.projects.join(' · ')||'尚未关联项目')}</small></button>`).join('');
    return `<div class="list-filter-row">${filters}</div><div class="list-results">${cards||'<p class="empty-state">这里还没有相应的工作空间。</p>'}</div>`;
  }

  function dynamicProjects(){
    if(ui.view!=='projects'||ui.workspaceId==='ios-handoff')return '';
    const item=workspace();
    return `<section class="project-dynamic" aria-label="当前工作空间可用项目">${model.machines.map(machine=>`<h3>${esc(machine.name)} · ${machine.online?'在线':'离线'}</h3>${machine.projects.map(project=>`<button type="button" data-action="toggle-project" data-project="${esc(project)}" aria-pressed="${item.projects.includes(project)}" ${machine.online?'':'disabled'}>${esc(project)} ${item.projects.includes(project)?'✓ 已关联':machine.online?'○ 可关联':'· 离线不可用'}</button>`).join('')||'<p>尚无项目</p>'}`).join('')}</section>`;
  }

  function dynamicBrief(){
    if(ui.view!=='brief'||ui.workspaceId==='ios-handoff')return '';
    const item=workspace();
    return `<section class="new-brief-page" aria-label="当前工作空间工作简报"><button type="button" data-action="chat" aria-label="返回工作空间对话">‹ 返回</button><h1>工作简报</h1><div class="brief-block"><b>当前目标</b><p>${esc(item.goal)}</p></div><div class="brief-block"><b>关联项目</b><p>${esc(item.projects.join(' · ')||'尚未关联项目')}</p>${link('添加已有项目','projects')}</div><div class="brief-block"><b>不能越过</b><p>${esc(item.hardRules.join(' · ')||'暂无硬约束')}</p></div><div class="brief-block"><b>当前决定</b><p>${esc(item.decisions.join(' · ')||'尚无决定')}</p></div>${link('纠正目标与限制','edit-brief','primary')}</section>`;
  }

  function inboxUpdate(){
    if(ui.view!=='inbox'||!ui.inboxTouched)return '';
    const items=inboxItems(model).filter(item=>(item.title+' '+item.detail).includes(ui.inboxQuery));
    return `<section class="inbox-updated" aria-label="当前待办">${ui.inboxQuery?`<div class="inbox-search-state">搜索「${esc(ui.inboxQuery)}」${link('清除搜索','clear-inbox-search')}</div>`:''}${items.map(item=>`<button type="button" class="inbox-card" data-action="${item.type==='review'?'acceptance':'decision'}"><em>${item.type==='review'?'成果待验收':'需要决定'}</em><strong>${esc(item.title)}</strong><small>${esc(item.detail)}</small></button>`).join('')||(ui.inboxQuery?'<p>没有符合条件的待办。</p>':'<p>目前没有需要你处理的事。Norma 会继续跟进。</p>')}</section>`;
  }

  function overrides(){
    let html=newWorkspace()+listResults()+dynamicProjects()+dynamicBrief()+inboxUpdate();
    const item=workspace();
    if(ui.view==='devices'&&model.machines.length>2)html+='<div class="device-new"><b>Studio Mini · 在线</b><small>刚刚注册 · 0 个项目<br>请在电脑端 Norma 添加项目</small></div>';
    if(ui.view==='chat'&&ui.workspaceId==='ios-handoff'&&item.status!=='running')html+=`<div class="chat-status">${esc(({review:'待验收',accepted:'已验收',archived:'已归档',idle:'待继续'})[item.status]||item.status)} · iOS App + Norma API</div>`;
    if(ui.view==='chat'&&ui.workspaceId==='ios-handoff'&&item.status!=='running')html+=`<div class="task-update"><strong>Codex · 调整 iOS 入口和 API 状态</strong><span>${esc(({review:'成果待验收',accepted:'成果已验收',archived:'工作空间已归档',idle:'等待新指令'})[item.status]||'任务状态已更新')}</span></div>`;
    if(ui.view==='chat'&&ui.workspaceId==='ios-handoff'&&item.messages.length>4)html+=`<section class="chat-updated" aria-label="最新对话"><b>最新对话</b>${item.messages.slice(4).map(message=>`<div class="chat-line"><strong>${esc(message.author)}</strong><span>${esc(message.text)}</span></div>`).join('')}</section>`;
    if(ui.view==='brief'&&ui.workspaceId==='ios-handoff'&&item.goal!=='让用户从 Mac 切到 iPhone 时，能看懂当前任务并继续操作。')html+=`<div class="override-card goal"><strong>当前目标</strong>${esc(item.goal)}</div>`;
    if(ui.view==='brief'&&item.hardRules.length!==1)html+=`<div class="override-card rule"><strong>不能越过</strong>${esc(item.hardRules.join(' · ')||'暂无硬约束')}</div>`;
    if(ui.view==='call'&&ui.workspaceId!=='ios-handoff')html+=`<div class="call-workspace">${esc(item.title)} · 当前工作空间</div>`;
    if(ui.view==='call'&&ui.voiceMuted)html+='<div class="voice-overlay">已静音</div>';
    if(ui.view==='call'&&model.voice?.lastLine!=='你可以直接说话，我记得这个频道的上下文。')html+=`<div class="voice-caption">${esc(model.voice.lastLine)}</div>`;
    return html;
  }

  function sheet(){
    if(!ui.sheet)return '';
    const item=workspace();
    let title='',body='';
    if(ui.sheet==='new'){
      title='保存新想法';body='<p>保存即创建工作空间。明确开始前，Agent 不会执行。</p><form id="idea-form"><label for="idea">新的想法</label><textarea id="idea" name="idea" required maxlength="240" placeholder="例如：让首次连接更安心"></textarea><button class="primary" type="submit">保存为工作空间</button></form>';
    }else if(ui.sheet==='settings'){
      title='工作空间设置';body=`<p>${esc(item.title)}</p><div class="settings-projects"><b>关联项目</b><span>${esc(item.projects.join(' · ')||'尚未关联项目')}</span></div>${link('添加已有项目','projects')}${link('查看工作简报','brief')}${link('已连接电脑','devices')}${item.status==='archived'?link('重开工作空间','reopen'):['idea','idle','accepted'].includes(item.status)?link('归档工作空间','archive','danger'):'<button type="button" disabled>归档工作空间 · 请先完成当前任务</button>'}`;
    }else if(ui.sheet==='scan'){
      title='扫码添加电脑';body='<p>在电脑端 Norma 打开“添加手机”，将配对码放入取景框。</p><div class="qr">对准配对二维码</div><p>已识别：Studio Mini · 在线</p>'+link('确认注册 Studio Mini','pair','primary');
    }else if(ui.sheet==='project-info'){
      title=ui.projectInfo;body=`<p>来自 ${ui.projectInfo==='Release Ops'?'Build Mac · 离线':'Alan Studio · 在线'}。进入工作空间设置后，可把这个已有项目关联到当前工作。</p>${link('前往工作空间','chat','primary')}`;
    }else if(ui.sheet==='edit-brief'){
      title='纠正目标与限制';body=`<form id="brief-form"><label for="goal">目标</label><textarea id="goal" name="goal" required maxlength="400">${esc(item.goal)}</textarea><label for="rules">硬约束，每行一条</label><textarea id="rules" name="rules" maxlength="500">${esc(item.hardRules.join('\n'))}</textarea><p class="hint">保存后会在工作空间留下变更回执。</p><button class="primary" type="submit">保存工作简报</button></form>`;
    }else if(ui.sheet==='compose'||ui.sheet==='voice-type'){
      title=ui.sheet==='voice-type'?'向 Norma 发文字':'发送工作空间消息';body='<form id="message-form"><label for="message">消息内容</label><textarea id="message" name="message" required maxlength="400" placeholder="@Codex 继续检查 API 状态"></textarea><button class="primary" type="submit">发送消息</button></form>';
    }else if(ui.sheet==='decision'){
      title='需要你的决定';body='<p>部署前检查触及“禁止发布生产环境”。Norma 已暂停发布动作，建议只在测试环境完成验证。</p>'+link('仅在测试环境继续','resolve-decision','primary');
    }else if(ui.sheet==='revise'){
      title='退回修改';body='<form id="revise-form"><label for="revision">修改意见</label><textarea id="revision" name="revision" required maxlength="300" placeholder="请把首屏状态再说明清楚"></textarea><button class="primary" type="submit">交回原任务</button></form>';
    }else if(ui.sheet==='search'){
      title='搜索工作空间';body='<form id="search-form"><label for="query">名称或内容</label><input id="query" name="query" placeholder="输入关键词" required><button class="primary" type="submit">搜索</button></form>';
    }else if(ui.sheet==='search-inbox'){
      title='搜索收件箱';body='<form id="inbox-search-form"><label for="inbox-query">事项或工作空间</label><input id="inbox-query" name="query" placeholder="例如：首屏交接" required><button class="primary" type="submit">搜索待办</button></form>';
    }else if(ui.sheet==='workspace-picker'){
      title='切换工作空间';body=model.channels.filter(channel=>channel.status!=='archived').map(channel=>link(channel.title,'pick-workspace')).join('');
    }else if(ui.sheet==='evidence'){
      title=ui.evidence;body=`<p>${esc(({'入口状态':'iPhone 首屏已显示当前任务，并可继续打开最近成果。','API 字段':'iOS 与 Norma API 的任务状态字段已核对一致。','关键流程':'从 Mac 切到 iPhone 后，继续任务的关键路径已验证。','首屏入口变更预览':'首屏入口已调整为一个主操作，并保留任务状态。','API 状态字段检查记录':'状态字段与 iOS 展示文案已经逐项核对。'})[ui.evidence]||'已关联到当前任务的验证记录。')}</p>${link('返回详情','close-sheet','primary')}`;
    }else if(ui.sheet==='attachments'){
      title='添加到工作空间';body='<p>把材料留在当前工作空间，所有参与者可见。</p>'+link('照片','attachment-demo')+link('截图','attachment-demo')+link('文件','attachment-demo');
    }else if(ui.sheet==='task-menu'){
      title='任务操作';body=link('在频道 @Codex','mention-codex')+link('查看工作简报','brief');
    }else if(ui.sheet==='preview'){
      title='iOS 首屏预览';body='<p>预览展示了跨端入口状态与最近成果。点击验收页的证据可查看验证说明。</p>'+link('返回成果验收','close-sheet','primary');
    }else if(ui.sheet==='decision-source'){
      title='决定来源';body='<p>这条决定来自当前工作空间对话，并已同步给参与的 Agent。</p>'+link('返回工作简报','close-sheet','primary');
    }
    return `<button type="button" class="scrim" aria-label="关闭弹层" data-action="close-sheet"></button><section class="sheet" role="dialog" aria-modal="true" aria-label="${esc(title)}"><div class="sheet-handle" aria-hidden="true"></div><button type="button" class="sheet-close" data-action="close-sheet" aria-label="完成">完成</button><h2>${esc(title)}</h2>${body}</section>`;
  }

  function render(){
    const screen=screens[ui.view]||screens.chat;
    const covered=(['chat','brief','projects'].includes(ui.view)&&ui.workspaceId!=='ios-handoff')||(ui.view==='inbox'&&ui.inboxTouched);
    app.innerHTML=`<div class="frame"><div class="artboard" data-view="${esc(ui.view)}"><img id="reference-screen" class="${ui.animateNext?'screen-enter':''}" src="${assets+screen}" alt="${covered?'':esc(screenDescriptions[ui.view])}"><div class="hotspots">${hotspots()}</div>${overrides()}<div class="notice${ui.notice?' visible':''}" role="status" aria-live="polite">${esc(ui.notice)}</div>${sheet()}</div></div>`;
    const thread=app.querySelector('.new-thread');if(thread)thread.scrollTop=thread.scrollHeight;
    const updates=app.querySelector('.chat-updated');if(updates)updates.scrollTop=updates.scrollHeight;
    ui.animateNext=false;
  }
  function show(view){if(view==='inbox'&&channelById(model,'ios-handoff').status==='running')act(model,{type:'complete',channelId:'ios-handoff'});clearTimeout(announce.timer);ui.notice='';ui.animateNext=view!==ui.view;ui.view=view;ui.sheet=null;render();}
  function announce(message){ui.notice=message;render();clearTimeout(announce.timer);announce.timer=setTimeout(()=>{ui.notice='';const notice=document.querySelector('.notice');if(notice){notice.classList.remove('visible');notice.textContent='';}},2800);}
  function openSheet(name){ui.sheet=name;render();requestAnimationFrame(()=>(document.querySelector('.sheet textarea,.sheet input')||document.querySelector('.sheet button'))?.focus());}
  function apply(type,extra={}){const result=act(model,{type,channelId:ui.workspaceId,...extra});announce(result.message);return result;}

  document.addEventListener('click',event=>{
    const target=event.target.closest('button[data-action]');if(!target||target.disabled)return;
    const action=target.dataset.action;
    if(action==='close-sheet'){ui.sheet=null;render();return;}
    if(action==='list'||action==='devices'||action==='brief'||action==='task'||action==='inbox'){show(action);return;}
    if(action==='open-ios'){ui.workspaceId='ios-handoff';show('chat');return;}
    if(action==='open-first'){ui.workspaceId='first-connect';show('chat');return;}
    if(action==='open-workspace'){ui.workspaceId=target.dataset.id;show('chat');return;}
    if(action==='chat'){show('chat');return;}
    if(action==='projects'){ui.projectReturn=ui.sheet==='settings'?'settings':ui.view==='brief'?'brief':'chat';show('projects');return;}
    if(action==='done-projects'){if(ui.projectReturn==='settings'){show('chat');openSheet('settings');}else show(ui.projectReturn);return;}
    if(action==='settings'||action==='new-idea'||action==='scan'||action==='search'||action==='search-inbox'||action==='compose'||action==='voice-type'||action==='attachments'||action==='task-menu'||action==='edit-brief'||action==='decision'||action==='revise'||action==='preview'||action==='decision-source'||action==='workspace-picker'){openSheet(action==='new-idea'?'new':action);return;}
    if(action==='project-info'){ui.projectInfo=target.dataset.project;openSheet('project-info');return;}
    if(action==='toggle-project'){apply('toggle-project',{project:target.dataset.project});return;}
    if(action==='offline-project'){announce('Build Mac 已离线，连接后才可加入执行范围');return;}
    if(action==='pair'){const result=act(model,{type:'pair-computer'});ui.sheet=null;announce(result.message);return;}
    if(action==='filter'){ui.filter=target.dataset.filter;announce('已筛选：'+({all:'全部',running:'执行中',idea:'构思中',archived:'已归档'}[ui.filter]));return;}
    if(action==='clear-inbox-search'){ui.inboxQuery='';render();return;}
    if(action==='start'){apply('start');return;}
    if(action==='persona'||action==='call'){if(model.voice)ui.workspaceId=model.voice.channelId;else act(model,{type:'call-start',channelId:ui.workspaceId});show('call');return;}
    if(action==='call-end'){const result=act(model,{type:'call-end',channelId:ui.workspaceId});show('chat');announce(result.message);return;}
    if(action==='voice-mute'){const result=act(model,{type:'call-mute',channelId:ui.workspaceId});if(result.ok)ui.voiceMuted=model.voice?.muted||false;announce(result.message);return;}
    if(action==='voice-demo'){const result=act(model,{type:'call-utterance',channelId:ui.workspaceId});announce(result.message);return;}
    if(action==='acceptance'){ui.workspaceId='ios-handoff';const item=workspace();if(item.status==='running')act(model,{type:'complete',channelId:item.id});show('acceptance');return;}
    if(action==='accept'){const result=act(model,{type:'accept',channelId:ui.workspaceId});if(result.ok){ui.inboxTouched=true;show('chat');}announce(result.message);return;}
    if(action==='resolve-decision'){const result=act(model,{type:'resolve-decision',channelId:'release-check'});if(result.ok)ui.inboxTouched=true;ui.sheet=null;announce(result.message);return;}
    if(action==='archive'||action==='reopen'){const result=apply(action);if(result.ok)ui.sheet=null;render();return;}
    if(action==='mention-codex'){show('chat');openSheet('compose');requestAnimationFrame(()=>{const input=document.getElementById('message');if(input){input.value='@Codex ';input.focus();}});return;}
    if(action==='evidence'){ui.evidence=target.dataset.evidence;openSheet('evidence');return;}
    if(action==='attachment-demo'){announce('已选中附件入口 · 原型不读取本机文件');return;}
    if(action==='pick-workspace'){const next=model.channels.find(item=>item.title===target.textContent);if(!next)return;if(model.voice)act(model,{type:'call-end',channelId:model.voice.channelId});ui.workspaceId=next.id;act(model,{type:'call-start',channelId:next.id});ui.voiceMuted=false;show('call');return;}
  });
  document.addEventListener('submit',event=>{
    const form=event.target;if(!['idea-form','brief-form','message-form','revise-form','search-form','inbox-search-form'].includes(form.id))return;
    event.preventDefault();const data=new FormData(form);
    if(form.id==='idea-form'){const result=act(model,{type:'save',text:data.get('idea')});if(result.ok){ui.workspaceId=result.channelId;show('chat');announce(result.message);}else announce(result.message);return;}
    if(form.id==='brief-form'){const result=act(model,{type:'update-brief',channelId:ui.workspaceId,goal:data.get('goal'),hardRules:String(data.get('rules')||'').split('\n')});if(result.ok)ui.sheet=null;announce(result.message);return;}
    if(form.id==='message-form'){const result=act(model,{type:'send',channelId:ui.workspaceId,text:data.get('message')});if(result.ok){ui.sheet=null;if(ui.view==='call'&&model.voice)model.voice.lastLine=workspace().messages.at(-1).text;}announce(result.message);return;}
    if(form.id==='revise-form'){const result=act(model,{type:'revise',channelId:ui.workspaceId,text:data.get('revision')});if(result.ok){show('chat');announce(result.message);}else announce(result.message);return;}
    if(form.id==='search-form'){const query=String(data.get('query')||'').trim();const found=model.channels.find(item=>item.title.includes(query));if(found){ui.workspaceId=found.id;show('chat');announce('已打开 '+found.title);}else announce('没有找到工作空间');}
    if(form.id==='inbox-search-form'){ui.inboxQuery=String(data.get('query')||'').trim();ui.inboxTouched=true;ui.sheet=null;render();}
  });
  let sheetDrag=null;
  document.addEventListener('pointerdown',event=>{
    const handle=event.target.closest('.sheet-handle');if(!handle)return;
    handle.setPointerCapture(event.pointerId);
    const panel=handle.closest('.sheet');
    panel.style.animation='none';
    sheetDrag={pointerId:event.pointerId,startY:event.clientY,panel};
  });
  document.addEventListener('pointermove',event=>{
    if(!sheetDrag||event.pointerId!==sheetDrag.pointerId)return;
    sheetDrag.panel.style.transform=`translateY(${Math.max(0,event.clientY-sheetDrag.startY)}px)`;
  });
  const finishSheetDrag=event=>{
    if(!sheetDrag||event.pointerId!==sheetDrag.pointerId)return;
    const {startY,panel}=sheetDrag;sheetDrag=null;
    if(event.type==='pointerup'&&event.clientY-startY>70){ui.sheet=null;render();return;}
    panel.style.transition='transform 180ms ease-out';
    panel.style.transform='translateY(0)';
  };
  document.addEventListener('pointerup',finishSheetDrag);
  document.addEventListener('pointercancel',finishSheetDrag);
  document.addEventListener('keydown',event=>{if(event.key==='Escape'&&ui.sheet){ui.sheet=null;render();return;}if(event.key==='Escape'&&ui.view!=='list'){show('list');}});
  render();
})();
