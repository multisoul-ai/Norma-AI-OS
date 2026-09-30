import {createHash, verify} from 'node:crypto';
import {certificates} from './collaboration-relay.mjs';

// The same operator-approved Mac identities authorize relay routing and initial phone registration.
// This proof is never a model key or a renewable voice pass; the service persists its own device.
export function pairedRegistration(body, env, current) {
  try {
    if (Object.keys(body).sort().join(',') !== 'device_id,name,proof,token' ||
        typeof body.proof !== 'string' || body.proof.length > 4096 ||
        typeof body.token !== 'string' || !/^[A-Za-z0-9_-]{43}$/.test(body.token) ||
        typeof body.name !== 'string' || !body.name.trim() || [...body.name].length > 80) return null;
    const parts = body.proof.split('.');
    if (parts.length !== 3 || parts[0] !== 'ncr1' || !/^(?:[0-9a-f]{2}){1,1536}$/.test(parts[1]) ||
        !/^(?:[0-9a-f]{2}){64,80}$/.test(parts[2])) return null;
    const payload = Buffer.from(parts[1], 'hex'), value = JSON.parse(payload), now = Math.floor(current / 1000);
    if (Object.keys(value).sort().join(',') !== 'aud,client,exp,iat,iss,sub,token_hash,v' ||
        value.v !== 1 || value.aud !== 'https://soulvoiceai.com/norma/voice' ||
        typeof value.sub !== 'string' || !/^[A-Za-z0-9_-]{1,128}$/.test(value.sub) ||
        typeof value.client !== 'string' || !/^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/.test(value.client) ||
        value.client !== body.device_id || value.token_hash !== createHash('sha256').update(body.token).digest('hex') ||
        !Number.isSafeInteger(value.iat) || !Number.isSafeInteger(value.exp) ||
        value.iat < 0 || value.exp - value.iat !== 120 || value.iat > now + 30 || value.exp <= now) return null;
    const key = certificates(env).get(value.iss);
    if (!key || !verify('sha256', Buffer.concat([Buffer.from('Norma cloud registration v1\n'), payload]),
      key, Buffer.from(parts[2], 'hex'))) return null;
    return {owner: createHash('sha256').update(`${value.iss}\n${value.sub}`).digest('hex'), tokenHash: value.token_hash};
  } catch { return null; }
}
