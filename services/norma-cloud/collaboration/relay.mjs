import {createHash, randomBytes, randomUUID, verify, X509Certificate} from 'node:crypto';
import WebSocket, {WebSocketServer} from 'ws';

const uuid = value => typeof value === 'string' && /^[0-9a-f]{8}-(?:[0-9a-f]{4}-){3}[0-9a-f]{12}$/i.test(value);
const machine = value => typeof value === 'string' && /^mac-[0-9a-f]{64}$/.test(value);
const fields = (value, names) => Object.keys(value).sort().join(',') === names.sort().join(',');
// 256 KiB opaque payload becomes 4 * ceil(n / 3) base64 bytes, plus < 1 KiB routing metadata.
const MAX_BYTES = 256 * 1024, MAX_FRAME = 4 * Math.ceil(MAX_BYTES / 3) + 1024;
const MAX_BUFFER = 4 * MAX_BYTES;

export function certificates(env) {
  try {
    const list = JSON.parse(env.NORMA_RELAY_TRUSTED_CERTIFICATES || '[]');
    if (!Array.isArray(list) || list.length > 8) return new Map();
    const result = new Map();
    for (const encoded of list) {
      if (typeof encoded !== 'string' || encoded.length > 8192) return new Map();
      const cert = new X509Certificate(Buffer.from(encoded, 'base64'));
      if (cert.publicKey.asymmetricKeyType !== 'ec' || cert.publicKey.asymmetricKeyDetails?.namedCurve !== 'prime256v1') return new Map();
      result.set('mac-' + createHash('sha256').update(cert.raw).digest('hex'), cert.publicKey);
    }
    return result;
  } catch { return new Map(); }
}

/// One process routes opaque bytes between live endpoints. Device/project authorization and
/// durable receipts remain at the Mac; a successful send here is never a delivery receipt.
export function createCollaborationRelay({env, authorize, now = Date.now}) {
  const trusted = certificates(env), peers = new Set(), macs = new Map(), streams = new Map();
  const wss = new WebSocketServer({noServer: true, maxPayload: MAX_FRAME, perMessageDeflate: false});
  const audience = env.PUBLIC_ORIGIN || 'http://127.0.0.1:8787';

  function active(peer) {
    if (peer.closed || peer.socket.readyState !== WebSocket.OPEN) return false;
    if (peer.role === 'phone' && peer.authorize().status !== 200) {
      stop(peer, 'revoked'); return false;
    }
    return true;
  }
  function send(peer, value) {
    if (!active(peer)) return false;
    const encoded = JSON.stringify(value);
    if (peer.socket.bufferedAmount + Buffer.byteLength(encoded) > MAX_BUFFER) {
      stop(peer, 'slow_peer'); return false;
    }
    try { peer.socket.send(encoded, error => { if (error) stop(peer, 'connection_failed'); }); return true; }
    catch { stop(peer, 'connection_failed'); return false; }
  }
  function dropStream(stream, reason, initiator) {
    if (!streams.delete(stream.id)) return;
    clearTimeout(stream.timer);
    stream.phone.streams.delete(stream.id); stream.mac.streams.delete(stream.id);
    const message = {type: 'close', stream_id: stream.id, reason};
    if (stream.phone !== initiator) send(stream.phone, message);
    if (stream.mac !== initiator) send(stream.mac, message);
  }
  function cleanup(peer) {
    if (peer.closed) return;
    peer.closed = true; clearTimeout(peer.deadline); clearInterval(peer.heartbeat); peers.delete(peer);
    if (macs.get(peer.machineID) === peer) macs.delete(peer.machineID);
    for (const id of [...peer.streams]) {
      const stream = streams.get(id); if (stream) dropStream(stream, 'peer_disconnected', peer);
    }
  }
  function stop(peer, reason) {
    if (peer.closed) return;
    cleanup(peer);
    peer.socket.close(1008, reason);
    // Allow 1s for close acknowledgement, then release a non-cooperating TCP peer.
    const timer = setTimeout(() => peer.socket.terminate(), 1000); timer.unref();
    peer.socket.once('close', () => clearTimeout(timer));
  }
  function touch(stream) {
    clearTimeout(stream.timer);
    stream.timer = setTimeout(() => dropStream(stream, 'idle_timeout'), 60_000);
    stream.timer.unref();
  }
  function register(peer, value) {
    if (now() >= peer.challengeExpires) { stop(peer, 'handshake_timeout'); return; }
    const key = trusted.get(value.machine_id);
    const signature = typeof value.signature === 'string' ? Buffer.from(value.signature, 'hex') : Buffer.alloc(0);
    if (!fields(value, ['type', 'machine_id', 'signature']) || !key || signature.length < 64 || signature.length > 80 ||
        signature.toString('hex') !== value.signature ||
        !verify('sha256', Buffer.from(`Norma relay registration v1\n${audience}\n${peer.challenge}`), key, signature)) {
      stop(peer, 'unauthorized'); return;
    }
    if (macs.has(value.machine_id)) {
      send(peer, {type: 'error', error: 'already_online'}); stop(peer, 'already_online'); return;
    }
    clearTimeout(peer.deadline); peer.challenge = null;
    peer.machineID = value.machine_id; macs.set(peer.machineID, peer);
    send(peer, {type: 'registered', machine_id: peer.machineID});
  }
  function receive(peer, bytes, binary) {
    if (!active(peer)) return;
    if (now() - peer.window >= 60_000) { peer.window = now(); peer.count = 0; }
    if (++peer.count > 120) { stop(peer, 'rate_limit'); return; }
    let value;
    try { value = JSON.parse(bytes.toString()); } catch { stop(peer, 'invalid_request'); return; }
    if (binary || !value || typeof value !== 'object' || Array.isArray(value)) { stop(peer, 'invalid_request'); return; }
    if (peer.role === 'mac' && !peer.machineID) {
      if (value.type !== 'register') { stop(peer, 'unauthorized'); return; }
      register(peer, value); return;
    }
    if (value.type === 'open' && peer.role === 'phone') {
      if (!fields(value, ['type', 'request_id', 'machine_id']) || !uuid(value.request_id) || !machine(value.machine_id)) {
        stop(peer, 'invalid_request'); return;
      }
      const mac = macs.get(value.machine_id);
      if (!mac || !active(mac)) {
        send(peer, {type: 'offline', request_id: value.request_id, machine_id: value.machine_id}); return;
      }
      if (peer.streams.size >= 8 || mac.streams.size >= 8) {
        send(peer, {type: 'error', request_id: value.request_id, error: 'busy'}); return;
      }
      if ([...peer.streams].some(id => streams.get(id)?.requestID === value.request_id)) {
        stop(peer, 'duplicate_request'); return;
      }
      const stream = {id: randomUUID(), requestID: value.request_id, phone: peer, mac};
      streams.set(stream.id, stream); peer.streams.add(stream.id); mac.streams.add(stream.id); touch(stream);
      if (!send(mac, {type: 'open', stream_id: stream.id, client_id: peer.deviceID})) {
        dropStream(stream, 'peer_disconnected'); return;
      }
      send(peer, {type: 'open', stream_id: stream.id, request_id: value.request_id, machine_id: value.machine_id});
      return;
    }
    const stream = streams.get(value.stream_id);
    if (!stream || !peer.streams.has(value.stream_id)) { stop(peer, 'unknown_stream'); return; }
    if (value.type === 'close' && fields(value, ['type', 'stream_id'])) {
      dropStream(stream, 'peer_closed'); return;
    }
    if (value.type !== 'data' || !fields(value, ['type', 'stream_id', 'data']) || typeof value.data !== 'string' ||
        !value.data.length || value.data.length > 4 * Math.ceil(MAX_BYTES / 3)) { stop(peer, 'invalid_request'); return; }
    const decoded = Buffer.from(value.data, 'base64');
    if (decoded.length > MAX_BYTES || decoded.toString('base64') !== value.data) { stop(peer, 'invalid_request'); return; }
    touch(stream);
    send(peer === stream.phone ? stream.mac : stream.phone, value);
  }

  return {upgrade(req, socket, head) {
    if (!req.url?.startsWith('/v1/relay/')) return false;
    const role = req.url === '/v1/relay/mac' ? 'mac' : req.url === '/v1/relay/phone' ? 'phone' : null;
    let status = role && !Object.hasOwn(req.headers, 'origin') ? 200 : 404;
    const auth = role === 'phone' ? authorize(req) : null;
    if (status === 200 && auth) status = auth.status;
    if (status === 200 && role === 'mac' && !trusted.size) status = 503;
    if (status === 200 && [...peers].filter(peer => peer.role === role).length >= 8) status = 429;
    if (status !== 200) {
      const reason = {401: 'Unauthorized', 403: 'Forbidden', 404: 'Not Found', 429: 'Too Many Requests', 503: 'Service Unavailable'}[status];
      socket.end(`HTTP/1.1 ${status} ${reason}\r\nConnection: close\r\n\r\n`); return true;
    }
    wss.handleUpgrade(req, socket, head, ws => {
      const peer = {socket: ws, role, deviceID: auth?.deviceID, authorize: () => authorize(req),
        streams: new Set(), closed: false, window: now(), count: 0, alive: true};
      peers.add(peer);
      ws.on('message', (bytes, binary) => receive(peer, bytes, binary));
      ws.on('error', () => cleanup(peer)); ws.on('close', () => cleanup(peer));
      // 20s heartbeat detects a silent TCP loss by the second tick (at most 40s).
      ws.on('pong', () => { peer.alive = true; });
      peer.heartbeat = setInterval(() => {
        if (!active(peer)) { cleanup(peer); return; }
        if (!peer.alive) { cleanup(peer); ws.terminate(); return; }
        peer.alive = false; ws.ping();
      }, 20_000); peer.heartbeat.unref();
      if (role === 'mac') {
        peer.challenge = randomBytes(32).toString('hex');
        peer.challengeExpires = now() + 10_000;
        peer.deadline = setTimeout(() => stop(peer, 'handshake_timeout'), 10_000); peer.deadline.unref();
        send(peer, {type: 'challenge', challenge: peer.challenge, audience});
      }
    });
    return true;
  }, close() {
    for (const peer of [...peers]) { cleanup(peer); peer.socket.terminate(); }
    wss.close();
  }};
}
