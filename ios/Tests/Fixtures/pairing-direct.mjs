import {createHash, randomBytes, randomUUID, X509Certificate} from 'node:crypto';
import {execFileSync} from 'node:child_process';
import {mkdirSync, readFileSync, writeFileSync} from 'node:fs';
import {createServer} from 'node:https';
import {join} from 'node:path';
import {WebSocketServer} from '../../../services/norma-cloud/node_modules/ws/wrapper.mjs';

// Loopback Mac transport double: explicit control grant, cloud registration unavailable.
// Run: node ios/Tests/Fixtures/pairing-direct.mjs /tmp/norma-direct-pairing
// Import pairing.png into a dedicated simulator, then run the UI test with
// NORMA_TEST_MODE=pairing-direct-ui NORMA_TEST_QR=/tmp/norma-direct-pairing/offer.json
// -only-testing:NormaUITests/ConnectedComputersFlowTests/testPairingFinishesWithoutCloud
const directory = process.argv[2];
mkdirSync(directory, {recursive: true});
const key = join(directory, 'key.pem'), cert = join(directory, 'cert.pem');
execFileSync('openssl', ['req', '-x509', '-newkey', 'ec', '-pkeyopt', 'ec_paramgen_curve:prime256v1',
  '-nodes', '-keyout', key, '-out', cert, '-days', '1', '-subj', '/CN=Direct Pairing Test',
  '-addext', 'subjectAltName=IP:127.0.0.1', '-addext', 'extendedKeyUsage=serverAuth',
  '-addext', 'basicConstraints=critical,CA:FALSE', '-addext', 'keyUsage=critical,digitalSignature'], {stdio: 'ignore'});
const certificate = new X509Certificate(readFileSync(cert));
const pin = createHash('sha256').update(certificate.raw).digest('hex');
const server = createServer({key: readFileSync(key), cert: readFileSync(cert)}, (_, res) => res.end());
const sockets = new WebSocketServer({server, path: '/mobile'});
const requests = [];
let phone;
sockets.on('connection', socket => socket.on('message', bytes => {
  const request = JSON.parse(bytes), action = request.action;
  requests.push(action.type);
  writeFileSync(join(directory, 'requests.json'), JSON.stringify(requests));
  const reply = {version: 1, request_id: request.request_id};
  const grant = phone && {device_id: phone.device_id, device_name: phone.device_name,
    all_projects_access: 'control', projects: [], revision: 1, revoked: false};
  if (action.type === 'pair') {
    phone = action.request;
    const input = JSON.stringify([phone.offer_id, phone.secret, phone.device_id, phone.device_token]);
    const code = String(createHash('sha256').update(input).digest().readUInt32BE() % 1_000_000).padStart(6, '0');
    reply.result = {status: 'pending', pending: {request_id: randomUUID(), device_id: phone.device_id,
      device_name: phone.device_name, verification_code: code}};
  } else if (phone && action.device_id === phone.device_id && action.token === phone.device_token) {
    if (action.type === 'authenticate') reply.result = {status: 'authorized', grant};
    else if (action.type === 'snapshot') reply.result = {status: 'snapshot', grant, projects: []};
    else reply.error = 'unavailable';
  } else reply.error = 'unauthorized';
  socket.send(JSON.stringify(reply));
}));
server.listen(0, '127.0.0.1', () => {
  const offer = {version: 1, machine_id: 'mac-' + pin, machine_name: '直连配对验收 Mac',
    certificate_sha256: pin, endpoints: [`wss://127.0.0.1:${server.address().port}/mobile`],
    offer_id: randomBytes(32).toString('hex'), secret: randomBytes(32).toString('hex'), expires_in_seconds: 120};
  const path = join(directory, 'offer.json');
  writeFileSync(path, JSON.stringify(offer), {mode: 0o600});
  writeFileSync(join(directory, 'requests.json'), '[]');
  execFileSync('swift', ['-e', `
import Foundation
import CoreImage
import AppKit
let filter = CIFilter(name: "CIQRCodeGenerator")!
filter.setValue(try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1])), forKey: "inputMessage")
let image = filter.outputImage!.transformed(by: CGAffineTransform(scaleX: 8, y: 8))
let bitmap = NSBitmapImageRep(cgImage: CIContext().createCGImage(image, from: image.extent)!)
try bitmap.representation(using: .png, properties: [:])!.write(to: URL(fileURLWithPath: CommandLine.arguments[2]))
`, path, join(directory, 'pairing.png')]);
  console.log('DIRECT_PAIRING_FIXTURE_READY');
});
process.on('SIGTERM', () => { for (const socket of sockets.clients) socket.terminate(); server.close(() => process.exit()); });
