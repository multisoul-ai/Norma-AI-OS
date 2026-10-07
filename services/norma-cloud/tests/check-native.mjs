// L3 provider boundary: production gateway, local ASR/Chat/TTS, actual Mac tool executor.
import {once} from 'node:events';
import {createServer} from 'node:http';
import {writeFileSync, readFileSync, existsSync, realpathSync} from 'node:fs';
import {randomBytes} from 'node:crypto';
import WebSocket, {WebSocketServer} from 'ws';
import {makeGateway} from '../server.mjs';

const configPath = process.argv[2];
if (!configPath) throw Error('private output config path required');
const fixtureRoot = realpathSync(process.env.NORMA_NATIVE_FIXTURE_ROOT);
if (!/^\/private\/tmp\/norma-doubao-l3\.[^/]+$|^\/tmp\/norma-doubao-l3\.[^/]+$/.test(fixtureRoot)) throw Error('isolated provider root required');
const turns = () => existsSync(`${fixtureRoot}/turns.jsonl`)
  ? readFileSync(`${fixtureRoot}/turns.jsonl`,'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const listen = async server => {
  server.listen(0, '127.0.0.1'); await once(server, 'listening'); return server.address().port;
};
const passphrase = randomBytes(32).toString('hex');
const stats = {frames:0, chats:0, receipts:[], speech:[]};
let asr;
let transcriptEnd = 0;
const asrServer = createServer(), peers = new WebSocketServer({server:asrServer});
const asrPort = await listen(asrServer);
function transcribe(result) {
  const body = Buffer.from(JSON.stringify({result})), prefix = Buffer.alloc(12);
  Buffer.from([0x11,0x91,0x10,0]).copy(prefix);
  prefix.writeInt32BE(1,4); prefix.writeUInt32BE(body.length,8);
  asr.send(Buffer.concat([prefix,body]));
}
peers.on('connection', ws => {
  asr=ws;
  ws.once('message',()=>transcribe({}));
  ws.on('message',()=>stats.frames++);
});
// Reserve then release an ephemeral port before constructing the fixed PUBLIC_ORIGIN.
// A racing bind fails the fixture rather than advertising a wrong-port tool ticket.
const reservation = createServer(), port = await listen(reservation);
await new Promise(resolve=>reservation.close(resolve));
const origin = `http://127.0.0.1:${port}`;
const pcm = Buffer.alloc(9600); // 24000Hz × 0.2s × 2bytes: audible native PCM, not an event-only ACK.
for (let i=0;i<4800;i++) pcm.writeInt16LE(Math.round(Math.sin(i*2*Math.PI*440/24000)*1000),i*2);
const gateway = makeGateway({env:{PUBLIC_ORIGIN:origin,COMPANION_ACCESS_TOKEN:passphrase,
  DOUBAO_PROTOCOL:'plan',DOUBAO_SPEECH_API_KEY:'local-provider-only'},
  upstreamFactory:()=>new WebSocket(`ws://127.0.0.1:${asrPort}`),
  fetchImpl:async (url, options) => {
    const body=JSON.parse(options.body);
    if (url.endsWith('/chat/completions')) {
      stats.chats++;
      const last=body.messages.at(-1);
      if (last.role==='tool') {
        stats.receipts.push(JSON.parse(last.content));
        return Response.json({choices:[{finish_reason:'stop',message:{content:JSON.stringify({text:'任务已接收，尚未完成。',emotion:'neutral',gesture:'none'})}}]});
      }
      let tool;
      if (last.content.includes('请让 Codex')) tool=['create_agent',{profile:'codex',task:'创建 result.txt，内容为 realtime complete'}];
      if (last.content.includes('queue second')) tool=['assign_agent_task',{agent:'Alan',task:'添加 queue second 到 result.txt'}];
      if (last.content.includes('cancelled.txt')) tool=['assign_agent_task',{agent:'Alan',task:'创建 cancelled.txt'}];
      if (last.content.startsWith('请取消任务 ')) tool=['cancel_agent_task',{task_id:Number(last.content.split(' ').at(-1))}];
      if (last.content==='查询任务状态') tool=['find_agent_tasks',{states:['succeeded','cancelled']}];
      if (tool) return Response.json({choices:[{finish_reason:'tool_calls',message:{role:'assistant',content:null,
        tool_calls:[{id:`native-function-${stats.chats}`,type:'function',function:{name:tool[0],arguments:JSON.stringify(tool[1])}}]}}]});
      return Response.json({choices:[{finish_reason:'stop',message:{content:JSON.stringify({text:'可以先讨论，尚未派工。',emotion:'neutral',gesture:'none'})}}]});
    }
    stats.speech.push(body.req_params.text);
    return new Response([{code:0,data:pcm.toString('base64')},
      {code:0,sentence:{text:body.req_params.text,words:[{word:body.req_params.text,startTime:0,endTime:0.2}]}},
      {code:20000000}].map(value=>JSON.stringify(value)+'\n').join(''));
  }});
gateway.listen(port,'127.0.0.1'); await once(gateway,'listening');
const control = createServer(async (req,res)=>{
  if (req.headers.authorization!==`Bearer ${passphrase}`) {res.writeHead(401); res.end(); return;}
  if (req.method==='GET' && req.url==='/stats') {
    res.setHeader('Content-Type','application/json'); res.end(JSON.stringify({...stats,turns:turns()})); return;
  }
  if (req.method==='POST' && req.url==='/complete-first' && turns().length===1) {
    writeFileSync(`${fixtureRoot}/release-first`,'release\n'); res.end('{}'); return;
  }
  const text = {'/discuss':'这个思路不错', '/dispatch':'请让 Codex 创建 result.txt，内容为 realtime complete',
    '/queue':'请让 Alan 添加 queue second 到 result.txt', '/queue-cancel':'请让 Alan 创建 cancelled.txt',
    '/cancel':`请取消任务 ${stats.receipts[2]?.task_id}`, '/tasks':'查询任务状态'}[req.url];
  if (req.method==='POST' && text && asr?.readyState===WebSocket.OPEN) {
    transcriptEnd += 1000;
    transcribe({utterances:[{text,definite:true,end_time:transcriptEnd}]});
    res.end('{}'); return;
  }
  res.writeHead(404); res.end();
});
const controlPort = await listen(control);
writeFileSync(configPath,JSON.stringify({origin,control:`http://127.0.0.1:${controlPort}`,passphrase}),{mode:0o600});
process.on('SIGTERM',()=>{
  for(const peer of peers.clients) peer.terminate();
  peers.close(); asrServer.close(); gateway.close(); control.close();
});
