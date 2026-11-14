const {chromium}=require('playwright');
const path=require('node:path');
const {pathToFileURL}=require('node:url');
const fs=require('node:fs');
const root=path.resolve(__dirname,'..');
const output=path.join(root,'assets');
const url=pathToFileURL(path.join(root,'index.html')).href;
async function freezeWallpaper(page){
 await page.locator('video').evaluate(async video=>{
  if(video.readyState<2)await new Promise(resolve=>video.addEventListener('loadeddata',resolve,{once:true}));
  video.pause();video.currentTime=3;
  await new Promise(resolve=>video.addEventListener('seeked',resolve,{once:true}));
 });
}
(async()=>{
 fs.mkdirSync(output,{recursive:true});const browser=await chromium.launch({channel:'chrome'});
 try{
  for(const variant of [1,2,3]){
   const page=await browser.newPage({viewport:{width:1440,height:1000}});
   await page.goto(`${url}?v=${variant}`);await page.locator('[data-node]').first().waitFor();await freezeWallpaper(page);
   if(variant===1)await page.getByTestId('status-capsule').click();
   if(variant===2)await page.locator('[data-task-tab="login"]').click();
   await page.screenshot({path:path.join(output,`variant-${variant}.png`)});
   if(variant===1){
    await page.locator('[data-queue-work="login"] .queue-work-title').click();
    await page.screenshot({path:path.join(output,'capsule-discussion.png')});
    await page.locator('[data-action="close-conversation"]').click();
    await page.screenshot({path:path.join(output,'capsule-idle.png')});
   }
   await page.close();
  }
  const page=await browser.newPage({viewport:{width:390,height:844}});
  await page.goto(`${url}?v=2`);await page.locator('[data-node]').first().waitFor();await freezeWallpaper(page);
  await page.locator('[data-task-tab="login"]').click();await page.screenshot({path:path.join(output,'compact-window.png')});
  await page.close();
 }finally{await browser.close()}
})().catch(error=>{process.exitCode=1;console.error(error)});
