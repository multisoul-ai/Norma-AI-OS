import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {once} from 'node:events';
import {makeGateway} from './server.mjs';
import {verifyVoiceAuthorization} from './voice-authorization.mjs';

/// 场景：Rust真实Mac安装身份签名，经Node网关验签后无需共享Key签发媒体票据。
/// 数据构造：TLS集成测试生成证书与控制设备授权；24×60×60=86400秒有效期。
/// 执行过程：正确签名换票；篡改签名/错部署/未登记Mac/到期边界逐一拒绝。
/// 预期结果：有效设备200且无供应商Key；匿名/伪造401，不因省去配置而匿名开放付费接口。
const fixture=JSON.parse(readFileSync(process.argv[2],'utf8'));
const {token,expires_at:expires}=fixture.authorization;
const env={COMPANION_ACCESS_TOKEN:'test-only-admin-passphrase-32chars',DOUBAO_PROTOCOL:'plan',
  DOUBAO_SPEECH_API_KEY:'test-only-doubao-key',PUBLIC_ORIGIN:'https://soulvoiceai.com/norma/voice',
  NORMA_VOICE_TRUSTED_CERTIFICATES:JSON.stringify([Buffer.from(fixture.certificate).toString('base64')])};
assert.equal(verifyVoiceAuthorization(token,env),true,'真实Mac签名应可验证');
assert.equal(verifyVoiceAuthorization(token,env,expires*1000),false,'到期瞬间必须拒绝');
assert.equal(verifyVoiceAuthorization(token,{...env,PUBLIC_ORIGIN:'https://wrong.vercel.app'}),false,'授权不可跨服务使用');
assert.equal(verifyVoiceAuthorization(token,{...env,NORMA_VOICE_TRUSTED_CERTIFICATES:'[]'}),false,'未登记Mac不能开放付费能力');
const gateway=makeGateway({env}); gateway.listen(0,'127.0.0.1'); await once(gateway,'listening');
try {
  const request=authorization=>fetch(`http://127.0.0.1:${gateway.address().port}/v1/session`,{method:'POST',
    headers:{'Content-Type':'application/json',Authorization:'Bearer '+authorization},body:'{"provider":"doubao"}'});
  const forged=token.slice(0,-2)+(token.endsWith('00')?'ff':'00');
  assert.equal((await request(forged)).status,401,'篡改签名不能换票');
  assert.equal((await request('')).status,401,'不能匿名调用付费接口');
  const response=await request(token), ticket=await response.json();
  assert.equal(response.status,200,'已配对手机无需填写Key即可换取媒体票据');
  assert.equal(ticket.url,'wss://soulvoiceai.com/norma/voice/v1/voice','票据必须指向新的同源部署');
  assert.equal(JSON.stringify(ticket).includes(env.DOUBAO_SPEECH_API_KEY),false,'票据不得携带豆包Key');
  assert.equal(JSON.stringify(ticket).includes(env.COMPANION_ACCESS_TOKEN),false,'票据不得携带管理口令');
} finally {gateway.closeAllConnections();await new Promise(resolve=>gateway.close(resolve));}
console.log('PASS: paired Mac signature, expiry, audience, trusted installation and key-free session ticket');
