import {EventEmitter} from 'node:events';
import {createHash, randomUUID} from 'node:crypto';

// Only control JSON uses Redis. Audio stays on its original WebSocket and never enters these queues.
export function createCloudTools({evalScript, prefix, now}) {
  const hash = value => createHash('sha256').update(value).digest('hex');
  const keys = id => [prefix+':tools:'+id, prefix+':tools:'+id+':up', prefix+':tools:'+id+':down'];
  const ticket = token => prefix+':tool-ticket:'+hash(token);
  const operation = async (id, action, side = '', value = '') => evalScript(`
    if redis.call('GET',KEYS[1]) ~= redis.call('HGET',KEYS[4],'lease') then return nil end
    if redis.call('HGET',KEYS[4],'state') == 'closed' then return nil end
    local queue = ARGV[2] == 'media' and KEYS[6] or KEYS[5]
    local counter = ARGV[2] == 'media' and 'down_bytes' or 'up_bytes'
    if ARGV[1] == 'send' then
      local bytes = tonumber(redis.call('HGET',KEYS[4],counter) or '0')
      if bytes + string.len(ARGV[3]) > 1048576 or redis.call('LLEN',queue) >= 16 then return nil end
      redis.call('RPUSH',queue,ARGV[3]); redis.call('HINCRBY',KEYS[4],counter,string.len(ARGV[3]))
      redis.call('EXPIRE',queue,45); return 1
    elseif ARGV[1] == 'close' then
      redis.call('HSET',KEYS[4],'state','closed'); return 1
    end
    queue = ARGV[2] == 'media' and KEYS[5] or KEYS[6]
    counter = ARGV[2] == 'media' and 'up_bytes' or 'down_bytes'
    local values = redis.call('LRANGE',queue,0,-1)
    redis.call('DEL',queue); redis.call('HSET',KEYS[4],counter,0); redis.call('EXPIRE',KEYS[4],45)
    return {redis.call('HGET',KEYS[4],'state'),values}`,keys(id),[action,side,value]);

  class Peer extends EventEmitter {
    constructor(id, side, activate) {
      super(); this.id=id; this.side=side; this.readyState=0; this.bufferedAmount=0;
      this.writes=Promise.resolve(); this.activate=activate; this.busy=false;
      // ponytail: a personal deployment polls control queues every 250ms; use Redis pub/sub if call volume grows.
      this.timer=setInterval(()=>void this.poll(),250); void this.poll();
    }
    async poll() {
      if (this.busy || this.readyState===3) return;
      this.busy=true;
      try {
        const result=await operation(this.id,'poll',this.side);
        if (!result) {this.close(); return;}
        if (this.readyState===3) return;
        if (result[0]==='claimed' && this.readyState===0) {this.readyState=1; this.activate?.(this);}
        for (const raw of result[1]) if (this.readyState===1) this.emit('message',Buffer.from(raw),false);
      } catch {this.close();}
      finally {this.busy=false;}
    }
    send(raw) {
      if (this.readyState!==1) return;
      const size=Buffer.byteLength(raw);
      if (size>512*1024 || this.bufferedAmount+size>512*1024) {this.close(); return;}
      this.bufferedAmount+=size;
      this.writes=this.writes.then(async()=>{
        if (!await operation(this.id,'send',this.side,String(raw))) this.close();
      }).catch(()=>{void this.close();}).finally(()=>{this.bufferedAmount-=size;});
    }
    close() {
      if (this.readyState===3) return this.writes;
      this.readyState=3; clearInterval(this.timer); this.emit('close');
      this.writes=this.writes.then(()=>operation(this.id,'close')).catch(()=>{});
      return this.writes;
    }
    terminate() {void this.close();}
    bind(ws) {
      ws.on('message',(raw,binary)=>{if (binary) this.close(); else this.send(raw);});
      ws.on('close',()=>this.close()); ws.on('error',()=>this.close());
      this.on('message',raw=>{
        if (ws.readyState!==1) return;
        if (ws.bufferedAmount>512*1024) {this.close();return;}
        ws.send(raw.toString());
      });
      this.on('close',()=>ws.close(1000,'tool_relay_closed'));
      if (this.readyState===3) ws.close(1008,'media_closed');
    }
  }
  return {
    open(media, lease, attach) {
      let previous='', closed=false;
      const peers=new Set();
      return {
        async mint(token) {
          if (closed) throw Error('media_closed');
          for (const peer of peers) if (peer.readyState===0) void peer.close();
          const id=randomUUID();
          const ok=await evalScript(`
            if redis.call('GET',KEYS[1]) ~= ARGV[1] then return 0 end
            if ARGV[5] ~= '' then redis.call('DEL',ARGV[5]) end
            redis.call('HSET',KEYS[4],'lease',ARGV[1],'media',ARGV[2],'state','pending')
            redis.call('EXPIRE',KEYS[4],30)
            redis.call('SET',KEYS[7],ARGV[3]..'|'..ARGV[4],'EX',30); return 1`,
          [...keys(id),ticket(token)],[lease,media,id,now()+30000,previous]);
          if (!ok || closed) throw Error('media_closed');
          previous=ticket(token);
          const peer=new Peer(id,'media',attach); peers.add(peer);
          peer.once('close',()=>peers.delete(peer));
        },
        async close() {
          closed=true;
          await Promise.all([...peers].map(peer=>peer.close()));
        },
      };
    },
    async claim(token, media) {
      if (typeof token!=='string' || !/^[A-Za-z0-9_-]{43}$/.test(token) || !/^doubao_[A-Za-z0-9_-]{1,128}$/.test(media)) return null;
      const value=await evalScript("return redis.call('GET',KEYS[4])",[ticket(token)]);
      if (!value) return null;
      const [id,expires]=value.split('|');
      if (Number(expires)<=now()) return null;
      const ok=await evalScript(`
        if redis.call('GET',KEYS[7]) ~= ARGV[1] or redis.call('HGET',KEYS[4],'media') ~= ARGV[2]
          or redis.call('HGET',KEYS[4],'state') ~= 'pending'
          or redis.call('GET',KEYS[1]) ~= redis.call('HGET',KEYS[4],'lease') then return 0 end
        redis.call('DEL',KEYS[7]); redis.call('HSET',KEYS[4],'state','claimed')
        redis.call('EXPIRE',KEYS[4],45); return 1`,[...keys(id),ticket(token)],[value,media]);
      if (!ok) return null;
      const peer=new Peer(id,'mac'); peer.readyState=1;
      return peer;
    },
  };
}
