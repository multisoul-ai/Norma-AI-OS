import fs from 'node:fs';
import {pathToFileURL} from 'node:url';
const root = process.env.NORMA_CLAUDE_PACKAGE || '/Users/openclawd/.norma/brain/adapters/node_modules/@agentclientprotocol/claude-agent-acp';
const {ClaudeAcpAgent} = await import(pathToFileURL(root+'/dist/acp-agent.js'));
const {createSessionFailureState} = await import(pathToFileURL(root+'/dist/session-failure-extension.js'));
// Historical assistant content/error from d30307c0… / a81968b1…; SDK envelope and
// terminal result are controlled fixtures, NOT captured historical ACP stdout.
const history = {uuid:'a81968b1-1e9d-44f1-977c-3d3c3175acba',error:'rate_limit',message:{
  id:'460c905b-b3a6-4884-926a-b2326804a399',model:'<synthetic>',role:'assistant',
  content:[{type:'text',text:"You've hit your session limit · resets 6:30am (America/Los_Angeles)"}],
  usage:{input_tokens:0,output_tokens:0,cache_creation_input_tokens:0,cache_read_input_tokens:0},stop_reason:'stop_sequence'}};
const manifest=JSON.parse(fs.readFileSync(root+'/package.json'));
if(manifest.version!=='0.94.0') throw new Error('Replay requires claude-agent-acp 0.94.0');
const sid='claude-quota-controlled-replay';
const frames=[];
const logger={log(){},warn(...x){console.error(...x)},error(...x){console.error(...x)}};
const agent=new ClaudeAcpAgent({sessionUpdate:async params=>frames.push({jsonrpc:'2.0',method:'session/update',params})},logger);
agent.clientCapabilities={fs:{readTextFile:false,writeTextFile:false},terminal:false};
// Controlled SDK input: original assistant payload; terminal result reconstructed, not historical ACP.
const messages=[{type:'assistant',uuid:history.uuid,parent_tool_use_id:null,session_id:sid,error:history.error,message:history.message},
{type:'result',subtype:'success',is_error:true,result:history.message.content[0].text,usage:history.message.usage,modelUsage:{},total_cost_usd:0,num_turns:1,session_id:sid}];
let resolve; const outcome=new Promise(r=>resolve=r);
const turn={promptUuid:'quota-replay-turn',settled:false,resolve:v=>resolve({result:v}),reject:e=>resolve({error:{code:e.code,message:e.message,data:e.data}})};
const session={query:{next:async()=>messages.length?{done:false,value:messages.shift()}:new Promise(()=>{})},
input:{end(){}},cancelled:false,cwd:process.cwd(),titles:{onAssistantText(){},onResult(){}},
accumulatedUsage:{inputTokens:0,outputTokens:0,cachedReadTokens:0,cachedWriteTokens:0},accumulatedModelUsage:{},lastModelUsageReading:{},
models:{currentModelId:'sonnet',availableModels:[]},modelInfos:[],configOptions:[],
abortController:new AbortController(),cancelController:new AbortController(),contextWindowSize:200000,
toolUseCache:{},emittedToolCalls:new Set(),messageIdToUuid:new Map(),sessionFailureState:createSessionFailureState(),
activeTurn:turn,turnQueue:[turn],owedTrailingIdles:0};
agent.sessions[sid]=session;
agent.runConsumer(session,{sessionId:sid}).catch(e=>{console.error(e);process.exitCode=1});
const result=await Promise.race([outcome,new Promise((_,reject)=>setTimeout(()=>reject(new Error('consumer timeout')),2000))]);
frames.push({jsonrpc:'2.0',id:5,...result});

console.log(JSON.stringify(frames,null,2));
if(!result.error?.message.includes('session limit')) throw new Error('Quota reason missing from ACP error');
process.exit(0);
