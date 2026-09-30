// Independent client for the Rust protocol test; it uses Node crypto, not Rust internals.
const crypto = require('node:crypto');
const readline = require('node:readline');
const pair = crypto.generateKeyPairSync('x25519');
const clientKey = pair.publicKey.export({type: 'spki', format: 'der'}).subarray(-32).toString('hex');
const nonce = crypto.randomBytes(32).toString('hex');
process.stdout.write(JSON.stringify({version: 1, client_key: clientKey, nonce}) + '\n');
readline.createInterface({input: process.stdin}).once('line', line => {
  const {stream, machine_id: machine, handshake, frame} = JSON.parse(line);
  const cert = new crypto.X509Certificate(Buffer.from(handshake.certificate, 'hex'));
  if ('mac-' + crypto.createHash('sha256').update(cert.raw).digest('hex') !== machine) throw new Error('wrong_pin');
  const transcript = scope => Buffer.from(`Norma collaboration session v1\n${scope}\n${nonce}\n${clientKey}\n${handshake.server_key}\n${machine}`);
  const signature = Buffer.from(handshake.signature, 'hex');
  const valid = crypto.verify('sha256', transcript(stream), cert.publicKey, signature);
  if (!valid) throw new Error('bad_signature');
  const serverKey = crypto.createPublicKey({type: 'spki', format: 'der',
    key: Buffer.concat([Buffer.from('302a300506032b656e032100', 'hex'), Buffer.from(handshake.server_key, 'hex')])});
  const digest = crypto.createHash('sha256').update(transcript(stream)).digest();
  const secret = crypto.diffieHellman({privateKey: pair.privateKey, publicKey: serverKey});
  const keys = Buffer.from(crypto.hkdfSync('sha256', secret, digest, 'Norma collaboration keys v1', 64));
  const aad = direction => Buffer.concat([digest, Buffer.from([direction]), Buffer.alloc(8)]);
  const received = Buffer.from(frame, 'hex');
  const decrypt = crypto.createDecipheriv('aes-256-gcm', keys.subarray(32), Buffer.alloc(12));
  decrypt.setAAD(aad(1)); decrypt.setAuthTag(received.subarray(-16));
  const plain = Buffer.concat([decrypt.update(received.subarray(8, -16)), decrypt.final()]).toString();
  const encrypt = crypto.createCipheriv('aes-256-gcm', keys.subarray(0, 32), Buffer.alloc(12));
  encrypt.setAAD(aad(0));
  const encrypted = Buffer.concat([Buffer.alloc(8), encrypt.update('修改首屏'), encrypt.final(), encrypt.getAuthTag()]);
  process.stdout.write(JSON.stringify({plaintext: plain, signature_valid: valid,
    wrong_stream_valid: crypto.verify('sha256', transcript('00000000-0000-4000-8000-000000000002'), cert.publicKey, signature),
    frame: encrypted.toString('hex')}) + '\n');
});
