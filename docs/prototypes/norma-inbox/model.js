const $=s=>document.querySelector(s),$$=s=>[...document.querySelectorAll(s)];
let lang='zh';const tr=(zh,en)=>lang==='zh'?zh:en,l=v=>Array.isArray(v)?v[lang==='zh'?0:1]:v;
const esc=s=>String(s??'').replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
const labels={needs:['需要你','Needs you'],running:['处理中','Working'],waiting:['等待中','Waiting'],done:['已回答','Answered'],merge:['待合并','Awaiting merge'],planning:['等待拆解','Awaiting planning'],archive:['已归档','Archived'],all:['所有工作空间','All workspaces'],projects:['所有项目','All projects'],close:['关闭','Close'],send:['发送','Send'],locate:['定位原卡片','Locate original card'],return:['返回任务页','Return to tasks'],canvas:['画布','Canvas'],draft:['草稿','Draft'],coordinator:['协调多个 Agent','Coordinate your agents'],localReply:['消息已记录在这张卡片的示例会话中。正式接入后，Agent 会沿用这里的上下文继续执行。','Message recorded in this card’s sample session. Once connected, the agent would continue with the same context.'],local:['交互原型 · 未连接真实 Agent','Interactive prototype · No live agents connected']};
const t=k=>labels[k]?l(labels[k]):k;
const state={items:structuredClone(works),scope:{space:'all',project:'all'},destination:{space:'product',project:'norma'},recipient:'norma',newDraft:'',active:null,contexts:new Map(),archiveOpen:false,search:'',searchOpen:false,leftScroll:0,eventAdded:false,menu:null,canvasId:null,camera:{x:80,y:110,z:.86},next:0};
state.items.forEach(a=>{a.archived=a.id==='cleanup';a.unread=false;});
Object.assign(state.items.find(a=>a.id==='drawer'),{title:['打磨全局任务页的交互','Refine the global task page'],summary:['正在验证快捷键和原会话衔接','Checking shortcuts and shared conversations'],initial:['保留画布，用一个全局入口管理任务，点开就能继续原卡片对话。','Keep the canvas. Add a global task entry that continues original card conversations.'],answer:['任务页布局已经完成，正在验证工作空间筛选、独立草稿和画布定位。每条工作继续对应原来的 Agent 卡片。','The task layout is ready. I’m checking workspace filters, separate drafts and canvas navigation. Each work item remains linked to its original agent card.']});
const byId=id=>state.items.find(a=>a.id===id);
const spaceName=id=>l(spaces.find(s=>s.id===id)?.name||id);
const identity=a=>[a.space,a.project,a.card||a.id,a.session].join('/');
function ctx(a){const key=identity(a);if(!state.contexts.has(key))state.contexts.set(key,{draft:'',messages:[],scroll:0,attachment:null});return state.contexts.get(key);}
function matchesScope(a){return(state.scope.space==='all'||a.space===state.scope.space)&&(state.scope.project==='all'||a.project===state.scope.project);}
function listed(){return state.items.filter(a=>matchesScope(a)&&(!state.search||[a.agent,l(a.title),spaceName(a.space),projects[a.project].name].join(' ').toLowerCase().includes(state.search.toLowerCase())));}
function group(a){return a.archived?'archive':a.status==='needs'||a.status==='done'?'needs':a.status==='running'?'running':'waiting';}
const scopeName=()=>state.scope.space==='all'?t('all'):spaceName(state.scope.space)+(state.scope.project==='all'?'':' / '+projects[state.scope.project].name);
const receiver=()=>state.recipient==='norma'?null:byId(state.recipient);
const active=()=>byId(state.active);
function sendMessage(a,text){if(!text.trim())return;const c=ctx(a);c.messages.push({role:'user',text:text.trim(),attachment:c.attachment?.name},{role:'assistant',text:labels.localReply,sample:true});c.draft='';c.attachment=null;c.scroll=Infinity;}
function menuOptions(kind){
 if(kind==='scope'||kind==='destination')return [...(kind==='scope'?[{id:'all/all',title:labels.all,detail:['跨空间查看全部任务','View tasks across workspaces'],space:'all',project:'all',type:'scope'}]:[]),...spaces.flatMap(s=>[...(kind==='scope'?[{id:s.id+'/all',title:s.name,detail:[s.projects.length+' 个项目',s.projects.length+' projects'],space:s.id,project:'all',type:'scope'}]:[]),...s.projects.map(p=>({id:s.id+'/'+p,title:projects[p].name,detail:s.name,space:s.id,project:p,type:'project'}))])];
 return [{id:'norma',title:'Norma',detail:labels.coordinator,type:'norma'},...state.items.filter(a=>a.card).map(a=>({id:a.id,title:a.agent,detail:[spaceName(a.space)+' / '+projects[a.project].name,spaceName(a.space)+' / '+projects[a.project].name],agent:a,type:'agent'}))];
}
function filteredOptions(){const m=state.menu;return menuOptions(m.kind).filter(o=>!m.query||[l(o.title),l(o.detail)].join(' ').toLowerCase().includes(m.query.toLowerCase()));}
function selectedOption(kind){return kind==='recipient'?state.recipient:state[kind].space+'/'+state[kind].project;}
