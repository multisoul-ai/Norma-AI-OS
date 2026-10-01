import assert from 'node:assert/strict';
import {once} from 'node:events';
import {createServer} from 'node:http';
import WebSocket, {WebSocketServer} from 'ws';
import {makeGateway} from './server.mjs';
import {createCloudSessions} from './cloud-sessions.mjs';
import {randomBytes} from 'node:crypto';

const listen = async server => {server.listen(0,'127.0.0.1'); await once(server,'listening'); return server.address().port;};
function inbox(ws) {
  const queue = [], waiting = [];
  ws.on('message', raw => {const value = JSON.parse(raw); waiting.length ? waiting.shift()(value) : queue.push(value);});
  return async type => {
    for (;;) {
      // Local: 1s protocol + 1s PCM + 1s scheduling = 3s.
      // Cloud: claim/send/poll can each use their 3s HTTP budget + 3s local margin = 12s.
      const budget=process.env.NORMA_CLOUD_TOOLS_CHECK==='1' ? 12000 : 3000;
      let timeout;
      const event = queue.length ? queue.shift() : await Promise.race([
        new Promise(resolve => waiting.push(resolve)),
        new Promise((_, reject) => {timeout = setTimeout(()=>reject(Error('timeout: '+type)),budget);})
      ]).finally(()=>clearTimeout(timeout));
      assert.notEqual(event.type,'gateway.error','媒体通道不能因合法任务操作失败：'+JSON.stringify(event));
      if (event.type === type) return event;
    }
  };
}
const send = (ws, event) => ws.send(JSON.stringify(event));

/// 场景：同一豆包媒体会话签发独立Mac票据，工具结果经唯一续答方回流，汇报只读真实文本。
///
/// 数据构造：真实HTTP/WS回环，假ASR返回明确原话；LLM只返回一次create_agent；
///   Mac回执status=accepted/task_id=7；PCM=4800bytes=24000Hz×2bytes×0.1s。
/// 执行过程：连媒体 → 错身份票据拒绝 → 正确票据绑定且不能重用 → 注册工具 →
///   转写/工具精确关联 → 回传accepted并继续一次 → 播放完成 → 独立汇报 → 关闭。
/// 预期结果：票据30s过期且没有长期Key；手机不执行工具；模型读到实际回执；
///   汇报不调用LLM、不改变会话历史；票据过期或媒体关闭后不可用，关闭媒体释放Mac连接。
///   NORMA_CLOUD_TOOLS_CHECK=1时，HTTP/工具连接在A、媒体在B；使用真实Redis随机命名空间。
///   必须跨实例完成相同任务，不能靠负载均衡恰好命中同一内存实例。
///   公网挂载 /norma/voice，代理剥离前缀；媒体与工具票据仍必须保留完整公网路径。
async function checkTasks() {
  const asrServer = createServer(), asrPeers = new WebSocketServer({server:asrServer});
  const asrPort = await listen(asrServer);
  let asr, time = 1000000, chatCalls = 0;
  const chatBodies = [], speech = [], pcm = Buffer.alloc(4800);
  const asrEvent = result => {
    const body = Buffer.from(JSON.stringify({result})), prefix = Buffer.alloc(12);
    Buffer.from([0x11,0x91,0x10,0]).copy(prefix); prefix.writeInt32BE(1,4); prefix.writeUInt32BE(body.length,8);
    asr.send(Buffer.concat([prefix,body]));
  };
  asrPeers.on('connection', ws => {asr=ws; ws.once('message',()=>asrEvent({}));});
  const cloud = process.env.NORMA_CLOUD_TOOLS_CHECK === '1';
  const env = {...process.env,PUBLIC_ORIGIN:'https://soulvoiceai.com/norma/voice',COMPANION_ACCESS_TOKEN:randomBytes(32).toString('hex'),DOUBAO_PROTOCOL:'plan',DOUBAO_SPEECH_API_KEY:'local-only-secret'};
  const options = {env, now:()=>time,
    upstreamFactory:()=>new WebSocket(`ws://127.0.0.1:${asrPort}`), fetchImpl:async (url, options) => {
      const body = JSON.parse(options.body);
      if (url.endsWith('/chat/completions')) {
        chatBodies.push(body); chatCalls++;
        if (chatCalls === 1) return Response.json({choices:[{finish_reason:'tool_calls',message:{role:'assistant',content:null,
          tool_calls:[{id:'function-task-1',type:'function',function:{name:'create_agent',arguments:JSON.stringify({profile:'codex',task:'实现登录校验'})}}]}}]});
        return Response.json({choices:[{finish_reason:'stop',message:{content:JSON.stringify({text:'任务已接收，尚未完成。',emotion:'neutral',gesture:'none'})}}]});
      }
      speech.push(body.req_params.text);
      return new Response([
        {code:0,data:pcm.toString('base64')},
        {code:0,sentence:{text:body.req_params.text,words:[{word:body.req_params.text,startTime:0,endTime:0.1}]}},
        {code:20000000}
      ].map(value=>JSON.stringify(value)+'\n').join(''));
    }};
  const gateway = makeGateway({...options,sessionStore:cloud ? createCloudSessions({env,now:()=>time}) : undefined});
  const port = await listen(gateway);
  const other = cloud ? makeGateway({...options,sessionStore:createCloudSessions({env,now:()=>time})}) : gateway;
  const mediaPort = cloud ? await listen(other) : port;
  let phone, mac;
  try {
    const response = await fetch(`http://127.0.0.1:${port}/v1/session`,{method:'POST',
      headers:{Authorization:`Bearer ${env.COMPANION_ACCESS_TOKEN}`,'Content-Type':'application/json'},body:'{"provider":"doubao"}'});
    assert.equal(response.status,200,'媒体票据签发必须成功，不能把存储连接失败误报成 WebSocket 鉴权失败');
    const ticket = await response.json();
    assert.equal(ticket.url,'wss://soulvoiceai.com/norma/voice/v1/voice','媒体票据不能丢失代理挂载路径');
    phone = new WebSocket(`ws://127.0.0.1:${mediaPort}/v1/voice`,{headers:{Authorization:`Bearer ${ticket.token}`}});
    const media = inbox(phone); await media('session.created');
    send(phone,{type:'gateway.tools.request',request_id:'binding-one'});
    const grant = await media('gateway.tools.ticket');
    assert.equal(grant.url,'wss://soulvoiceai.com/norma/voice/v1/tools','Mac工具票据必须保留同一挂载路径');
    assert.match(grant.call_id,/^doubao_[A-Za-z0-9_-]+$/,'豆包身份不能伪装OpenAI rtc身份');
    assert.equal(grant.request_id,'binding-one','异步票据必须对应请求代次');
    assert.equal(grant.expires_at,1030,'1000秒当前时间+30秒有效期');
    assert.ok(!JSON.stringify(grant).includes(env.DOUBAO_SPEECH_API_KEY),'票据不能含长期供应商密钥');
    const openMac = call => new WebSocket(`ws://127.0.0.1:${port}/v1/tools?call_id=${call}`,
      {headers:{Authorization:`Bearer ${grant.token}`}});
    const wrong = openMac('doubao_other');
    assert.match((await once(wrong,'error'))[0].message,/401/,'错媒体身份必须被拒绝');
    mac = openMac(grant.call_id); const control = inbox(mac); await control('session.created');
    send(mac,{type:'session.update',session:{type:'realtime',instructions:'你是Norma，只汇报真实回执。',
      tools:[{type:'function',name:'create_agent',description:'创建Agent',parameters:{type:'object',properties:{profile:{type:'string'},task:{type:'string'}}}}]}});
    await control('session.updated');
    // Configure within the real 5s handshake deadline before spending network time on replay rejection.
    const reused = openMac(grant.call_id);
    assert.match((await once(reused,'error'))[0].message,/401/,'一次性票据不能开第二个执行通道');
    asrEvent({utterances:[{text:'请让 Codex 实现登录校验',definite:true,end_time:1000}]});
    const turn = await control('input_audio_buffer.speech_started');
    const transcript = await control('conversation.item.input_audio_transcription.completed');
    assert.equal(transcript.item_id,turn.item_id,'授权原话必须关联实际用户回合');
    const done = await control('response.done');
    assert.equal(done.response.output[0].call_id,'function-task-1','function身份不能被媒体身份覆盖');
    send(mac,{type:'conversation.item.create',item:{type:'function_call_output',call_id:'function-task-1',output:'{"status":"accepted","task_id":7}'}});
    send(mac,{type:'response.create'});
    const started = await media('response.output_audio.started');
    await media('response.output_audio.done');
    assert.equal(chatCalls,2,'工具后只能续答一次');
    assert.equal(JSON.parse(chatBodies[1].messages.find(message=>message.role==='tool').content).task_id,7,'续答必须使用Mac真实回执');
    send(phone,{type:'gateway.playback.done',response_id:started.response_id});
    send(phone,{type:'gateway.report',report_id:'report-one',text:'登录校验已完成，等待你验收。'});
    const report = await media('response.output_audio.started');
    assert.equal(report.report_id,'report-one','汇报音频必须携带准确批次');
    await media('response.output_audio.done');
    assert.equal(chatCalls,2,'独立汇报不能再次调用LLM或工具');
    assert.equal(speech.at(-1),'登录校验已完成，等待你验收。','TTS必须逐字读取确认的结果');
    send(phone,{type:'gateway.playback.done',response_id:report.response_id});
    send(phone,{type:'gateway.tools.request',request_id:'expires'});
    const expiring = await media('gateway.tools.ticket');
    // 30001ms = 30s ticket lifetime + 1ms outside its validity window.
    time += 30001;
    const expired = new WebSocket(`ws://127.0.0.1:${port}/v1/tools?call_id=${expiring.call_id}`,
      {headers:{Authorization:`Bearer ${expiring.token}`}});
    assert.match((await once(expired,'error'))[0].message,/401/,'过期票据不能绑定现有媒体');
    send(phone,{type:'gateway.tools.request',request_id:'before-close'});
    const closing = await media('gateway.tools.ticket');
    const macClosed=once(mac,'close');
    send(phone,{type:'session.close'}); await media('session.closed');
    await macClosed;
    const closed = new WebSocket(`ws://127.0.0.1:${port}/v1/tools?call_id=${closing.call_id}`,
      {headers:{Authorization:`Bearer ${closing.token}`}});
    assert.match((await once(closed,'error'))[0].message,/401/,'媒体关闭必须立即撤销未消费票据');
  } finally {
    phone?.terminate(); mac?.terminate();
    for (const peer of asrPeers.clients) peer.terminate();
    gateway.close(); if (other!==gateway) other.close(); asrPeers.close(); asrServer.close();
  }
}
await checkTasks();
console.log('PASS: Doubao media-bound tickets, single Mac tool continuation, literal report PCM');
