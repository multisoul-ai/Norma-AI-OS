import {createHash, randomUUID} from 'node:crypto';
import {createCloudTools} from './tools.mjs';

// ponytail: one private tester and one active paid call. Add account-scoped keys before multi-user distribution.
export function createCloudSessions({env = process.env, fetchImpl = fetch, now = Date.now} = {}) {
  const endpoint = env.KV_REST_API_URL, secret = env.KV_REST_API_TOKEN;
  if (!endpoint?.startsWith('https://') || !secret || !env.COMPANION_ACCESS_TOKEN) throw new Error('cloud_session_store_not_configured');
  const prefix = 'norma-voice:' + createHash('sha256').update(env.COMPANION_ACCESS_TOKEN).digest('hex').slice(0,24);
  const keys = [prefix+':active',prefix+':history',prefix+':rate'];
  async function evalScript(script, extraKeys = [], args = []) {
    const response = await fetchImpl(endpoint, {method:'POST', redirect:'error', signal:AbortSignal.timeout(3000),
      headers:{Authorization:`Bearer ${secret}`,'Content-Type':'application/json'},
      body:JSON.stringify(['EVAL',script,keys.length+extraKeys.length,...keys,...extraKeys,...args.map(String)])});
    if (!response.ok) throw new Error('cloud_session_store_unavailable');
    const data = await response.json();
    if (data.error) throw new Error('cloud_session_store_failed');
    return data.result;
  }
  const ticketKey = token => prefix+':ticket:'+createHash('sha256').update(token).digest('hex');
  return {
    tools:createCloudTools({evalScript,prefix,now}),
    async mint(token, resume, owner = 'admin') {
      return await evalScript(`
        if redis.call('EXISTS',KEYS[1]) == 1 then return 0 end
        redis.call('ZREMRANGEBYSCORE',KEYS[3],'-inf',ARGV[1]-60000)
        if redis.call('ZCARD',KEYS[3]) >= 6 then return 0 end
        redis.call('ZADD',KEYS[3],ARGV[1],ARGV[2])
        redis.call('PEXPIRE',KEYS[3],60000)
        redis.call('SET',KEYS[4],ARGV[3],'EX',30)
        return 1`,[ticketKey(token)],[now(),randomUUID(),(resume ? '1' : '0')+'|'+createHash('sha256').update(owner).digest('hex')]) === 1;
    },
    async claim(token) {
      if (typeof token !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(token)) return null;
      const id = randomUUID();
      const value = await evalScript(`
        local resume = redis.call('GET',KEYS[4])
        if not resume or redis.call('EXISTS',KEYS[1]) == 1 then return nil end
        redis.call('DEL',KEYS[4])
        local owner = string.sub(resume,3)
        redis.call('SET',KEYS[1],ARGV[1]..'|'..owner,'PX',45000)
        local history = string.sub(resume,1,1) == '1' and redis.call('GET',KEYS[2]) or nil
        redis.call('DEL',KEYS[2])
        return cjson.encode({owner=owner,previous=history or '{}'})`,[ticketKey(token)],[id]);
      if (value === null) return null;
      const claimed=JSON.parse(value);
      let previous;
      try {
        previous = JSON.parse(claimed.previous);
        const c = previous.conversation, s = previous.session;
        if (previous.owner!==claimed.owner || !s || typeof s.id !== 'string' || s.expires <= now() || !c || c.id !== s.id ||
            !Array.isArray(c.messages) || c.messages.length > 24 || Buffer.byteLength(value)>40000 ||
            c.messages.some(m=>!['user','assistant'].includes(m.role) || typeof m.content !== 'string')) previous = {};
      } catch { previous = {}; }
      return {id:id+'|'+claimed.owner, session:previous.session || {id:'',expires:0}, conversation:previous.conversation || {id:'',messages:[]}};
    },
    async renew(id) {
      return await evalScript(`
        if redis.call('GET',KEYS[1]) ~= ARGV[1] then return 0 end
        return redis.call('PEXPIRE',KEYS[1],45000)`,[],[id]) === 1;
    },
    async release(id, previous) {
      const value = previous?.session?.id ? JSON.stringify({...previous,owner:id.split('|')[1]}) : '';
      if (Buffer.byteLength(value)>40000) throw new Error('cloud_history_too_large');
      return await evalScript(`
        if redis.call('GET',KEYS[1]) ~= ARGV[1] then return 0 end
        if ARGV[2] ~= '' then redis.call('SET',KEYS[2],ARGV[2],'EX',1800)
        else redis.call('DEL',KEYS[2]) end
        redis.call('DEL',KEYS[1])
        return 1`,[],[id,value]) === 1;
    },
  };
}
