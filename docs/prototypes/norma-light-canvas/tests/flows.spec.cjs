const {test,expect}=require('playwright/test');
const {pathToFileURL}=require('node:url');const path=require('node:path');
const url=pathToFileURL(path.resolve(__dirname,'../index.html')).href;
test.use({channel:'chrome',viewport:{width:1440,height:1000}});
async function openWork(page,id){await page.getByTestId('status-capsule').click();await page.locator(`[data-queue-work="${id}"] .queue-work-title`).click();}

// 场景：三种轻量入口都留在同一画布，任务各自保存草稿和对话。
// 数据：6 张现场卡片、3 件人工事项，分别给登录和预览写可区分的草稿。
// 预期：视频真实解码，草稿与消息不串任务，三套没有重型导航页。
for(const variant of [1,2,3])test(`variant ${variant}: canvas and independent work context`,async({page})=>{
 const errors=[];page.on('pageerror',e=>errors.push(e.message));
 await page.goto(`${url}?v=${variant}`);
 await expect(page.locator('[data-node]')).toHaveCount(6);
 await expect(page.locator('.threads-sidebar,.workspace-grid')).toHaveCount(0);
 await expect.poll(()=>page.locator('video').evaluate(v=>v.videoWidth)).toBe(1920);
 await openWork(page,'login');
 await page.getByTestId('composer').fill('登录工作的独立草稿');
 await openWork(page,'preview');
 await expect(page.getByTestId('composer')).toHaveValue('');
 await page.getByTestId('composer').fill('预览工作的独立草稿');
 await openWork(page,'login');
 await expect(page.getByTestId('composer')).toHaveValue('登录工作的独立草稿');
 await page.getByTestId('send').click();
 await expect(page.getByTestId('conversation')).toContainText('登录工作的独立草稿');
 await openWork(page,'preview');
 await expect(page.getByTestId('composer')).toHaveValue('预览工作的独立草稿');
 await expect(page.getByTestId('conversation')).not.toContainText('登录工作的独立草稿');
 expect(errors,'三套原型均不应有脚本错误').toEqual([]);
});

// 场景：处理决定后跨原型读同一状态，验收不能直接冒充合并与发布。
// 数据：采用登录方案，再验收定位工作。
// 预期：待办依次从 3 降到 2、1，工作分别继续执行和待合并。
test('decisions and progress remain consistent across variants',async({page})=>{
 await page.goto(url);await openWork(page,'login');
 await page.getByTestId('apply-decision').click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');
 await expect(page.getByTestId('work-stage')).toContainText('继续执行');
 await page.locator('.proto-picker-item').nth(1).click();
 await expect(page.getByTestId('pending-count')).toHaveText('2');
 await openWork(page,'navigation');await page.getByTestId('apply-decision').click();
 await expect(page.getByTestId('work-stage')).toContainText('待合并');
 await expect(page.getByTestId('pending-count')).toHaveText('1');
});

// 场景：浮窗或聚焦模式能收起回到画布，定位准确，接管阻止自动采用。
// 数据：将 Simon 卡片居中，接管再打开对应工作。
// 预期：目标卡片中心误差 < 4px，接管时推荐按钮禁用，交还后恢复。
test('locate, focus exit and takeover preserve the canvas',async({page})=>{
 await page.goto(`${url}?v=3`);await openWork(page,'login');
 await page.getByTestId('exit-focus').click();
 await expect(page.locator('.focus-pane')).toHaveCount(0);
 await page.locator('[data-locate-work="login"]').first().click();
 const centered=await page.locator('[data-node="simon"]').evaluate(el=>{const a=el.getBoundingClientRect(),v=el.closest('.canvas-viewport').getBoundingClientRect();return Math.abs(a.x+a.width/2-v.x-v.width/2)<4&&Math.abs(a.y+a.height/2-v.y-v.height/2)<4;});
 expect(centered,'定位应将目标卡片完整居中').toBe(true);
 await page.locator('[data-control="simon"]').click();
 await openWork(page,'login');await expect(page.getByTestId('apply-decision')).toBeDisabled();
 await page.getByTestId('exit-focus').click();await page.locator('[data-control="simon"]').click();
 await openWork(page,'login');await expect(page.getByTestId('apply-decision')).toBeEnabled();
});

// 场景：修改方案有回执，输入时数字不切原型，减少动态效果和背景控制生效。
// 数据：修改一次原问题，切到英文并检查三个入口。
// 预期：修改文本可查；视频可暂停；输入框内数字不会触发比较器。
test('edit, locale, keyboard and video controls',async({page})=>{
 await page.goto(`${url}?v=2`);await openWork(page,'login');
 await page.getByTestId('edit-decision').click();
 await page.getByTestId('decision-edit').fill('保留本地数据，让用户重新登录一次');
 await page.getByTestId('submit-edit').click();
 await expect(page.getByTestId('conversation')).toContainText('保留本地数据，让用户重新登录一次');
 await expect(page.getByTestId('pending-count')).toHaveText('2');
 await page.getByTestId('composer').fill('1');await page.keyboard.press('3');
 await expect(page.locator('.proto-picker-item[data-active]')).toContainText('任务条');
 await page.getByTestId('video-toggle').click();
 expect(await page.locator('video').evaluate(v=>v.paused),'背景按钮应真实暂停视频').toBe(true);
 await page.getByTestId('language').click();await expect(page.getByTestId('status-capsule')).toContainText('working');
 for(let i=0;i<3;i++){await page.locator('.proto-picker-item').nth(i).click();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),'英文原型不应横向溢出').toBe(true);}
});

// 场景：画布直接操作不改任务归属，视图切换保留卡片位置与返回位置。
// 数据：Simon 向右拖 100px、向下拖 45px，随后平移和放大一次。
// 预期：世界坐标按当前缩放换算；聚焦退出回原相机；草稿和阅读位置保留。
test('canvas manipulation, return camera and reading position persist',async({page})=>{
 await page.goto(url);await openWork(page,'login');
 await page.getByTestId('composer').fill('保持这份草稿');
 const scroll=page.locator('[data-conversation-scroll="login"]');await scroll.evaluate(el=>el.scrollTop=0);
 await page.locator('[data-action="close-conversation"]').click();
 const header=page.locator('[data-drag-node="simon"]'),before=await page.locator('[data-node="simon"]').boundingBox();
 const grab=await header.boundingBox();await page.mouse.move(grab.x+70,grab.y+20);await page.mouse.down();await page.mouse.move(grab.x+170,grab.y+65,{steps:6});await page.mouse.up();
 const after=await page.locator('[data-node="simon"]').boundingBox();expect(Math.abs(after.x-before.x-100),'卡片应跟随指针水平移动 100px').toBeLessThan(2);expect(Math.abs(after.y-before.y-45),'卡片应跟随指针竖直移动 45px').toBeLessThan(2);
 await page.mouse.move(35,145);await page.mouse.down();await page.mouse.move(80,180,{steps:4});await page.mouse.up();
 await page.locator('[data-zoom="1"]').click();const camera=await page.locator('.canvas-world').getAttribute('style');
 await openWork(page,'login');await expect(page.getByTestId('composer')).toHaveValue('保持这份草稿');
 expect(await page.locator('[data-conversation-scroll]').evaluate(el=>el.scrollTop),'重新打开应保留阅读位置').toBe(0);
 await page.locator('.proto-picker-item').nth(2).click();await page.getByTestId('exit-focus').click();
 const overview=await page.locator('.canvas-world').getAttribute('style');await openWork(page,'login');await page.getByTestId('exit-focus').click();
 await expect(page.locator('.canvas-world'),'退出聚焦应精确返回原相机').toHaveAttribute('style',overview);
 await page.locator('.proto-picker-item').nth(0).click();await expect(page.locator('.canvas-world'),'方案切换应保留当前画布位置').toHaveAttribute('style',camera);
});

// 场景：三种方案在不同窗口中，主入口与比较器均可操作，浮窗不越界。
// 数据：1440、1280、1024、768、390px 五种宽度，中英文各一遍。
// 预期：入口不被比较器遮挡；讨论与待办在视口内；页面无水平滚动。
test('responsive layouts keep controls and panels in view',async({page})=>{
 for(const width of [1440,1280,1024,768,390]){
  await page.setViewportSize({width,height:1000});
  for(const variant of [1,2,3]){
   await page.goto(`${url}?v=${variant}`);await expect(page.locator('[data-node]')).toHaveCount(6);
   for(const locale of ['zh','en']){
    if(locale==='en')await page.getByTestId('language').click();
    const overlaps=await page.getByTestId('status-capsule').evaluate(el=>{const a=el.getBoundingClientRect(),b=document.querySelector('.proto-picker').getBoundingClientRect();return a.left<b.right&&a.right>b.left&&a.top<b.bottom&&a.bottom>b.top});
    expect(overlaps,`${width}px 方案${variant} ${locale}：状态入口不应被比较器遮挡`).toBe(false);
    const pickerWraps=await page.locator('.proto-picker-item').evaluateAll(items=>items.some(item=>{const range=document.createRange();range.selectNodeContents(item);return range.getBoundingClientRect().height>20}));
    expect(pickerWraps,`${width}px 方案${variant} ${locale}：比较器名称不应折行`).toBe(false);
    await openWork(page,'login');
    const rect=await page.getByTestId('conversation').boundingBox();expect(rect.x,`${width}px 方案${variant}：讨论左边界`).toBeGreaterThanOrEqual(0);expect(rect.x+rect.width,`${width}px 方案${variant}：讨论右边界`).toBeLessThanOrEqual(width+1);expect(rect.y+rect.height,`${width}px 方案${variant}：讨论下边界`).toBeLessThanOrEqual(1001);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`${width}px 方案${variant}：页面不应横向溢出`).toBe(true);
   }
  }
 }
});

// 场景：减少动态效果下默认暂停；弹窗键盘焦点不能漏到画布。
// 数据：reduce 偏好，编辑对话内连续 Tab 与 Escape。
// 预期：视频默认暂停；Tab 仍在编辑框内；Escape 回到未改变的任务。
test('reduced motion and dialog keyboard behavior',async({page})=>{
 await page.emulateMedia({reducedMotion:'reduce'});await page.goto(url);await openWork(page,'login');
 expect(await page.locator('video').evaluate(v=>v.paused),'减少动态效果时默认暂停背景').toBe(true);
 await page.getByTestId('edit-decision').click();await page.getByTestId('submit-edit').focus();await page.keyboard.press('Tab');
 expect(await page.locator('#overlay').evaluate(el=>el.contains(document.activeElement)),'Tab 必须留在模态对话框中').toBe(true);
 await page.keyboard.press('Escape');await expect(page.locator('#overlay')).toBeEmpty();await expect(page.getByTestId('pending-count')).toHaveText('3');
});
