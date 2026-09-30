import WebSocket from 'ws';
import {setDefaultResultOrder} from 'node:dns';
// Prefer the IPv4 route used by the verified cloud speech/model connections.
setDefaultResultOrder('ipv4first');
import {makeGateway} from '../server.mjs';
import {fetchPlan} from '../plan.mjs';
import {createCloudSessions} from '../cloud-sessions.mjs';

// Log only protocol/status codes. Credentials, transcripts and provider response bodies stay private.
const upstreamFactory=(url,options)=>{
  const peer=new WebSocket(url,options);
  peer.on('error',error=>console.error('voice_asr_transport',{
    code:/^[A-Z0-9_]+$/.test(error.code||'')?error.code:'unknown',
    status:/^Unexpected server response: ([0-9]{3})$/.exec(error.message)?.[1]||null,
  }));
  return peer;
};
const fetchImpl=async (url,options)=>{
  const service=url.endsWith('/chat/completions')?'chat':'tts',start=Date.now();
  try {
    const response=await fetchPlan(url,options);
    console.info('voice_provider_http',{service,status:response.status,elapsed_ms:Date.now()-start});
    return response;
  } catch(error) {
    const code=error.cause?.code || error.code;
    console.error('voice_provider_transport',{service,code:/^[A-Z0-9_]+$/.test(code||'')?code:'unknown',elapsed_ms:Date.now()-start,region:process.env.VERCEL_REGION});
    throw error;
  }
};
export default makeGateway({sessionStore:createCloudSessions(),upstreamFactory,fetchImpl});
