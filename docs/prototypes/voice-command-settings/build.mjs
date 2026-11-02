import {readFile, writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const root=path.dirname(fileURLToPath(import.meta.url));
const read=name=>readFile(path.join(root,'src',name),'utf8');
const css=await read('styles.css')+'\n'+await read('picker.css');
const names=['model.mjs','locales.mjs','views.mjs','app.mjs','picker.js'];
const js=(await Promise.all(names.map(read))).map(source=>source.replace(/^export /gm,'')).join('\n');
if(/<\/script/i.test(js))throw new Error('Inline script must not contain a closing script tag');
const picker=await read('picker.html');
const html=`<!doctype html>
<html lang="zh-CN" data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="dark light"><title>语音指令 · 交互原型</title><style>${css}</style></head>
<body><header class="harness" id="harness"></header><div id="stage"></div><p class="prototype-note" id="prototype-note"></p>${picker}
<dialog id="editor" aria-labelledby="dialog-title"></dialog><div class="popover" id="popover" hidden></div><div class="snackbar" id="snackbar" role="status" aria-live="polite"></div>
<script type="module">${js}</script></body></html>`;
await writeFile(path.join(root,'index.html'),html);
console.log('Built standalone prototype: docs/prototypes/voice-command-settings/index.html');
