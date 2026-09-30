// Throwaway prototype state. No network, storage, microphone or real Agent execution.
function initialState() {
  return {
    nextId: 20,
    voice: null,
    machines: [
      {id:'alan-studio',name:'Alan Studio',online:true,projects:['iOS App','Norma API']},
      {id:'build-mac',name:'Build Mac',online:false,projects:['Release Ops']}
    ],
    channels: [
      {
        id:'ios-handoff', title:'改进 iOS 首屏交接', status:'running', updated:'刚刚',
        goal:'让用户从 Mac 切到 iPhone 时，能看懂当前任务并继续操作。',
        summary:'首屏聚焦任务状态与最近成果；iOS 和 API 一起调整。',
        projects:['iOS App','Norma API'], members:['Norma','Codex','Design'],
        hardRules:['禁止发布生产环境'], decisions:['首屏只呈现任务状态与最近成果','交接只保留一个主操作'],
        messages:[
          {id:'m1',author:'你',text:'@Norma，把首屏交接做顺。iOS 和 API 都看一下。',time:'09:41'},
          {id:'m2',author:'Norma',text:'我会让 Design 检查信息层级，Codex 处理两端状态。两项工作共享这个频道的目标和决定。',time:'09:42'},
          {id:'m3',author:'Design',text:'已接收。先梳理引导文案和首屏层级。',time:'09:43'},
          {id:'m4',author:'Codex',text:'已接收。正在调整 iOS 入口，并核对 API 返回。',time:'09:43'}
        ],
        tasks:[
          {id:'task-codex',agent:'Codex',title:'调整 iOS 入口和状态',project:'iOS App · Norma API',status:'running',progress:68,detail:'已调整 3 处入口状态，正在验证两端数据。',evidence:['修改 3 处入口状态','API 返回字段对照完成']},
          {id:'task-design',agent:'Design',title:'首屏文案与层级',project:'iOS App',status:'running',progress:84,detail:'已整理 3 处引导文案，等待预览。',evidence:['3 处文案草案','层级对照图']}
        ], artifact:null, pendingDecision:false, failure:null
      },
      {
        id:'first-connect',title:'新人首次连接体验',status:'idea',updated:'昨天',
        goal:'第一次打开 iPhone 时，让用户顺利连上自己的 Mac。',
        summary:'记录了想法，尚未安排执行。',projects:[],members:['Norma'],hardRules:[],decisions:[],
        messages:[
          {id:'m5',author:'你',text:'第一次配对电脑的过程可以更安心一些。',time:'昨天 18:20'},
          {id:'m6',author:'Norma',text:'已记到这个频道。需要开始时告诉我。',time:'昨天 18:20'}
        ],tasks:[],artifact:null,pendingDecision:false,failure:null
      },
      {
        id:'release-check',title:'部署前检查',status:'failed',updated:'10 分钟前',
        goal:'在不触碰生产环境的情况下完成发布前验证。',summary:'测试环境缺少依赖，等待你决定下一步。',
        projects:['Release Ops'],members:['Norma','Codex'],hardRules:['禁止发布生产环境'],decisions:[],
        messages:[
          {id:'m7',author:'Codex',text:'测试环境下载依赖失败；没有执行生产发布。',time:'10:18'},
          {id:'m8',author:'Norma',text:'我已暂停这条验证路径。可以改用已缓存的依赖继续吗？',time:'10:19'}
        ],tasks:[{id:'task-release',agent:'Codex',title:'验证发布前检查',project:'Release Ops',status:'failed',progress:55,detail:'测试环境下载依赖失败，等待处理。',evidence:['依赖下载失败日志']}],
        artifact:null,pendingDecision:true,failure:'测试环境下载依赖失败'
      }
    ]
  };
}
function channelById(state, id) { return state.channels.find(channel => channel.id === id); }
function pushMessage(state, channel, author, text, kind) {
  channel.messages.push({id:'m'+state.nextId++,author,text,time:'刚刚',...(kind?{kind}:{})});
  channel.updated='刚刚';
}
function inboxItems(state) {
  return state.channels.flatMap(channel => {
    const items=[];
    if(channel.status==='review') items.push({type:'review',channelId:channel.id,title:channel.title,detail:'成果已准备好，等待你查看与验收'});
    if(channel.pendingDecision) items.push({type:'decision',channelId:channel.id,title:channel.title,detail:'是否改用已缓存的依赖继续验证？'});
    if(channel.failure && !channel.pendingDecision) items.push({type:'failed',channelId:channel.id,title:channel.title,detail:channel.failure});
    return items;
  });
}
function act(state, action) {
  if(action.type==='pair-computer') {
    if(state.machines.some(machine => machine.id==='studio-mini')) return {ok:false,message:'Studio Mini 已注册。'};
    state.machines.push({id:'studio-mini',name:'Studio Mini',online:true,projects:[]});
    return {ok:true,message:'Studio Mini 已注册 · 在电脑端添加项目后即可关联频道'};
  }
  if(action.type==='save') {
    const text=(action.text||'').trim();
    if(!text) return {ok:false,message:'先写下一个想法。'};
    const id='channel-'+state.nextId++;
    const title=text.replace(/[。！!？?].*$/,'').slice(0,18);
    state.channels.unshift({id,title,status:'idea',updated:'刚刚',goal:text,summary:'刚记录的想法，尚未执行。',projects:[],members:['Norma'],hardRules:[],decisions:[],messages:[],tasks:[],artifact:null,pendingDecision:false,failure:null});
    const channel=state.channels[0];
    pushMessage(state,channel,'你',text);
    pushMessage(state,channel,'Norma','已保存为频道。你可以继续讨论，明确要做时我再派工。');
    return {ok:true,channelId:id,message:'想法已保存为频道 · 尚未执行'};
  }
  const channel=channelById(state,action.channelId);
  if(!channel) return {ok:false,message:'找不到这个频道。'};
  if(channel.status==='archived' && action.type!=='reopen') return {ok:false,message:'先重开频道，再继续操作。'};
  if(action.type==='update-brief') {
    const goal=String(action.goal||'').trim();
    if(!goal) return {ok:false,message:'先写清楚这件事的目标。'};
    channel.goal=goal;
    channel.hardRules=(action.hardRules||[]).map(rule=>String(rule).trim()).filter(Boolean);
    pushMessage(state,channel,'你','更新工作简报：'+goal+'；不能越过：'+(channel.hardRules.join('、')||'暂无')+'。','brief');
    pushMessage(state,channel,'Norma','工作简报已更新。我会让参与 Agent 按新的目标和禁令继续，并把变化留在频道。','receipt');
    return {ok:true,message:'工作简报已更新 · 参与 Agent 已知悉'};
  }
  if(action.type==='toggle-project') {
    const project=String(action.project||'');
    if(!state.machines.some(machine => machine.projects.includes(project))) return {ok:false,message:'这个项目尚未从已注册电脑同步。'};
    const index=channel.projects.indexOf(project);
    if(index>=0) {
      if(channel.tasks.some(task => task.status==='running' && task.project.split(' · ').includes(project))) return {ok:false,message:'该项目有进行中的任务，完成后再移除。'};
      channel.projects.splice(index,1);
    } else channel.projects.push(project);
    channel.projectsConfigured=true;
    pushMessage(state,channel,'你',(index>=0?'移除':'关联')+'项目：'+project+'。','project');
    return {ok:true,message:project+(index>=0?' 已从频道移除':' 已关联到频道')};
  }
  if(action.type==='start') {
    if(!['idea','idle'].includes(channel.status)) return {ok:false,message:'这个频道已经在执行或等待处理。'};
    const projects=channel.projectsConfigured?channel.projects:['iOS App','Norma API'].filter(project=>state.machines.some(machine=>machine.projects.includes(project)));
    if(!projects.length) return {ok:false,message:'请先关联一个已注册电脑上的项目。'};
    channel.status='running';channel.projects=[...projects];channel.members=['Norma','Codex','Design'];
    channel.tasks=[
      {id:'task-'+state.nextId++,agent:'Design',title:'梳理体验和文案',project:projects[0],status:'running',progress:12,detail:'已接收，开始梳理。',evidence:[]},
      {id:'task-'+state.nextId++,agent:'Codex',title:'实现与验证',project:projects.join(' · '),status:'running',progress:8,detail:'已接收，开始核对项目状态。',evidence:[]}
    ];
    pushMessage(state,channel,'Norma','已在 '+projects.join(' 与 ')+' 派工；我把体验交给 Design，把实现与验证交给 Codex。不会越过频道硬约束。','dispatch');
    pushMessage(state,channel,'Design','已接收。先梳理体验和文案。','receipt');
    pushMessage(state,channel,'Codex','已接收。开始核对关联项目状态。','receipt');
    return {ok:true,message:'Norma 已派工 · 2 位 Agent 接收'};
  }
  if(action.type==='send') {
    const text=(action.text||'').trim();
    if(!text) return {ok:false,message:'请输入消息。'};
    pushMessage(state,channel,'你',text);
    if(channel.status==='idea' && /^(开始|执行|做这个)/.test(text)) return act(state,{type:'start',channelId:channel.id});
    const mentions=[...new Set([...text.matchAll(/@(Norma|Codex|Design)\b/gi)].map(match => ({norma:'Norma',codex:'Codex',design:'Design'})[match[1].toLowerCase()]))];
    if(mentions.length===0) mentions.push('Norma');
    for(const agent of mentions) {
      if(agent==='Norma') pushMessage(state,channel,'Norma',channel.status==='idea'?'收到，我先和你讨论；你明确要做时再开始执行。':'收到。我会继续协调，并把重要变化留在这个频道。','reply');
      else {
        if(!channel.members.includes(agent)) channel.members.push(agent);
        pushMessage(state,channel,agent,'收到。我会沿当前频道目标和硬约束处理，并把结果回到这里。','reply');
      }
    }
    return {ok:true,message:mentions.join('、')+' 已回应'};
  }
  if(action.type==='call-start') {
    if(state.voice) return {ok:false,message:'已有一通对话进行中。'};
    state.voice={channelId:channel.id,muted:false,quiet:false,lastLine:'你可以直接说话，我记得这个频道的上下文。'};
    return {ok:true,message:'已进入频道实时对话'};
  }
  if(action.type==='call-utterance') {
    if(state.voice?.channelId!==channel.id) return {ok:false,message:'请先进入这个频道的对话。'};
    if(state.voice.muted) return {ok:false,message:'麦克风已静音，请先取消静音。'};
    const text='@Codex 再核对一下 API 状态，接着原任务做。';
    pushMessage(state,channel,'你',text,'transcript');
    pushMessage(state,channel,'Codex','收到，我会在原任务继续核对 API 状态。','receipt');
    state.voice.lastLine='已转给 Codex，原任务继续。';
    return {ok:true,message:'语音指令已转写并送达原任务'};
  }
  if(action.type==='call-end') {
    if(state.voice?.channelId!==channel.id) return {ok:false,message:'当前频道没有进行中的对话。'};
    state.voice=null;
    pushMessage(state,channel,'Norma','对话已结束。转写与 Agent 回执已留在频道，任务继续执行。','call-end');
    return {ok:true,message:'对话已结束 · 任务继续'};
  }
  if(action.type==='call-mute' || action.type==='call-quiet') {
    if(state.voice?.channelId!==channel.id) return {ok:false,message:'当前频道没有进行中的对话。'};
    const key=action.type==='call-mute'?'muted':'quiet';state.voice[key]=!state.voice[key];
    return {ok:true,message:key==='muted'?(state.voice.muted?'已静音':'已取消静音'):(state.voice.quiet?'普通进展将延后汇报':'已恢复汇报')};
  }
  if(action.type==='complete') {
    if(channel.status!=='running') return {ok:false,message:'这个频道没有进行中的任务。'};
    channel.tasks.forEach(task => {task.status='done';task.progress=100;});
    channel.status='review';
    channel.artifact={title:channel.id==='ios-handoff'?'iOS 首屏交接 · 第 2 版':'频道目标已完成',evidence:['iOS 入口和 API 状态已联动','关键流程检查通过','手机预览与修改记录已准备好'],accepted:false};
    pushMessage(state,channel,'Norma','两项任务已完成，成果与验证记录已准备好。请你查看后验收。','artifact');
    return {ok:true,message:'成果已进入收件箱 · 待验收'};
  }
  if(action.type==='revise') {
    if(channel.status!=='review') return {ok:false,message:'目前没有待修改的成果。'};
    const text=(action.text||'请再调整首屏层级。').trim();
    channel.status='running';channel.artifact=null;
    channel.tasks.forEach(task => {task.status='running';task.progress=80;});
    pushMessage(state,channel,'你','退回修改：'+text);
    pushMessage(state,channel,'Norma','已把修改意见送回原任务，Agent 会继续处理。','receipt');
    return {ok:true,message:'已退回原任务继续修改'};
  }
  if(action.type==='accept') {
    if(channel.status!=='review'||!channel.artifact) return {ok:false,message:'目前没有可验收的成果。'};
    channel.status='accepted';channel.artifact.accepted=true;
    pushMessage(state,channel,'你','成果验收通过。');
    return {ok:true,message:'已验收 · 可归档频道'};
  }
  if(action.type==='resolve-decision') {
    if(!channel.pendingDecision) return {ok:false,message:'没有待处理的决定。'};
    channel.pendingDecision=false;channel.failure=null;channel.status='running';
    channel.tasks.forEach(task => {task.status='running';task.detail='使用已缓存依赖继续验证，不触碰生产环境。';});
    pushMessage(state,channel,'你','同意用已缓存依赖继续验证。');
    pushMessage(state,channel,'Norma','已恢复验证；生产发布禁令仍有效。','receipt');
    return {ok:true,message:'决定已记录 · 验证继续'};
  }
  if(action.type==='archive') {
    if(!['idea','idle','accepted'].includes(channel.status)) return {ok:false,message:'先完成当前任务或验收，再归档频道。'};
    channel.status='archived';channel.updated='刚刚';
    return {ok:true,message:'频道已归档 · 可随时重开'};
  }
  if(action.type==='reopen') {
    if(channel.status!=='archived') return {ok:false,message:'这个频道尚未归档。'};
    channel.status='idle';channel.updated='刚刚';
    pushMessage(state,channel,'Norma','频道已重开。此前的决定、消息和成果仍在这里。','system');
    return {ok:true,message:'频道已重开'};
  }
  return {ok:false,message:'未识别的操作。'};
}
const NormaModel={initialState,act,inboxItems,channelById};
if(typeof module!=='undefined'&&module.exports) module.exports=NormaModel;
if(typeof window!=='undefined') window.NormaModel=NormaModel;
