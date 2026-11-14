const agents=[
{id:'simon',name:'Simon',initial:'S',color:'#c7cfa8',model:'Claude Code',role:['交互与修复','Interaction & fixes'],x:750,y:490,control:'agent'},
{id:'luna',name:'Luna',initial:'L',color:'#a9c8ef',model:'Codex',role:['画布导航','Canvas navigation'],x:1320,y:450,control:'agent'},
{id:'atlas',name:'Atlas',initial:'A',color:'#e6c495',model:'Claude Code',role:['交付与预览','Delivery & preview'],x:1910,y:850,control:'agent'},
{id:'nova',name:'Nova',initial:'N',color:'#9fd0c9',model:'Codex',role:['验证','Validation'],x:1260,y:1080,control:'agent'},
{id:'echo',name:'Echo',initial:'E',color:'#c5b7d9',model:'Claude Code',role:['文档','Documentation'],x:650,y:1140,control:'agent'},
{id:'iris',name:'Iris',initial:'I',color:'#dbbca7',model:'Codex',role:['视觉规范','Visual conventions'],x:2150,y:1350,control:'agent'}
];
const work=[
{id:'login',owner:'simon',title:['修复登录后白屏','Fix the post-login blank screen'],stage:'attention',last:['已定位旧会话恢复的问题，等待产品选择','Session restoration issue found; awaiting a product choice'],plan:[['复现旧会话失效','Reproduce stale sessions'],['保留数据并修复恢复逻辑','Preserve data and fix restoration'],['验证重新登录与数据保留','Verify re-login and data retention']]},
{id:'navigation',owner:'luna',title:['优化卡片定位与返回','Improve card location & return'],stage:'ready',last:['定位与返回已可体验，等待本轮验收','Location and return are ready for review'],plan:[['记录原画布位置','Keep the original canvas location'],['定位目标卡片','Locate the target card'],['验证返回与上下文保留','Verify return and retained context']]},
{id:'preview',owner:'atlas',title:['更新团队预览环境','Update the team preview'],stage:'approval',last:['构建已准备，等待共享预览部署授权','Build ready; waiting for preview deployment permission'],plan:[['完成构建与检查','Complete build and checks'],['部署团队预览','Deploy the team preview'],['核对访问与回滚点','Verify access and rollback point']]},
{id:'regression',owner:'nova',title:['补齐工作台回归验证','Complete workbench regression checks'],stage:'running',last:['正在验证双语与交互状态，暂不需要你处理','Checking locales and interaction states; no decision needed'],plan:[['整理验收场景','Prepare acceptance scenarios'],['检查双语与交互','Check locales and interactions'],['整理验证结果','Collect validation results']]},
{id:'notes',owner:'echo',title:['整理新版发布说明','Prepare the release notes'],stage:'waiting',last:['等待验证结果，不需要你转述进度','Waiting for validation results; no relay needed'],plan:[['读取验证结果','Read validation results'],['整理用户可见变化','Summarize visible changes'],['交付发布说明','Deliver release notes']]},
{id:'visual',owner:'iris',title:['统一工作台视觉规范','Unify workbench visual conventions'],stage:'done',last:['视觉规范已交付，记录保持可追溯','Visual conventions delivered; the record remains available'],plan:[['统一主题与间距','Unify theme and spacing'],['补齐交互状态','Complete interaction states'],['交付视觉规范','Deliver visual conventions']]}
];
const decisions=[
{id:'login',kind:'question',title:['修复旧登录时，允许用户重新登录一次吗？','May stale sessions ask users to sign in once more?'],recommended:['保留本地数据，允许一次重新登录','Keep local data and allow one fresh sign-in'],reason:['避免扩大改造范围；用户会看到一次登录提示。','Keep this fix focused; affected users see one sign-in prompt.'],evidence:['旧凭据无法可靠恢复；要保持无感登录，需要进一步调查。','Stale credentials cannot be restored reliably. Seamless recovery needs more investigation.'],impact:['过期登录用户需重新登录一次；保留本地数据，不发布版本。','Affected users sign in once more. Local data stays. No release.'],status:'open',deferred:false,version:1},
{id:'preview',kind:'permission',title:['允许 Atlas 更新团队共享预览吗？','May Atlas update the shared team preview?'],recommended:['仅部署预览环境，保留当前线上版本','Deploy the preview; keep production unchanged'],reason:['先验证改动，避免把未验收的结果直接发布。','Review the changes before a production release.'],evidence:['构建与预览配置已准备；本次操作会更新团队共享链接。','Build and preview configuration are ready. This updates the shared team preview.'],impact:['仅本次团队预览部署，需要你的明确允许。','This team preview deployment only. It needs your permission.'],status:'open',deferred:false,version:1},
{id:'navigation',kind:'review',title:['卡片定位与返回，可以验收这一轮了','Card location & return are ready for review'],recommended:['检查现场，确认定位和返回符合预期','Check the canvas, then accept this revision'],reason:['先体验真实路径；本轮验收通过也不会自动合并。','Try the actual path. Acceptance does not merge changes.'],evidence:['Luna 提供了定位、返回、输入保留三项验收步骤。','Luna prepared three checks: location, return, and draft retention.'],impact:['确认这一轮交互结果；合并与发布分别安排。','Accept this interaction revision. Merge and release remain separate.'],status:'open',deferred:false,version:1}
];
const seedMessages={
 login:[
 {role:'user',time:'12:04',text:['老账号登录后有时会白屏。请先修好，给我能验证的结果，先不要发布。','Returning users sometimes get a blank screen after login. Please fix it and give me a verifiable result. Do not release yet.']},
 {role:'assistant',time:'12:04',text:['这件事已交给 Simon。目标是修复旧登录并验证数据保留，发布会单独征求你的决定。\n后续补充都留在这条任务里，你可以同时推进其他工作。','Simon is handling this work: fix returning-user login and verify data retention. Release will be a separate decision.\nKeep follow-ups in this thread while other work continues.']},
 {role:'assistant',time:'12:16',text:['Simon 已定位到旧会话恢复路径：部分过期凭据无法可靠恢复，本地内容可以保留。\n现在需要你确认的是用户体验：这次是否接受让受影响的用户重新登录一次？','Simon traced the issue to session restoration: some expired credentials cannot be restored reliably, while local content can stay.\nThe product decision is whether affected users may sign in once more.'],citation:'Simon',decision:'login'}],
 preview:[
 {role:'user',time:'11:50',text:['给团队准备一个能检查工作台改动的预览链接，别直接发布线上。','Prepare a team preview link for the workbench changes. Do not publish to production.']},
 {role:'assistant',time:'11:51',text:['这项工作由 Atlas 负责。预览与正式发布分开处理；现在已经完成构建准备。','Atlas is handling this work. Preview and production release stay separate. The build is prepared.']},
 {role:'assistant',time:'12:17',text:['更新共享预览会影响团队正在访问的预览内容。Atlas 已准备好操作，等你允许本次部署。','Updating the shared preview changes what the team is viewing. Atlas is ready and waiting for your permission.'],citation:'Atlas',decision:'preview'}],
 navigation:[
 {role:'user',time:'10:45',text:['点一个任务，能快速去到对应卡片；回来时不要丢掉刚才的讨论和输入。','A task should take me straight to its canvas card. Coming back should keep the discussion and draft.']},
 {role:'assistant',time:'10:46',text:['Luna 正在处理定位与返回。会保留原画布操作，只在需要时带你到对应现场。','Luna is working on location and return. Canvas interactions stay intact; open the relevant card when needed.']},
 {role:'assistant',time:'12:14',text:['这轮交互已经可以体验。Luna 提供了三项验收：目标卡片居中、返回原任务、保留输入草稿。\n你可以先去现场走一遍，再决定是否通过本轮验收。','This revision is ready to try. Luna prepared three checks: center the target card, return to the original work, and keep the draft.\nTry the canvas path before accepting this revision.'],citation:'Luna',decision:'navigation'}],
 regression:[
 {role:'user',time:'11:10',text:['请补齐这轮工作台的双语和交互状态验证，结果放在这项工作里。','Complete locale and interaction-state checks for this iteration. Keep the results with this work.']},
 {role:'assistant',time:'11:11',text:['Nova 在验证中。当前没有需要你回答的问题，普通进度会保留在这条线程里。','Nova is validating the changes. No question needs you right now; routine progress stays in this thread.']},
 {role:'assistant',time:'12:15',text:['最近有效进展：验收场景已经整理，正在检查中英文切换与焦点反馈。下一步是汇总验证材料。','Latest meaningful progress: acceptance scenarios are prepared. Locale switching and focus feedback are being checked. Next is collecting the evidence.'],citation:'Nova'}],
 notes:[
 {role:'user',time:'11:23',text:['验证结果出来后，帮我整理一份面向用户的发布说明。','After validation finishes, prepare user-facing release notes.']},
 {role:'assistant',time:'11:24',text:['已为 Echo 保留这项工作。目前等待回归验证结果，不需要你在 Agent 之间转述。','This work is reserved for Echo. It is waiting for regression results; you do not need to relay them between agents.']}],
 visual:[
 {role:'user',time:'10:00',text:['官网首屏要更明确地说明 Norma 怎样替用户协调多个 Agent。','Make the website headline clearer about how Norma coordinates agents for the user.']},
 {role:'assistant',time:'10:02',text:['Iris 负责这项官网工作。相关文案讨论会留在官网频道，不混入桌面工作台的任务。','Iris owns this website work. Keep its discussion in the website channel, separate from desktop workbench tasks.']},
 {role:'assistant',time:'12:06',text:['首屏文案已经交付。需要继续调整时，可以在这条任务里补充反馈，工作记录仍然保留。','The headline copy is delivered. Leave further feedback in this thread; the work record stays available.'],citation:'Iris'}],
 'channel:desktop':[{role:'assistant',time:'12:18',text:['这个频道集中讨论桌面工作台的目标与跨任务安排。具体执行已经拆成独立任务：登录修复、定位返回、预览部署、回归验证和发布说明。\n你在频道里谈方向，在任务里补充细节。','This channel holds desktop goals and cross-work coordination. Implementation has separate threads for login, navigation, preview, regression and release notes.\nDiscuss direction here and details in the relevant work thread.']}],
 'channel:website':[{role:'assistant',time:'12:18',text:['官网与品牌工作放在这里。首屏文案已经交付；新的官网改动可以创建独立任务，和桌面迭代并行。','Website and brand work stays here. Headline copy is delivered. Create separate threads for new website work alongside the desktop iteration.']}],
 norma:[{role:'user',time:'09:40',text:['今天我会同时推进桌面工作台和官网。帮我记住各项工作的目标，需要我决定时再提醒我。','Today I’m working on both the desktop workbench and the website. Keep track of each goal and bring me decisions when needed.']},{role:'assistant',time:'09:40',text:['我会在这里保留跨项目的目标和优先级。每项具体工作都有自己的讨论、草稿和进度。\n你可以从工作总览看所有阶段，从“需要你处理”集中做决定。','I’ll keep cross-project goals and priorities here. Each specific work item has its own discussion, draft and progress.\nUse Overview for all stages, and Needs you for decisions.']}]
};

const cardPositions=[[160,180],[610,205],[1060,165],[195,505],[650,540],[1095,510]];
agents.forEach((a,i)=>{a.x=cardPositions[i][0];a.y=cardPositions[i][1]});
const meta={login:{completed:1,short:['登录修复','Login fix']},navigation:{completed:3,short:['卡片定位','Card location']},preview:{completed:1,short:['团队预览','Team preview']},regression:{completed:1,short:['回归验证','Regression']},notes:{completed:0,short:['发布说明','Release notes']},visual:{completed:3,short:['视觉规范','Visual spec']}};
seedMessages.visual=[{role:'assistant',time:'12:06',citation:'Iris',text:['视觉规范已交付。这里保留交付物和后续反馈。','Visual conventions delivered. This work keeps the artifact and follow-up feedback.']}];
