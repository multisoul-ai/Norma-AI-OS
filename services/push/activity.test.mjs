import test from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import * as http2 from 'node:http2';
import { createAPNs, createRelay } from './relay.mjs';
const now=1800000000;
const state={name:'工作空间',updated_at:now,active:true,running_count:2,approval_count:1,review_count:0,failure_count:0,hidden_projects:0,projects:[{area_id:'area-a',name:'项目',status:'approval',reason:'确认 Bash',agents:[{name:'Alice',action:'等待审批'}],other_agents:0}]};
const activity=(event)=>({event_id:`00000000-0000-4000-8000-${event==='start'?'1':event==='update'?'2':'3'}`.padEnd(36,'0'),token:'a'.repeat(64),expires_at:now+120,kind:'activity',event,timestamp:now,stale_date:now+120,priority:event==='update'?5:10,content_state:state,...(event==='start'?{attributes:{machine_id:`mac-${'b'.repeat(64)}`,desktop_workspace_id:'space',generation:'generation',cycle_id:'cycle',started_at:now}}:{}),...(event==='end'?{dismissal_date:now+600}:{})});
/// 实时活动复用 APNs 连接：start 提示一次，update 静默，end 保留十分钟。
/// 数据构造：120s=2分钟过期，600s=10分钟结果保留；start/update/end 各一份。
/// 执行过程：真实 HTTP入口校验→本地 HTTP/2 接收 APNs headers/payload→重放→非法属性拒绝。
/// 预期结果：专用 topic/type，start 有 alert，update 无 sound/alert，重复 start 不多发，4KiB以上拒绝。
test('ActivityKit uses dedicated topic, silent updates and bounded validated content',async t=>{
 const received=[];const h2=http2.createServer();h2.on('stream',(s,headers)=>{let body='';s.on('data',c=>body+=c);s.on('end',()=>{received.push({headers,body:JSON.parse(body)});s.respond({':status':200});s.end();});});
 await new Promise(r=>h2.listen(0,'127.0.0.1',r));
 const {privateKey}=generateKeyPairSync('ec',{namedCurve:'prime256v1'});
 const apns=createAPNs({privateKey,teamID:'TEAM123456',keyID:'KEY1234567',environment:'sandbox',topic:'com.norma-ai.os.ios'},{now:()=>now*1000,connect:()=>http2.connect(`http://127.0.0.1:${h2.address().port}`)});
 const relay=createRelay({key:'c'.repeat(64),now:()=>now*1000,deliver:apns.send});await new Promise(r=>relay.listen(0,'127.0.0.1',r));t.after(()=>{relay.closeAllConnections();relay.close();apns.close();h2.close();});
 const post=async event=>(await fetch(`http://127.0.0.1:${relay.address().port}/v1/notifications`,{method:'POST',headers:{authorization:`Bearer ${'c'.repeat(64)}`,'content-type':'application/json'},body:JSON.stringify(event)})).status;
 assert.equal(await post(activity('start')),200,'合法 start 应接受');assert.equal(await post(activity('start')),200,'重复返回原结果');
 assert.equal(received.length,1,'重复 start 不得产生第二个活动');
 assert.equal(await post(activity('update')),200,'更新应接受');assert.equal(await post(activity('end')),200,'结束应接受');
 assert.equal(received[0].headers['apns-topic'],'com.norma-ai.os.ios.push-type.liveactivity','必须使用活动 topic');
 assert.equal(received[1].headers['apns-push-type'],'liveactivity','静默仍使用 liveactivity');
 assert.equal(received[0].body.aps['attributes-type'],'WorkspaceActivityAttributes','Swift 属性类型必须准确');
 assert.ok(received[0].body.aps.alert,'远程 start 必须有一次提示');assert.equal(received[1].body.aps.alert,undefined,'更新不得发普通通知');assert.equal(received[1].body.aps.sound,undefined,'更新不得发声音');
 assert.equal(received[2].body.aps['dismissal-date'],now+600,'结果保留十分钟');
 const review={event_id:'00000000-0000-4000-8000-400000000000',token:'a'.repeat(64),expires_at:now+300,kind:'review',machine_id:`mac-${'b'.repeat(64)}`,workspace_id:'desktop:space:area-a',desktop_workspace_id:'space',area_id:'area-a',route_id:'workspace',node_id:'1',run_id:'run',session_id:'session',turn_id:'turn',preview:{project:'项目',card:'Maya',summary:'2个文件待验收'}};
 assert.equal(await post(review),200,'真实可验收提醒复用普通通知管线');
 assert.equal(received[3].headers['apns-push-type'],'alert','验收事项仍是普通通知');
 assert.equal(received[3].body.norma.desktop_workspace_id,'space','通知保留工作空间身份');
 assert.equal(received[3].body.norma.area_id,'area-a','点击保留项目区域身份');
 assert.equal(await post({...review,event_id:'00000000-0000-4000-8000-500000000000',workspace_id:'other'}),400,'通知中的复合scope必须匹配工作空间与区域');
 assert.equal(await post({...activity('update'),attributes:activity('start').attributes}),400,'update 不得更换活动身份');
 assert.equal(await post({...activity('start'),content_state:{...state,name:'x'.repeat(4097)}}),413,'超限负载应拒绝');
 assert.equal(await post({...activity('update'),stale_date:now+121}),400,'不能延长系统过期预算');
});
