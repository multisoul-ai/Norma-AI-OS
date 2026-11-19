// 数据模型：Workspace → Project → Task(phase, messages[], workers[]) ；Worker = 任务派出的一次执行
// 任务展示状态不手写，由 deriveStatus(task) 从 phase 与存活 Worker（非 Reclaimed）推导。
const ORDER = ['needs','running','waiting','merge','done'];
const AGENTS = {
  Grace:{engine:'Codex',color:'#a8c5cd'}, Iris:{engine:'Claude Code',color:'#c2b2db'},
  Jasper:{engine:'Codex',color:'#dbb68e'}, Kevin:{engine:'Codex',color:'#a3c1dd'},
  Henry:{engine:'Codex',color:'#a9cbb5'}, Luna:{engine:'Codex',color:'#b7c3ee'}, Echo:{engine:'Claude Code',color:'#d5b5c6'}
};
// worker.state: Active | Reclaimed(cardId 为空，保留 engineSessionId) | Failed | Interrupted
// task.phase: Active | Merging | Done；msg.resolved 表示幕僚的建议已被用户处理
const W = (agent, role, state, id, note, extra={}) => ({agent, engine:AGENTS[agent].engine, role, state, cardId:state==='Reclaimed'?null:'card-'+id, dispatchId:'d-'+id+'-1', engineSessionId:'es-'+id, note, ...extra});
const tasks = [
 {id:'t1', project:'Norma-AI-OS', title:'重做幕僚模式页面', updated:'刚刚', phase:'Active',
  workers:[W('Kevin','交互与布局原型','Active','k1','正在打磨三栏布局'), W('Iris','数据结构与文档','Active','i1','正在改 Task/Worker 文档'), W('Grace','需求调研','Reclaimed','g1','已回收，可唤起继续')],
  files:[{path:'docs/prototypes/norma-staff-mode/index.html',tag:'新增'},{path:'docs/prototypes/norma-staff-mode/README.md',tag:'新增'},{path:'docs/design-docs/norma-staff-mode.md',tag:'修改'}],
  messages:[
   {who:'you',text:'基于底部抽屉原型，重做成幕僚模式：左任务、中对话、右文件预览。'},
   {who:'norma',text:'我把它拆成三个 Worker，分别派出，你不用逐个盯：',plan:['Kevin','Iris','Grace']},
   {who:'event',agent:'Grace',text:'Grace 完成调研，已被回收，会话 es-g1 已保留。'},
   {who:'event',agent:'Kevin',text:'Kevin 完成了三栏骨架，正在调整状态分组。'}]},
 {id:'t2', project:'Norma-AI-OS', title:'排查画布拖动卡顿', updated:'12 分钟前', phase:'Active',
  workers:[W('Grace','日志与采样分析','Failed','g2','采样进程异常退出'), W('Jasper','卡顿复现','Interrupted','j2','重启后查不到引擎会话'), W('Iris','渲染剖析','Active','i2','等待采样结果')],
  files:[{path:'observation-20261104.jsonl',tag:'新增'}],
  messages:[{who:'you',text:'拖动、缩放时偶尔会卡顿，帮我分析最近的日志。'},{who:'norma',text:'已派出三个 Worker。',plan:['Grace','Jasper','Iris']},
   {who:'event',agent:'Grace',text:'Grace 失败：采样进程异常退出（exit 137）。'},{who:'event',agent:'Jasper',text:'Jasper 中断：重启对账时查不到引擎会话。'},
   {who:'norma',text:'Grace 失败、Jasper 中断。我建议重派 Grace，Jasper 先回收；不会自动重试，避免重复写操作。',ask:true}]},
 {id:'t3', project:'Norma-AI-OS', title:'修复流星效果缺失', updated:'1 小时前', phase:'Active', workers:[], files:[],
  messages:[{who:'you',text:'流星效果缺失，帮我排查。'},{who:'norma',text:'等待拆解：我会先在干净 dev 上确认现象，再给出 Worker 提议。'}]},
 {id:'t4', project:'Norma-AI-OS', title:'验收 PR #182 的打包修改', updated:'2 小时前', phase:'Merging',
  workers:[W('Jasper','打包与 CI','Active','j4','CI 通过，等待你验收')],
  files:[{path:'scripts/package-release.sh',tag:'修改'},{path:'docs/references/packaging.md',tag:'修改'}],
  messages:[{who:'you',text:'这些修改后打包会变多大？还有哪些风险需要确认？'},{who:'norma',text:'PR #182 已更新，交叉 review 与最新 CI 通过，尚未合并。请确认体积与启动表现后再验收。'}]},
 {id:'t5', project:'Norma Website', title:'完善官网下载体验', updated:'3 小时前', phase:'Active',
  workers:[W('Luna','下载页实现','Active','l5','检查移动端布局'), W('Echo','发布说明','Reclaimed','e5','等下载页完成后再唤起')],
  files:[{path:'site/src/download.tsx',tag:'修改'}],
  messages:[{who:'you',text:'优化官网 macOS 下载体验。'},{who:'norma',text:'Luna 做页面，Echo 随后写发布说明。',plan:['Luna','Echo']}]},
 {id:'t6', project:'Norma-AI-OS', title:'清理构建缓存', updated:'昨天', phase:'Done',
  workers:[W('Henry','清理与复查','Reclaimed','h6','已恢复 17.4 GiB')], files:[],
  messages:[{who:'you',text:'看看哪些构建产物可以删除。'},{who:'norma',text:'已删除 9 处构建缓存，磁盘可用空间从 118 MiB 恢复到 17.4 GiB，源码与验收证据均保留。'}]}
];
// 未归属卡片：没有任何 Task.workers 引用的画布卡片（存量 / 手动打开）
const orphans = [
  {id:'o1', agent:'Echo', engine:'Claude Code', cardId:'card-o1', role:'手动打开的终端', note:'cargo test -p brain 运行中'},
  {id:'o2', agent:'Luna', engine:'Codex', cardId:'card-o2', role:'旧 Goal 遗留卡片', note:'空闲，上次活动 2 天前'}
];
const preview = {
 'docs/prototypes/norma-staff-mode/index.html':'<body data-mode="staff">\n  <header id="topbar"></header>\n  <main id="canvas"></main>\n  <section id="staff"></section>\n</body>',
 'docs/prototypes/norma-staff-mode/README.md':'# Norma 幕僚模式\n\nNorma 是幕僚：一次会话 = 一个任务，一个任务由多个 Worker 协作完成。\n\n## 关系\n\n1. 工作空间包含多个项目\n2. 项目下有多个任务\n3. 任务下有多个 Worker（0..n）\n\n## 待确认\n\n- Worker 不跨任务移动',
 'docs/design-docs/norma-staff-mode.md':'## 任务状态推导\n\nphase=Done → 已完成；Merging → 待合并\n有 Failed/Interrupted/待你决定 → 需要你\n有 Active → 处理中；其余 → 等待中',
 'scripts/package-release.sh':'-  tar czf "$OUT" "$APP"\n+  tar --exclude="*.dSYM" -czf "$OUT" "$APP"',
};
