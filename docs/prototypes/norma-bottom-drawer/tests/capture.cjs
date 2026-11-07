const {chromium}=require('playwright');
const path=require('node:path');const {pathToFileURL}=require('node:url');
(async()=>{
 const browser=await chromium.launch({channel:'chrome',headless:true});
 const page=await browser.newPage({viewport:{width:1600,height:1050},deviceScaleFactor:1});
 await page.goto(pathToFileURL(path.resolve(__dirname,'../index.html')).href);
 await page.locator('video').evaluate(async v=>{v.pause();if(v.readyState<2)await new Promise(r=>v.addEventListener('loadeddata',r,{once:true}));await new Promise(r=>{v.addEventListener('seeked',r,{once:true});v.currentTime=3;});});
 const shot=async name=>{await page.screenshot({path:path.resolve(__dirname,'../assets/'+name+'.png')});};
 await shot('canvas');await page.getByTestId('dock-open').click();await page.waitForTimeout(350);await page.locator('textarea').first().blur();await shot('drawer');
 await page.locator('[data-action="project"]').click();await page.keyboard.press('ArrowDown');await shot('material-menu');await page.keyboard.press('Escape');
 await page.getByTestId('simulate-event').click();await shot('material-feedback');await page.locator('[data-snackbar-close]').click();
 await page.getByTestId('locate').click();await shot('card-session');await page.getByTestId('return').click();await page.waitForTimeout(300);
 await page.locator('[data-action="language"]').click();await shot('drawer-en');
 await page.locator('#drawer-dock [data-action="theme"]').click();await shot('drawer-light');
 await page.locator('#drawer-dock [data-action="theme"]').click();await page.locator('[data-action="language"]').click();
 await page.setViewportSize({width:390,height:900});await shot('drawer-compact');
 await browser.close();
})();
