import {readFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import vm from 'node:vm';
const root=path.dirname(fileURLToPath(import.meta.url));
const files=['model.mjs','locales.mjs','views.mjs'];
let source=(await Promise.all(files.map(name=>readFile(path.join(root,'src',name),'utf8')))).map(text=>text.replace(/^export /gm,'')).join('\n');
source=source.replace("COPY[key]?.[ui.lang === 'en' ? 1 : 0] ?? key", "COPY[key]?.[ui.lang === 'en' ? 1 : 0] ?? (()=>{throw new Error('Missing translation: '+key)})()");
source+=`\nlet settings={...DEFAULT_SETTINGS};const ui={lang:'zh',step:'input',demo:{input:'Bruno，继续刚才的登录任务',proposal:interpretDemo('Bruno，继续刚才的登录任务'),receipt:false}};globalThis.checks={renders:0,dialogs:0};
for(const lang of ['zh','en']){ui.lang=lang;for(const variant of [renderGrouped,renderWorkflow,renderOverview]){for(const choice of [{},{interaction:'continuous'},{source:'cloud'},{status:'failed'},{spokenReply:false}]){settings={...DEFAULT_SETTINGS,...choice};for(const step of (variant===renderWorkflow?GROUPS:['input'])){ui.step=step;const html=variant();if(!html.includes('Bruno')||!html.includes('Henry')||!html.includes('data-demo-input'))throw new Error('Missing interactive context');globalThis.checks.renders++;}}}}
for(const panel of [...GROUPS,'targets','agent-bruno','agent-henry','advanced','model','cloud','mic-test','shortcut','help']){const result=dialogContents(panel);if(!result.title||!result.body)throw new Error('Missing dialog '+panel);globalThis.checks.dialogs++;}
`;
const context={};vm.runInNewContext(source,context,{timeout:5000});
console.log(JSON.stringify({...context.checks,variants:3,languages:2,missing_translations:0,browser_acceptance:false}));
