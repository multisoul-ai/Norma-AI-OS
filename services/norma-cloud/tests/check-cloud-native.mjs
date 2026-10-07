// Explicit cloud L3: real ASR/Chat/TTS; synthetic PCM from the native sender, isolated Mac executor.
import {once} from 'node:events';
import {createServer} from 'node:http';
import {readFileSync, writeFileSync, existsSync, realpathSync} from 'node:fs';
import {randomBytes} from 'node:crypto';
import WebSocket from 'ws';
import {makeGateway} from '../server.mjs';

if (process.env.NORMA_DOUBAO_CLOUD_L3 !== '1' || !process.env.DOUBAO_SPEECH_API_KEY) throw Error('explicit cloud L3 and Plan key required');
const configPath = process.argv[2], root = realpathSync(process.env.NORMA_NATIVE_FIXTURE_ROOT);
if (!configPath || !/^\/(private\/)?tmp\/norma-doubao-l3\.[^/]+$/.test(root)) throw Error('isolated fixture required');
const listen = async server => {server.listen(0,'127.0.0.1'); await once(server,'listening'); return server.address().port;};
const reservation = createServer(), port = await listen(reservation);
await new Promise(resolve=>reservation.close(resolve));
const origin = `http://127.0.0.1:${port}`, passphrase = randomBytes(32).toString('hex');
const stats = {audioPackets:0,chats:0,receipts:[],speech:[],users:[],tools:[],http:[],asrConnections:0};
const realCLI = process.env.NORMA_DOUBAO_REAL_CLI === '1';
if (realCLI && !existsSync(`${root}/real-cli`)) throw Error('explicit real CLI fixture required');
const turns=()=>existsSync(`${root}/turns.jsonl`) ? readFileSync(`${root}/turns.jsonl`,'utf8').trim().split('\n').filter(Boolean).map(JSON.parse) : [];
const snapshot=()=>{
  const value=JSON.stringify({...stats,realCLI,turns:turns()});
  writeFileSync(`${root}/cloud-stats.json`,value,{mode:0o600});
  return value;
};
const asr = new Set();
const gateway = makeGateway({env:{PUBLIC_ORIGIN:origin,COMPANION_ACCESS_TOKEN:passphrase,
  DOUBAO_PROTOCOL:'plan',DOUBAO_SPEECH_API_KEY:process.env.DOUBAO_SPEECH_API_KEY},
  upstreamFactory:(url, options)=>{
    const ws = new WebSocket(url,options), send = ws.send.bind(ws);
    asr.add(ws); stats.asrConnections++;
    ws.on('close',()=>asr.delete(ws));
    ws.send = (data,...args)=>{if (Buffer.isBuffer(data) && data[1]>>4 === 2) stats.audioPackets++; return send(data,...args);};
    return ws;
  },
  fetchImpl:async (url,options)=>{
    const body=JSON.parse(options.body), chat=url.endsWith('/chat/completions');
    if (chat) {
      // One spoken task normally needs lookup/create/result (<=4). Eight rounds bound paid test usage.
      if (++stats.chats > 8) throw Error('cloud L3 request budget exceeded');
      const last=body.messages.at(-1);
      if (last.role==='tool') stats.receipts.push(JSON.parse(last.content));
      if (last.role==='user') stats.users.push(last.content);
    } else stats.speech.push(body.req_params.text);
    const response=await fetch(url,options);
    stats.http.push({service:chat?'chat':'tts',status:response.status});
    if (chat && response.ok) {
      const data=await response.clone().json();
      stats.tools.push(...(data.choices?.[0]?.message?.tool_calls || []));
    }
    snapshot();
    return response;
  }});
gateway.listen(port,'127.0.0.1'); await once(gateway,'listening');
const control=createServer((req,res)=>{
  if (req.headers.authorization!==`Bearer ${passphrase}`) {res.writeHead(401);res.end();return;}
  if (req.method==='GET' && req.url==='/audio') {
    res.setHeader('Content-Type','application/octet-stream');res.end(readFileSync(`${root}/speech.pcm`));return;
  }
  if (req.method==='GET' && req.url==='/stats') {
    res.setHeader('Content-Type','application/json');res.end(snapshot());return;
  }
  if (!realCLI && req.method==='POST' && req.url==='/complete-first' && turns().length===1) {
    writeFileSync(`${root}/release-first`,'');res.end('{}');return;
  }
  res.writeHead(404);res.end();
});
const controlPort=await listen(control);
writeFileSync(configPath,JSON.stringify({origin,control:`http://127.0.0.1:${controlPort}`,passphrase,realCLI}),{mode:0o600});
process.on('SIGTERM',()=>{for (const ws of asr) ws.terminate();gateway.close();control.close();});
