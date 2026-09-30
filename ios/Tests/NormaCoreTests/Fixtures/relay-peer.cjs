const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const readline = require('node:readline');
const {execFileSync} = require('node:child_process');
const dir = process.argv[2];
const keyPath = path.join(dir, 'key.pem'), certPath = path.join(dir, 'cert.pem');
execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:P-256',
  '-nodes', '-keyout', keyPath, '-out', certPath, '-subj', '/CN=norma-native-relay-test', '-days', '1'], {stdio: 'ignore'});
const cert = new crypto.X509Certificate(fs.readFileSync(certPath)), privateKey = fs.readFileSync(keyPath);
const machine = 'mac-' + crypto.createHash('sha256').update(cert.raw).digest('hex');
process.stdout.write(JSON.stringify({machine_id: machine}) + '\n');
let keys, digest;
const aad = direction => Buffer.concat([digest, Buffer.from([direction]), Buffer.alloc(8)]);
readline.createInterface({input: process.stdin}).on('line', line => {
  const value = JSON.parse(line);
  if (value.hello) {
    const {stream, hello} = value, pair = crypto.generateKeyPairSync('x25519');
    const serverKey = pair.publicKey.export({type: 'spki', format: 'der'}).subarray(-32).toString('hex');
    const transcript = Buffer.from(`Norma collaboration session v1\n${stream}\n${hello.nonce}\n${hello.client_key}\n${serverKey}\n${machine}`);
    const remote = crypto.createPublicKey({type: 'spki', format: 'der',
      key: Buffer.concat([Buffer.from('302a300506032b656e032100', 'hex'), Buffer.from(hello.client_key, 'hex')])});
    digest = crypto.createHash('sha256').update(transcript).digest();
    const secret = crypto.diffieHellman({privateKey: pair.privateKey, publicKey: remote});
    keys = Buffer.from(crypto.hkdfSync('sha256', secret, digest, 'Norma collaboration keys v1', 64));
    const cipher = crypto.createCipheriv('aes-256-gcm', keys.subarray(32), Buffer.alloc(12));
    cipher.setAAD(aad(1));
    const frame = Buffer.concat([Buffer.alloc(8), cipher.update('Mac 已收到'), cipher.final(), cipher.getAuthTag()]);
    process.stdout.write(JSON.stringify({handshake: {version: 1, server_key: serverKey,
      certificate: cert.raw.toString('hex'), signature: crypto.sign('sha256', transcript, privateKey).toString('hex')},
      frame: frame.toString('base64')}) + '\n');
  } else {
    const frame = Buffer.from(value.frame, 'base64');
    const decipher = crypto.createDecipheriv('aes-256-gcm', keys.subarray(0, 32), Buffer.alloc(12));
    decipher.setAAD(aad(0)); decipher.setAuthTag(frame.subarray(-16));
    process.stdout.write(JSON.stringify({plaintext: Buffer.concat([decipher.update(frame.subarray(8, -16)), decipher.final()]).toString()}) + '\n');
  }
});
