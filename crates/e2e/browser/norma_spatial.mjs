/**
 * 场景：开发者在自由画布指挥 Codex/Claude，然后查看改动并切换原型布局。
 * 数据：独立 mock CLI，输入路由任务，内存状态。执行：真实控件与键盘交互。
 * 预期：点名中断不干扰其他会话；上下文保留；便签可编辑；变体切换不丢状态。
 * page 同时兼容 CUA tab.playwright 与 Playwright Page。
 */
export async function runNormaSpatialE2E(page) {
 const results=[];const ensure=(condition,message)=>{if(!condition)throw new Error(message);results.push(message)};
 const button=name=>page.getByRole('button',{name,exact:true});
 const input=page.getByLabel('给 Norma 的指令',{exact:true});
 const send=async text=>{await input.fill(text);await button('发送指令').click()};
 ensure(await page.getByLabel('给 Codex 的 CLI 指令',{exact:true}).isVisible(),'默认画布显示 Codex CLI');
 ensure(await page.getByLabel('给 Claude 的 CLI 指令',{exact:true}).isVisible(),'默认画布显示 Claude CLI');
 await page.getByLabel('给 Codex 的 CLI 指令',{exact:true}).fill('检查终端焦点路由');await button('发送到 Codex').click();
 ensure(await button('Codex · 实现 · CLI · 执行中').isVisible(),'直接输入启动 Codex');
 ensure(await button('Claude · 审查 · CLI · 就绪').isVisible(),'独立 CLI 不误启动 Claude');
 await send('暂停 Codex');
 ensure(await button('Codex · 实现 · CLI · 已暂停').isVisible(),'自然语言可中断指定 CLI');
 ensure((await page.getByRole('log',{name:'Codex 终端输出',exact:true}).innerText()).includes('检查终端焦点路由'),'中断后保留会话内容');
 await button('适应全部内容').click();
 await page.getByLabel('我的想法',{exact:true}).fill('圈出焦点切换的问题，再交给 Claude 审查。');
 await button('让 Claude 审查改动').click();await send('全部暂停');
 ensure(await button('Claude · 审查 · CLI · 已暂停').isVisible(),'Claude 能接手审查并暂停');
 await page.getByRole('tab',{name:'改动',exact:true}).click();
 ensure((await page.getByRole('tabpanel',{name:'改动',exact:true}).innerText()).includes('crates/runtime/src/route.rs'),'改动面板展示当前仓库路径');
 await button('返回画布').click();await button('添加内容').click();await button('图片').click();
 ensure(await page.getByRole('img',{name:'宁静湖畔环境参考',exact:true}).isVisible(),'自由画布支持添加图片');
 await button('关闭 空间参考图').click();
 await button('下一个原型').click();
 ensure(await page.getByRole('heading',{name:'两位伙伴，同一个开发现场。',exact:true}).isVisible(),'B 使用宽画布协作结构');
 ensure((await page.getByLabel('我的想法',{exact:true}).getAttribute('aria-label'))==='我的想法','变体切换保留可编辑便签');
 await input.fill('保留这条未发送的输入');await input.press('ArrowRight');
 ensure(await page.getByRole('heading',{name:'两位伙伴，同一个开发现场。',exact:true}).isVisible(),'文本输入的方向键不切换原型');
 await input.fill('');await button('下一个原型').click();
 ensure(await page.getByRole('heading',{name:'从目标，到可审阅的改动。',exact:true}).isVisible(),'C 使用任务主导结构');
 await button('下一个原型').click();
 ensure(await button('Codex · 实现 · CLI · 已暂停').isVisible(),'绕回 A 后保留执行状态');
 return results;
}

/**
 * 场景：在 Codex 内容上画出修改意见并交给 Norma。数据：一条跨越节点的笔迹。
 * 执行：真实 pointer drag、撤销、重做、橡皮与提交；随后移动节点、用键盘调整大小。
 * 预期：笔迹数量准确、上下文有节点标题、直接操作改变几何；不通过脚本注入 UI 事件。
 */
export async function runNormaCanvasPointerE2E(page,pointer) {
 const results=[];const ensure=(c,m)=>{if(!c)throw new Error(m);results.push(m)};
 const button=name=>page.getByRole('button',{name,exact:true});
 const count=async()=>Number((await page.locator('.canvas-topline').innerText()).match(/(\d+)\s*处标注/)?.[1]);
 const before=await count();
 const bounds=await page.evaluate(()=>{const n=document.querySelector('.node-codex').getBoundingClientRect(),s=document.querySelector('.canvas-stage').getBoundingClientRect();return {x:n.x+45,y:Math.min(n.y+120,s.bottom-60)}});
 const from=[bounds.x,bounds.y],to=[bounds.x+170,bounds.y+20];
 await page.getByLabel('画笔',{exact:true}).click();await pointer.drag(from,to);
 ensure(await count()===before+1,'真实指针绘制新增一条笔迹');
 await button('撤销笔迹').click();ensure(await count()===before,'撤销移除刚才笔迹');
 await button('重做笔迹').click();ensure(await count()===before+1,'重做恢复同一笔迹');
 await page.getByLabel('橡皮',{exact:true}).click();await pointer.drag([bounds.x+85,bounds.y-20],[bounds.x+85,bounds.y+45]);
 ensure(await count()===before,'橡皮可命中线段中间');
 await page.getByLabel('画笔',{exact:true}).click();await pointer.drag(from,to);await button('标注交给 Norma').click();
 ensure((await page.getByRole('log',{name:'Codex 终端输出',exact:true}).innerText()).includes('画笔标注调整，关联内容：Codex'),'标注指令包含关联 CLI');
 await page.getByLabel('选择',{exact:true}).click();
 const heading=await page.evaluate(()=>{const n=document.querySelector('.node-codex .node-heading').getBoundingClientRect();return {x:n.x+110,y:n.y+12}});
 const old=await page.locator('.node-codex').getAttribute('style');
 await pointer.drag([heading.x,heading.y],[heading.x+22,heading.y-10]);
 ensure(await page.locator('.node-codex').getAttribute('style')!==old,'拖动标题改变节点位置');
 const width=await page.evaluate(()=>document.querySelector('.node-codex').style.width);
 await button('调整 Codex · 实现 大小').press('ArrowRight');
 ensure(await page.evaluate(()=>document.querySelector('.node-codex').style.width)!==width,'键盘调整节点大小');
 return results;
}
