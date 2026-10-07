import assert from 'node:assert/strict';
import {randomBytes} from 'node:crypto';
import {createServer} from 'node:http';
import {once} from 'node:events';
import WebSocket, {WebSocketServer} from 'ws';
import {makeGateway} from '../server.mjs';
import {createCloudSessions} from '../cloud/sessions.mjs';

/// 场景：真实 Redis 在不同网关实例间消费一次性票据、互斥付费会话并恢复已播放上下文。
///
/// 数据构造（含关键数值的推导过程）：
///   随机 32 bytes = 256 bits，隔离生产 Redis 命名空间；两个独立 store/网关代表两个云实例。
///   票据有效 30s，过期检查等 31s = 30s + 1s 调度余量；全检查最多 55s。
///   一条已播“你好，”与未播“听我说。”区分恢复历史；只保存前者。
///   实例时钟推进 241000ms = 240000ms 云端轮换阈值 + 1000ms 调度余量。
/// 执行过程：
///   1. A 签发、B 消费；拒绝 A 重放、并发会话、错误租约续期及释放。
///   2. 正确释放已播历史 → 新实例恢复；其他设备和不安全关闭后不可恢复旧历史。
///   3. 等待真实 Redis TTL 到期，确认过期票据不可用。
///   4. HTTP 在实例 A，WebSocket 在实例 B；只连本机假 ASR，无付费云调用。
///   5. 正常关闭并交叉实例恢复；推进时钟触发空闲轮换，检查 1012 与相同会话 ID。
/// 预期结果：
///   - 票据单次消费、全局互斥和租约归属都由真实 Redis 原子操作保证。
///   - 已播内容可恢复，未播后缀和不安全关闭的旧历史均不可恢复。
///   - 请求落到不同实例仍可用；平台时限前关闭付费上游，再由客户端重新连接。
async function checkCloud() {
  let time = Date.now();
  const env = {...process.env,COMPANION_ACCESS_TOKEN:randomBytes(32).toString('hex'),
    DOUBAO_PROTOCOL:'plan',DOUBAO_SPEECH_API_KEY:'fake-upstream-key',PUBLIC_ORIGIN:'http://127.0.0.1:8787'};
  const options = {env,now:()=>time};
  const a = createCloudSessions(options), b = createCloudSessions(options);
  const token = () => randomBytes(32).toString('base64url');
  const first = token();
  assert.equal(await a.mint(first,false),true,'实例 A 必须成功签发票据');
  const lease = await b.claim(first);
  assert.ok(lease?.id,'实例 B 必须消费实例 A 签发的票据');
  assert.equal(await a.claim(first),null,'已消费票据不得跨实例重放');
  assert.equal(await a.mint(token(),false),false,'已有付费会话时不能创建并发会话');
  assert.equal(await a.renew('wrong-owner'),false,'错误租约不能续期');
  assert.equal(await a.renew(lease.id),true,'持有者应可跨实例续期');
  assert.equal(await a.release('wrong-owner',null),false,'错误租约不能释放活动会话');
  const history = {session:{id:'history-1',expires:time+1800000},conversation:{id:'history-1',
    messages:[{role:'user',content:'陪我聊聊'},{role:'assistant',content:'你好，'}]}};
  assert.equal(await b.release(lease.id,history),true,'持有者应保存安全历史后释放会话');
  const second = token(); await a.mint(second,true);
  const resumed = await b.claim(second);
  assert.equal(resumed?.session.id,'history-1','不同实例应恢复已确认的会话');
  assert.equal(resumed?.conversation.messages[1].content,'你好，','恢复历史必须保留已播前缀');
  assert.equal(JSON.stringify(resumed).includes('听我说。'),false,'恢复历史不得包含未播后缀');
  await b.release(resumed.id,history);
  const otherDevice=token(); await a.mint(otherDevice,true,'another-paired-phone');
  const isolated=await b.claim(otherDevice);
  assert.equal(isolated?.session.id,'','另一设备不能恢复前一设备的对话身份');
  assert.equal(isolated?.conversation.messages.length,0,'另一设备不能读取前一设备的语音历史');
  await b.release(isolated.id,null);
  const third = token(); await a.mint(third,true);
  const cleared = await b.claim(third);
  assert.equal(cleared?.session.id,'','不安全关闭不能恢复旧会话 ID');
  assert.equal(cleared?.conversation.messages.length,0,'不安全关闭不能恢复旧历史');
  await b.release(cleared.id,null);
  const expired = token(); await a.mint(expired,false);
  await new Promise(resolve=>setTimeout(resolve,31000));
  assert.equal(await b.claim(expired),null,'真实 Redis TTL 到期后票据必须失效');

  const fake = createServer(), asr = new WebSocketServer({server:fake});
  fake.listen(0,'127.0.0.1'); await once(fake,'listening');
  let upstreamCount=0, closedCount=0;
  asr.on('connection',peer=>{
    upstreamCount++; peer.once('close',()=>closedCount++);
    peer.once('message',()=>{
      const body=Buffer.from(JSON.stringify({result:{utterances:[]}})),header=Buffer.alloc(12);
      Buffer.from([0x11,0x91,0x10,0]).copy(header);header.writeInt32BE(1,4);header.writeUInt32BE(body.length,8);
      peer.send(Buffer.concat([header,body]));
    });
  });
  const make = store => makeGateway({...options,sessionStore:store,
    upstreamFactory:()=>new WebSocket(`ws://127.0.0.1:${fake.address().port}`),
    fetchImpl:()=>{throw new Error('paid_cloud_calls_forbidden_in_check');}});
  const serverA=make(a),serverB=make(b),clients=[];
  for (const server of [serverA,serverB]) {server.listen(0,'127.0.0.1');await once(server,'listening');}
  const open = async (issuer,receiver,resume) => {
    const r=await fetch(`http://127.0.0.1:${issuer.address().port}/v1/session`,{method:'POST',
      headers:{'Content-Type':'application/json',Authorization:`Bearer ${env.COMPANION_ACCESS_TOKEN}`},
      body:JSON.stringify({provider:'doubao',resume})});
    assert.equal(r.status,200,'独立 HTTP 实例必须可签发云端票据');
    const ticket=await r.json();
    assert.equal(JSON.stringify(ticket).includes(env.DOUBAO_SPEECH_API_KEY),false,'票据中不能泄漏服务商 Key');
    const ws=new WebSocket(`ws://127.0.0.1:${receiver.address().port}/v1/voice`,{headers:{Authorization:`Bearer ${ticket.token}`}});
    clients.push(ws);
    const ready=new Promise((resolve,reject)=>{
      ws.on('error',reject);ws.on('message',raw=>{const e=JSON.parse(raw);if(e.type==='session.created')resolve(e.session.id);if(e.type==='gateway.error')reject(new Error(e.error.code));});
    });
    return {ws,id:await ready};
  };
  const end=async client=>{const close=once(client,'close');client.send('{"type":"session.close"}');await close;};
  try {
    // 五次存储级 mint + 三次连接会超过每分钟六次，推进 60001ms 开始独立连接测试窗口。
    time+=60001;
    const first=await open(serverA,serverB,false);await end(first.ws);
    const second=await open(serverB,serverA,true);
    assert.equal(second.id,first.id,'正常关闭后交叉实例连接必须恢复同一会话');
    const rotated=once(second.ws,'close');time+=241000;
    const [code]=await rotated;
    assert.equal(code,1012,'空闲轮换应以 1012 触发客户端重连，不能发正常挂断事件');
    const third=await open(serverA,serverB,true);
    assert.equal(third.id,first.id,'平台轮换后仍应保留可安全恢复的会话');
    await end(third.ws);
    assert.equal(upstreamCount,3,'仅明确的三次通话应创建假 ASR 连接');
    assert.equal(closedCount,3,'交付关闭确认前所有旧上游必须真正释放');
  } finally {
    for(const client of clients)client.terminate();
    for(const server of [serverA,serverB,fake]){server.closeAllConnections();await new Promise(resolve=>server.close(resolve));}
    asr.close();
  }
}

const timeout=setTimeout(()=>{console.error('FAIL: cloud session check timed out');process.exit(1);},55000);
try {await checkCloud();console.log('PASS: Redis atomic tickets, replay rejection, exclusive leases, expiry, cross-instance recovery and quiet rotation');}
finally {clearTimeout(timeout);}
