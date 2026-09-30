import {X509Certificate, createHash, verify} from 'node:crypto';

export function verifyVoiceAuthorization(token, env, current = Date.now()) {
  try {
    if (typeof token!=='string' || token.length>4096) return false;
    const parts=token.split('.');
    if (parts.length!==3 || parts[0]!=='nva1' || !/^(?:[0-9a-f]{2}){1,1536}$/.test(parts[1]) ||
        !/^(?:[0-9a-f]{2}){64,80}$/.test(parts[2])) return false;
    const payload=Buffer.from(parts[1],'hex'), value=JSON.parse(payload), now=Math.floor(current/1000);
    // Only this migration accepts the old Mac audience; signature, installation and expiry still apply.
    const migrated=env.PUBLIC_ORIGIN==='https://soulvoiceai.com/norma/voice' && value.aud==='https://norma-voice.vercel.app';
    if (value.v!==1 || (value.aud!==env.PUBLIC_ORIGIN && !migrated) || typeof value.sub!=='string' || !/^[A-Za-z0-9_-]{1,128}$/.test(value.sub) ||
        !Number.isSafeInteger(value.iat) || !Number.isSafeInteger(value.exp) ||
        value.iat<0 || value.exp-value.iat!==86400 || value.iat>now+30 || value.exp<=now) return false;
    const certificates=JSON.parse(env.NORMA_VOICE_TRUSTED_CERTIFICATES || '[]');
    if (!Array.isArray(certificates) || certificates.length>8) return false;
    for (const encoded of certificates) {
      const cert=new X509Certificate(Buffer.from(encoded,'base64'));
      if (value.iss!=='mac-'+createHash('sha256').update(cert.raw).digest('hex')) continue;
      return verify('sha256',Buffer.concat([Buffer.from('Norma voice authorization v1\n'),payload]),
        cert.publicKey,Buffer.from(parts[2],'hex'));
    }
  } catch { /* Malformed or untrusted credentials never enable a paid session. */ }
  return false;
}
