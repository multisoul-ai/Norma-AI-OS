import assert from 'node:assert/strict';
import * as crypto from 'node:crypto';
import {readFileSync, writeFileSync, mkdtempSync, rmSync, existsSync} from 'node:fs';
import {join} from 'node:path';
import {tmpdir} from 'node:os';
import {createInterface} from 'node:readline';
import {once} from 'node:events';
import {spawn} from 'node:child_process';
import {createServer} from 'node:http';
import {fileURLToPath} from 'node:url';
import WebSocket, {WebSocketServer} from 'ws';
import {makeGateway} from './server.mjs';
import {checkThreadTerminal} from './check-thread-terminal.mjs';
import {checkThreadTerminalControl} from './check-thread-terminal-control.mjs';
import {checkThreadFollowup} from './check-thread-followup.mjs';

// Called by the Rust e2e: production gateway + real MobileServer; this phone is a protocol fixture.
const fixture = JSON.parse(readFileSync(process.argv[2]));
const directory = mkdtempSync(join(tmpdir(), 'norma-mac-relay-'));
const admin = crypto.randomBytes(32).toString('base64url');
const env = {COMPANION_ACCESS_TOKEN: admin, NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json'),
  NORMA_RELAY_TRUSTED_CERTIFICATES: JSON.stringify([Buffer.from(fixture.certificate).toString('base64')])};
let server = makeGateway({env});
const lines = createInterface({input: process.stdin})[Symbol.asyncIterator]();
const sockets = new Set();
const connections = new Map();
function track(server) {
  const active = new Set(); connections.set(server, active);
  server.on('connection', socket => { active.add(socket); socket.once('close', () => active.delete(socket)); });
}
async function shutdown(server) {
  for (const socket of connections.get(server) || []) socket.destroy();
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
}
// Budget covers startup, negative handshakes, revoke and stop; hangs must fail rather than block Rust.
// Optional native: 40s network +80s Swift build/tests +60s between independent clients' rate windows.
const native = process.env.NORMA_NATIVE_RELAY_CHECK === '1';
const nativeControl = process.env.NORMA_NATIVE_TERMINAL_CONTROL_CHECK === '1';
const nativeFollowup = process.env.NORMA_NATIVE_FOLLOWUP_CHECK === '1';
const reviewUI = process.env.NORMA_REVIEW_UI_FIXTURE;
const terminalUI = process.env.NORMA_TERMINAL_UI_FIXTURE;
const deadline = setTimeout(() => { console.error('Mac relay network check timed out'); process.exit(1); }, (reviewUI || terminalUI) ? 900_000 : (native || nativeControl || nativeFollowup) ? 180_000 : 40_000);
let origin, token;
const report = phase => process.stdout.write(JSON.stringify({phase}) + '\n');

function mailbox(ws) {
  const queued = [], waiting = [];
  let failed;
  const deliver = value => waiting.length ? waiting.shift().resolve(value) : queued.push(value);
  ws.on('message', bytes => deliver(JSON.parse(bytes)));
  ws.on('error', error => { failed = error; while (waiting.length) waiting.shift().reject(error); });
  ws.on('close', () => deliver({type: 'socket_closed'}));
  return () => {
    if (queued.length) return Promise.resolve(queued.shift());
    if (failed) return Promise.reject(failed);
    return new Promise((resolve, reject) => {
      const waiter = {resolve: value => {clearTimeout(timer); resolve(value);}, reject: error => {clearTimeout(timer); reject(error);}};
      const timer = setTimeout(() => {const i = waiting.indexOf(waiter); if (i >= 0) waiting.splice(i, 1); reject(new Error('peer timeout'));}, 5000);
      waiting.push(waiter);
    });
  };
}

async function phone() {
  const ws = new WebSocket(origin.replace('http:', 'ws:') + '/v1/relay/phone', {headers: {Authorization: `Bearer ${token}`}});
  sockets.add(ws); ws.once('close', () => sockets.delete(ws));
  const next = mailbox(ws);
  await once(ws, 'open');
  const send = value => ws.send(JSON.stringify(value));
  const request = crypto.randomUUID();
  let opened;
  // Registration races this independent outbound connection; retry only transport open, never tasks.
  for (let attempt = 0; attempt < 50; attempt++) {
    send({type: 'open', request_id: request, machine_id: fixture.machine_id});
    opened = await next();
    if (opened.type !== 'offline') break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(opened.type, 'open', 'Mac must register through its outbound connection');
  const stream = opened.stream_id;
  const pair = crypto.generateKeyPairSync('x25519');
  const clientKey = pair.publicKey.export({type: 'spki', format: 'der'}).subarray(-32).toString('hex');
  const nonce = crypto.randomBytes(32).toString('hex');
  const sendBytes = bytes => send({type: 'data', stream_id: stream, data: bytes.toString('base64')});
  sendBytes(Buffer.from(JSON.stringify({version: 1, client_key: clientKey, nonce})));
  const handshakeFrame = await next();
  assert.equal(handshakeFrame.type, 'data');
  const reply = JSON.parse(Buffer.from(handshakeFrame.data, 'base64'));
  const cert = new crypto.X509Certificate(Buffer.from(reply.certificate, 'hex'));
  assert.equal('mac-' + crypto.createHash('sha256').update(cert.raw).digest('hex'), fixture.machine_id, 'Pinned Mac identity must match');
  const transcript = Buffer.from(`Norma collaboration session v1\n${stream}\n${nonce}\n${clientKey}\n${reply.server_key}\n${fixture.machine_id}`);
  assert.equal(crypto.verify('sha256', transcript, cert.publicKey, Buffer.from(reply.signature, 'hex')), true, 'Mac must sign this exact session');
  const remote = crypto.createPublicKey({type: 'spki', format: 'der',
    key: Buffer.concat([Buffer.from('302a300506032b656e032100', 'hex'), Buffer.from(reply.server_key, 'hex')])});
  const digest = crypto.createHash('sha256').update(transcript).digest();
  const keys = Buffer.from(crypto.hkdfSync('sha256', crypto.diffieHellman({privateKey: pair.privateKey, publicKey: remote}), digest, 'Norma collaboration keys v1', 64));
  let sent = 0n, received = 0n;
  return {ws, next, async request(action) {
    const id = crypto.randomUUID(), sequence = Buffer.alloc(8); sequence.writeBigUInt64BE(sent++);
    const cipher = crypto.createCipheriv('aes-256-gcm', keys.subarray(0, 32), Buffer.concat([Buffer.alloc(4), sequence]));
    cipher.setAAD(Buffer.concat([digest, Buffer.from([0]), sequence]));
    const plaintext = JSON.stringify({version: 1, request_id: id, machine_id: fixture.machine_id, action});
    sendBytes(Buffer.concat([sequence, cipher.update(plaintext), cipher.final(), cipher.getAuthTag()]));
    const event = await next();
    if (event.type !== 'data') return event;
    assert.equal(event.stream_id, stream, 'Reply cannot cross relay streams');
    const frame = Buffer.from(event.data, 'base64');
    assert.equal(frame.readBigUInt64BE(), received++, 'Replies must be ordered and unreplayed');
    const decipher = crypto.createDecipheriv('aes-256-gcm', keys.subarray(32), Buffer.concat([Buffer.alloc(4), frame.subarray(0, 8)]));
    decipher.setAAD(Buffer.concat([digest, Buffer.from([1]), frame.subarray(0, 8)]));
    decipher.setAuthTag(frame.subarray(-16));
    const result = JSON.parse(Buffer.concat([decipher.update(frame.subarray(8, -16)), decipher.final()]));
    assert.equal(result.request_id, id, 'Reply must belong to the original request');
    return result;
  }};
}

try {
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  origin = `http://127.0.0.1:${server.address().port}`;
  // The relay captures audience at construction; match its documented development default.
  // Bind fixture to the actual port by recreating the gateway after discovering a free address.
  await new Promise(resolve => server.close(resolve));
  env.PUBLIC_ORIGIN = origin;
  server = makeGateway({env});
  track(server);
  server.listen(Number(new URL(origin).port), '127.0.0.1'); await once(server, 'listening');
  const invite = await fetch(origin + '/v1/admin/devices/invite', {method: 'POST', headers: {
    Authorization: `Bearer ${admin}`, 'Content-Type': 'application/json'}, body: '{}'}).then(r => r.json());
  const claimed = await fetch(origin + '/v1/devices/claim', {method: 'POST', headers: {'Content-Type': 'application/json'},
    body: JSON.stringify({code: invite.code, name: 'Relay test phone'})}).then(r => r.json());
  token = claimed.token; assert.ok(token, 'Real cloud invitation must succeed');
  process.stdout.write(JSON.stringify({port: Number(new URL(origin).port)}) + '\n');
  await lines.next();
  const beforeAuth = await phone();
  assert.equal((await beforeAuth.request({type: 'snapshot', device_id: '86AAD6D7-2A37-4D69-BAF7-03C424336A35', token: 'a'.repeat(64)})).type, 'close', 'A paired token alone cannot skip session authentication');
  beforeAuth.ws.close();
  const bad = await phone();
  assert.equal((await bad.request({type: 'authenticate', device_id: '86AAD6D7-2A37-4D69-BAF7-03C424336A35', token: 'b'.repeat(64)})).type, 'close', 'Cloud access cannot replace Mac pairing');
  bad.ws.close();
  let valid = await phone();
  const credentials = {device_id: '86AAD6D7-2A37-4D69-BAF7-03C424336A35', token: 'a'.repeat(64)};
  const authorized = await valid.request({type: 'authenticate', ...credentials});
  assert.equal(authorized.result.status, 'authorized');
  assert.equal(authorized.result.capabilities.includes('collaboration_v1'), false, 'Do not advertise unfinished durable delivery');
  assert.equal(authorized.result.capabilities.includes('thread_delivery_v1'), true, 'Mac must explicitly advertise durable message reception');
  assert.equal(authorized.result.capabilities.includes('thread_tasks_v1'), true, 'Task dispatch must be explicitly negotiated');
  assert.equal(authorized.result.capabilities.includes('thread_task_acceptance_v1'), true, 'Frozen task acceptance must be explicitly negotiated');
  assert.equal(authorized.result.capabilities.includes('thread_task_revision_v1'), true, 'Artifact revision tasks must explicitly validate their source');
  const message = {operation_id: crypto.randomUUID(), collaboration_workspace_id: crypto.randomUUID(),
    thread_id: crypto.randomUUID(), message_id: crypto.randomUUID(), origin_device_id: crypto.randomUUID(),
    project_ids: [fixture.project_id], text: '讨论首屏：先明确目标，不执行任务。'};
  const receiptQuery = {type: 'thread_receipt', ...credentials, collaboration_workspace_id: message.collaboration_workspace_id,
    thread_id: message.thread_id, operation_id: message.operation_id};
  const missing = await valid.request(receiptQuery);
  assert.equal(missing.result.status, 'thread_receipt');
  assert.equal(missing.result.thread_receipt, null, 'Unsent message cannot acquire a receipt');
  const delivered = await valid.request({type: 'thread_message', ...credentials, message});
  assert.equal(delivered.result.thread_receipt.state, 'received', 'Only Mac persisted reception counts as delivery');
  assert.equal(delivered.result.thread_receipt.message_id, message.message_id);
  const originalReceipt = delivered.result.thread_receipt;
  assert.deepEqual((await valid.request({type: 'thread_message', ...credentials, message})).result.thread_receipt, originalReceipt, 'Lost acknowledgement retry must return the same event');
  assert.equal((await valid.request({type: 'thread_message', ...credentials, message: {...message, text: '换一条执行指令'}})).error, 'idempotency_conflict', 'Operation content is immutable');
  const eventQuery = {type: 'thread_events', ...credentials, collaboration_workspace_id: message.collaboration_workspace_id, thread_id: message.thread_id, after: 0};
  assert.deepEqual((await valid.request(eventQuery)).result.thread_events, [originalReceipt], 'Retries cannot duplicate visible events');
  assert.deepEqual((await valid.request({...eventQuery, thread_id: crypto.randomUUID()})).result.thread_events, [], 'Other threads cannot receive this event');
  const snapshot = await valid.request({type: 'snapshot', ...credentials});
  assert.deepEqual(snapshot.result.projects.map(p => p.workspace_id), [fixture.project_id], 'Only the paired project is visible');
  const forbidden = await phone();
  await forbidden.request({type: 'authenticate', ...credentials});
  assert.equal((await forbidden.request({type: 'pairing_code', message: 'network-pairing-forbidden'})).type, 'close', 'Relay must not expose a second pairing entry');
  forbidden.ws.close();
  const task = {operation_id: crypto.randomUUID(), collaboration_workspace_id: message.collaboration_workspace_id,
    thread_id: message.thread_id, source_operation_id: message.operation_id, source_message_id: message.message_id,
    project_id: fixture.project_id, agent_definition_id: 2, task: '修复首屏导航', context_version: 1,
    context: '已确认目标：首屏只展示当前任务。'};
  const dispatch = {type: 'thread_task', ...credentials, route_id: 'route-a', grant_revision: 1, task};
  assert.equal((await valid.request({...dispatch, task: {...task, agent_definition_id: 0}})).error, 'invalid_request', 'Agent definition must be positive');
  assert.equal((await valid.request({...dispatch, task: {...task, context_version: 0}})).error, 'invalid_request', 'Shared context must have a version');
  assert.equal((await valid.request({...dispatch, task: {...task, task: 'x'.repeat(32 * 1024)}})).error, 'invalid_request', 'Context shares the 32 KiB task budget');
  const reader = await phone();
  const readOnly = {device_id: 'relay-reader', token: 'c'.repeat(64)};
  await reader.request({type: 'authenticate', ...readOnly});
  assert.equal((await reader.request({...dispatch, ...readOnly})).type, 'close', 'Reading a project cannot authorize task execution');
  reader.ws.close();
  const taskQuery = {type: 'thread_task_receipt', ...credentials, collaboration_workspace_id: task.collaboration_workspace_id,
    thread_id: task.thread_id, operation_id: task.operation_id};
  assert.equal((await valid.request(taskQuery)).result.thread_task, null, 'Unsent task cannot have an execution receipt');
  const acceptedTask = (await valid.request(dispatch)).result.thread_task;
  assert.equal(acceptedTask.receipt.state, 'accepted', 'Queue reception is not Agent completion');
  assert.equal(acceptedTask.binding.source_message_id, message.message_id, 'Task must bind the original visible instruction');
  assert.deepEqual((await valid.request({...dispatch, route_id: 'route-b'})).result.thread_task, acceptedTask,
    'Changed route retry must return original command without another execution');
  assert.equal((await valid.request({...dispatch, task: {...task, task: '改写指令'}})).error, 'idempotency_conflict');
  assert.equal((await valid.request({...dispatch, task: {...task, agent_definition_id: 3}})).error, 'idempotency_conflict');
  assert.equal((await valid.request({...taskQuery, thread_id: crypto.randomUUID()})).result.thread_task, null, 'Another thread must not read this task');
  assert.equal((await valid.request({...dispatch, grant_revision: 99})).error, 'grant_changed', 'Stale attempts cannot use an old grant');
  report('task-submitted'); await lines.next();
  let taskResult;
  // 50 * 20ms = 1s for the existing completion writer; no fake execution success is generated by the gateway.
  for (let attempt = 0; attempt < 50; attempt++) {
    taskResult = (await valid.request(taskQuery)).result.thread_task;
    if (taskResult.receipt.state !== 'accepted') break;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.deepEqual(taskResult.receipt.result, {status: 'accepted', task_id: 7, node_id: 2, task_ref: 'test-runtime:7'}, 'Only the fixture foreground reply can finish dispatch');
  assert.equal(taskResult.receipt.route_id, 'route-a', 'Retry cannot overwrite original execution route');
  assert.equal(taskResult.receipt.state, 'succeeded', 'Dispatch receipt must be durable');
  assert.equal(taskResult.execution?.execution.state, 'done', 'Persistent task completion must return through the original thread receipt');
  assert.equal(taskResult.execution.execution.task_ref, 'test-runtime:7', 'Reused numeric ID must not select another runtime task');
  assert.equal(taskResult.execution.execution.node_id, 2, 'Original task must remain on node 2, not later node 3');
  assert.deepEqual(taskResult.execution.execution.result, {text: '原任务：首屏导航已修复', truncated: false}, 'Return the durable original Agent reply');
  const savedDelivery = taskResult.execution.execution.delivery;
  if (process.env.NORMA_THREAD_FOLLOWUP_CHECK === '1') {
    await checkThreadFollowup(phone, credentials, readOnly, valid, task, message, report, lines, nativeFollowup ? async (followup, parentSource, followupSource) => {
      const root = fileURLToPath(new URL('../../', import.meta.url));
      const path = join(directory, 'native-followup.json');
      writeFileSync(path, JSON.stringify({port: Number(new URL(origin).port), machineID: fixture.machine_id,
        task, followup, parentSource, followupSource, cloud: {deviceID: claimed.device_id, token},
        mac: {deviceID: credentials.device_id, token: credentials.token}}), {mode: 0o600});
      const child = spawn('swift', ['test', '--package-path', join(root, 'ios'), '--filter', 'ThreadFollowupNetworkTests'], {
        cwd: root, env: {...process.env, NORMA_NATIVE_FOLLOWUP_FIXTURE: path}, stdio: ['ignore', 2, 2],
      });
      const [code, signal] = await once(child, 'exit');
      assert.equal(signal, null, 'Swift follow-up client must finish normally');
      assert.equal(code, 0, 'Swift must recover the same follow-up through the production relay');
    } : undefined);
  } else if (process.env.NORMA_TERMINAL_CONTROL_CHECK === '1') {
    valid.ws.close();
    await checkThreadTerminalControl(phone, credentials, task, report, lines, (nativeControl || terminalUI) ? async () => {
      // Independent Swift client begins in a fresh real60s window; no service limit is weakened.
      await new Promise(resolve => setTimeout(resolve, 60_100));
      if (terminalUI) {
        assert.equal(existsSync(terminalUI), false, 'Never overwrite terminal UI credentials');
        writeFileSync(terminalUI, JSON.stringify({port: Number(new URL(origin).port), namespace: crypto.randomUUID(),
          machineID: fixture.machine_id, projectID: fixture.project_id, endpoints: fixture.endpoints, message, task,
          cloud: {deviceID: claimed.device_id, token}, mac: {deviceID: credentials.device_id, token: credentials.token}}), {mode: 0o600});
        console.error('Native terminal UI fixture ready');
        while (!existsSync(terminalUI + '.finished')) await new Promise(resolve => setTimeout(resolve, 500));
        rmSync(terminalUI); rmSync(terminalUI + '.finished');
        return;
      }
      const root = fileURLToPath(new URL('../../', import.meta.url));
      const path = join(directory, 'native-terminal.json');
      writeFileSync(path, JSON.stringify({port: Number(new URL(origin).port), machineID: fixture.machine_id,
        task, cloud: {deviceID: claimed.device_id, token}, mac: {deviceID: credentials.device_id, token: credentials.token}}), {mode: 0o600});
      const child = spawn('swift', ['test', '--disable-sandbox', '--package-path', join(root, 'ios'),
        '--filter', 'ThreadTerminalNetworkTests'], {
        cwd: root, env: {...process.env, NORMA_NATIVE_TERMINAL_FIXTURE: path}, stdio: ['ignore', 2, 2],
      });
      const [code, signal] = await once(child, 'exit');
      assert.equal(signal, null, 'Swift terminal client must finish normally');
      assert.equal(code, 0, 'Swift must validate real relay control, receipt and reconnect semantics');
    } : undefined);
  } else {
  const terminalScope = await checkThreadTerminal(phone, credentials, task, fixture.project_id, readOnly);
  assert.equal(savedDelivery.file_count, 1, 'Original turn has exactly one frozen file');
  const reviewRequest = {collaboration_workspace_id: message.collaboration_workspace_id,
    thread_id: task.thread_id, operation_id: task.operation_id, task_ref: 'test-runtime:7',
    fingerprint: savedDelivery.fingerprint, offset: 0};
  const readReview = request => valid.request({type: 'thread_task_review', ...credentials, request});
  const chunks = [];
  let totalBytes;
  do {
    const query = {...reviewRequest, offset: chunks.reduce((n, value) => n + value.length, 0)};
    const reply = await readReview(query);
    assert.equal(reply.result?.status, 'thread_task_review', 'Frozen file contents must be readable over the real relay');
    const part = reply.result.thread_review;
    assert.deepEqual(part.request, query, 'Every chunk must echo the exact task/fingerprint/offset');
    const bytes = Buffer.from(part.data, 'base64');
    assert.equal(bytes.toString('base64'), part.data, 'Chunk bytes must be canonical base64');
    assert.ok(bytes.length > 0 && bytes.length <= 160 * 1024, 'Each raw chunk must fit the relay budget after base64');
    totalBytes ??= part.total_bytes;
    assert.equal(part.total_bytes, totalBytes, 'Frozen transfer length cannot change between chunks');
    chunks.push(bytes);
  } while (chunks.reduce((n, value) => n + value.length, 0) < totalBytes);
  assert.ok(chunks.length > 1, 'The fixture must exercise multi-frame delivery');
  const frozenReview = JSON.parse(Buffer.concat(chunks).toString('utf8'));
  assert.equal(frozenReview.delivery.fingerprint, savedDelivery.fingerprint, 'The displayed artifact is the selected frozen turn');
  assert.equal(frozenReview.review.files[0].path, 'result.txt', 'Only the original frozen path is returned');
  assert.equal(Buffer.from(frozenReview.review.files[0].after.bytes).toString('utf8'), '结果\n'.repeat(20_000), 'UTF-8 file bytes must survive fragment boundaries without loss');
  assert.equal((await readReview({...reviewRequest, fingerprint: 'f'.repeat(64)})).error, 'not_found', 'A different artifact cannot be substituted');
  assert.equal((await readReview({...reviewRequest, task_ref: 'other-runtime:7'})).error, 'not_found', 'Same numeric task ID in a new runtime cannot read the original result');
  assert.equal((await readReview({...reviewRequest, thread_id: crypto.randomUUID()})).error, 'not_found', 'Other threads must not read original file bytes');
  assert.equal((await readReview({...reviewRequest, offset: totalBytes})).error, 'invalid_request', 'Past-end offsets cannot produce empty success frames');
  assert.deepEqual((await valid.request(taskQuery)).result.thread_task.execution, taskResult.execution, 'Repeated polling must reuse event identity and cursor');
  assert.equal(authorized.result.capabilities.includes('thread_task_results_v1'), true, 'Durable execution results require an explicit capability');
  assert.equal((await valid.request({...taskQuery, thread_id: crypto.randomUUID()})).result.thread_task, null, 'Completed execution must remain isolated from other threads');
  const otherReader = await phone();
  await otherReader.request({type: 'authenticate', ...readOnly});
  assert.equal((await otherReader.request({type: 'thread_task_review', ...readOnly, request: reviewRequest})).error, 'not_found', 'Another paired device cannot read original file bytes');
  assert.equal((await otherReader.request({...taskQuery, ...readOnly})).result.thread_task, null, 'Another paired device cannot read this phones task or reply');
  otherReader.ws.close();
  assert.equal((await valid.request({...dispatch, task: {...task, operation_id: crypto.randomUUID(),
    source_message_id: crypto.randomUUID()}})).error, 'source_message_missing', 'Unrelated source cannot dispatch');
  const revisionMessage = {...message, operation_id: crypto.randomUUID(), message_id: crypto.randomUUID(), text: '把按钮改为蓝色'};
  await valid.request({type: 'thread_message', ...credentials, message: revisionMessage});
  const revisionTask = {...task, operation_id: crypto.randomUUID(), source_operation_id: revisionMessage.operation_id,
    source_message_id: revisionMessage.message_id, task: '根据原成果修改：\n把按钮改为蓝色', revision_of: {
      operation_id: task.operation_id, task_ref: 'test-runtime:7', fingerprint: savedDelivery.fingerprint}};
  const revise = {...dispatch, task: revisionTask};
  assert.equal((await valid.request({...revise, task: {...revisionTask, revision_of: {...revisionTask.revision_of, fingerprint: 'f'.repeat(64)}}})).error,
    'not_found', 'A revision must bind the actual frozen artifact');
  assert.equal((await valid.request({...revise, task: {...revisionTask, agent_definition_id: 3}})).error,
    'invalid_revision_target', 'A revision must not silently switch to another Agent');
  const revisionQueued = (await valid.request(revise)).result.thread_task;
  assert.equal(revisionQueued.receipt.state, 'accepted', 'Revision is a separate tracked task');
  assert.equal(JSON.parse(revisionQueued.binding.payload).revision_of.operation_id, task.operation_id, 'Mac must persist the original delivery link');
  assert.deepEqual((await valid.request({...revise, route_id: 'revision-retry'})).result.thread_task, revisionQueued, 'Repeated return-for-revision cannot dispatch twice');
  report('revision-submitted'); await lines.next();
  const revisionQuery = {...taskQuery, operation_id: revisionTask.operation_id};
  let revised;
  // The writer has at most50×20ms=1s to persist the fixture dispatch acknowledgement.
  for (let attempt = 0; attempt < 50; attempt++) {
    revised = (await valid.request(revisionQuery)).result.thread_task;
    if (revised.receipt.state !== 'accepted') break;
    await new Promise(resolve => setTimeout(resolve, 20));
  }
  assert.equal(revised.execution.execution.task_ref, 'revision-runtime:8', 'Revision returns under its own persistent identity');
  assert.equal(revised.execution.execution.result.text, '修改任务：按钮已改为蓝色', 'Revision reply returns through the same thread');
  assert.equal(revised.execution.execution.delivery.outcome.key.turn_id, 'revision-turn', 'New delivery must remain distinct from the original');
  assert.equal(revised.binding.thread_id, task.thread_id, 'Revision result cannot create an unrelated conversation');
  assert.deepEqual((await valid.request(taskQuery)).result.thread_task.execution, taskResult.execution, 'Revision must not overwrite the original reply or frozen artifact');
  // A separate decision binds the frozen artifact; it never mutates the original execution event.
  const {offset: _offset, ...target} = reviewRequest;
  const decision = {type: 'accept_delivery', operation_id: crypto.randomUUID(), target};
  const acceptanceQuery = {type: 'thread_task_acceptance', ...credentials, decision};
  const accept = {type: 'thread_task_accept', ...credentials, decision, route_id: 'accept-route', grant_revision: 1};
  const unaccepted = (await valid.request(acceptanceQuery)).result.thread_acceptance;
  assert.equal(unaccepted.receipt, null, 'Reading a delivery must not create an acceptance command');
  assert.equal(unaccepted.acceptance, null, 'Completed task is not yet user-accepted');
  assert.equal((await valid.request({...accept, decision: {...decision, target: {...target, fingerprint: '0'.repeat(64)}}})).error,
    'not_found', 'Acceptance cannot target a different frozen artifact');
  assert.equal((await valid.request({...accept, decision: {...decision, target: {...target, thread_id: crypto.randomUUID()}}})).error,
    'not_found', 'Acceptance cannot cross threads');
  assert.equal((await valid.request({...accept, grant_revision: 99})).error, 'grant_changed', 'Acceptance requires the current authorization revision');
  const acceptanceReader = await phone();
  await acceptanceReader.request({type: 'authenticate', ...readOnly});
  assert.equal((await acceptanceReader.request({...accept, ...readOnly})).error, 'not_found', 'Another read-only phone cannot accept this phones artifact');
  acceptanceReader.ws.close();
  const waitingAcceptance = (await valid.request(accept)).result.thread_acceptance;
  assert.equal(waitingAcceptance.receipt.state, 'accepted', 'Queued acceptance must not display as completed');
  assert.equal(waitingAcceptance.acceptance, null, 'No acceptance exists before Mac commits it');
  assert.deepEqual((await valid.request({...accept, route_id: 'other-route'})).result.thread_acceptance, waitingAcceptance,
    'Lost acknowledgement retry keeps original operation and route');
  assert.equal((await valid.request({...accept, decision: {...decision, operation_id: task.operation_id}})).error,
    'idempotency_conflict', 'Decision cannot overwrite the original dispatch command');
  assert.equal((await valid.request({...taskQuery, operation_id: decision.operation_id})).error,
    'not_found', 'Acceptance command must not masquerade as an Agent task');
  report('acceptance-submitted'); await lines.next();
  valid.ws.close();
  valid = await phone();
  await valid.request({type: 'authenticate', ...credentials});
  const confirmed = (await valid.request(acceptanceQuery)).result.thread_acceptance;
  assert.deepEqual(confirmed.decision, decision, 'Recovered acceptance belongs to this exact decision');
  assert.equal(confirmed.receipt.state, 'succeeded', 'Lost response is recovered from persistent Mac storage');
  assert.equal(confirmed.acceptance.operation_id, decision.operation_id, 'Acceptance records the independent user operation');
  assert.equal(confirmed.acceptance.fingerprint, savedDelivery.fingerprint, 'Acceptance pins the frozen content');
  assert.equal(confirmed.acceptance.key.turn_id, savedDelivery.outcome.key.turn_id, 'Acceptance cannot follow a later Agent turn');
  assert.ok(confirmed.acceptance.accepted_at_unix > 0, 'Mac must supply a durable acceptance timestamp');
  assert.deepEqual((await valid.request({...accept, route_id: 'after-reconnect'})).result.thread_acceptance, confirmed,
    'Repeated acceptance must reuse the same durable result without another foreground call');
  assert.deepEqual((await valid.request(taskQuery)).result.thread_task.execution, taskResult.execution,
    'User acceptance must not rewrite the immutable execution event or original Agent reply');
  assert.equal((await valid.request({...accept, decision: {...decision, operation_id: crypto.randomUUID()}})).error,
    'delivery_accepted', 'Another operation cannot replace the saved user decision');
  if (native) {
    // Node and Swift are separate clients with separate budget owners but share this Mac connection.
    // Finish Node's stable-task assertions before starting Swift in a fresh real 60s rate window.
    valid.ws.close();
    await new Promise(resolve => setTimeout(resolve, 60_100)); //60s server window +100ms scheduling margin.
    const root = fileURLToPath(new URL('../../', import.meta.url));
    const path = join(directory, 'native.json');
    const silent = createServer(), redirect = createServer(), sink = createServer();
    const silentSockets = new Set();
    const wss = new WebSocketServer({server: silent});
    wss.on('connection', ws => { silentSockets.add(ws); ws.once('close', () => silentSockets.delete(ws)); });
    let leaked = 0;
    sink.on('upgrade', (_req, socket) => { leaked++; socket.destroy(); });
    sink.on('request', (_req, response) => { leaked++; response.writeHead(403).end(); });
    for (const local of [silent, redirect, sink]) { track(local); local.listen(0, '127.0.0.1'); await once(local, 'listening'); }
    redirect.on('upgrade', (_req, socket) => socket.end(`HTTP/1.1 302 Found\r\nLocation: ws://127.0.0.1:${sink.address().port}/collect\r\nContent-Length: 0\r\nConnection: close\r\n\r\n`));
    writeFileSync(path, JSON.stringify({port: Number(new URL(origin).port), machineID: fixture.machine_id,
      projectID: fixture.project_id, task, revisionTask, acceptanceDecision: decision, cloud: {deviceID: claimed.device_id, token},
      mac: {deviceID: credentials.device_id, token: credentials.token},
      silentPort: silent.address().port, redirectPort: redirect.address().port}), {mode: 0o600});
    const child = spawn('swift', ['test', '--disable-sandbox', '--package-path', join(root, 'ios'),
      '--cache-path', join(root, 'target/swift-cache'), '--filter', 'RelayNetworkTests'], {
      cwd: root, env: {...process.env, NORMA_NATIVE_RELAY_FIXTURE: path}, stdio: ['ignore', 2, 2],
    });
    try {
      const [code, signal] = await once(child, 'exit');
      assert.equal(signal, null, 'Swift client must complete without termination');
      assert.equal(code, 0, 'Swift must connect through the production relay to the real Mac service');
      assert.equal(leaked, 0, 'Redirect target must never receive an authenticated upgrade');
      // Close frames complete asynchronously; allow 100ms for the local peer to observe cancellation.
      await new Promise(resolve => setTimeout(resolve, 100));
      assert.equal(silentSockets.size, 0, 'Timeout and cancellation must release waiting sockets');
    } finally {
      for (const ws of silentSockets) ws.terminate();
      wss.close();
      for (const local of [silent, redirect, sink]) {
        await shutdown(local);
      }
    }
    valid = await phone();
    await valid.request({type: 'authenticate', ...credentials});
  }
  if (reviewUI) {
    valid.ws.close(); // UI inspection can exceed the 60s idle timeout; reopen after it ends.
    // Explicit UI fixture: 15min = native build/tests 10min + Computer Use 5min.
    // A caller-owned sentinel ends the session; execution is still the Rust fixture above.
    assert.equal(existsSync(reviewUI), false, 'Use a fresh fixture path, never overwrite credentials');
    writeFileSync(reviewUI, JSON.stringify({port: Number(new URL(origin).port), namespace: crypto.randomUUID(),
      machineID: fixture.machine_id, projectID: fixture.project_id, endpoints: fixture.endpoints,
      message: revisionMessage, task: revisionTask, cloud: {deviceID: claimed.device_id, token},
      mac: {deviceID: credentials.device_id, token: credentials.token}}), {mode: 0o600});
    console.error('Native review UI fixture ready');
    while (!existsSync(reviewUI + '.finished')) await new Promise(resolve => setTimeout(resolve, 500));
    rmSync(reviewUI); rmSync(reviewUI + '.finished');
    valid = await phone();
    assert.equal((await valid.request({type: "authenticate", ...credentials})).result.status, "authorized");
  }
  report('acceptance-readonly'); await lines.next();
  assert.equal((await valid.next()).type, 'close', 'Permission downgrade must end existing authenticated streams');
  valid.ws.close(); valid = await phone();
  await valid.request({type: 'authenticate', ...credentials});
  assert.deepEqual((await valid.request(acceptanceQuery)).result.thread_acceptance, confirmed, 'Original phone may still read its accepted artifact with read-only access');
  assert.equal((await valid.request({type: 'thread_task_terminal', ...credentials, request: terminalScope})).result?.status,
    'thread_task_terminal', 'Original device with current read permission may still view its bound terminal');
  assert.equal((await valid.request({...accept, grant_revision: 2})).type, 'close', 'Original source ownership cannot replace current control permission');
  valid.ws.close();
  report('acceptance-restore'); await lines.next();
  valid = await phone();
  await valid.request({type: 'authenticate', ...credentials});
  // Queue the revocation probe after optional Swift compilation, which can exceed the 30s command deadline.
  assert.equal((await valid.request({...dispatch, grant_revision: 3, task: {...task, operation_id: crypto.randomUUID()}})).result.thread_task.receipt.state,
    'accepted', 'Second explicit operation waits for its own lease');
  report('authenticated'); await lines.next();
  assert.equal((await valid.next()).type, 'close', 'Revoke must close existing relay streams without another phone request');
  valid.ws.close();
  const revoked = await phone();
  assert.equal((await revoked.request({type: 'authenticate', ...credentials})).type, 'close', 'Reconnection cannot restore a revoked grant');
  report('revoked'); await lines.next();
  await new Promise(resolve => setTimeout(resolve, 2200)); // 2s reconnect interval + 200ms scheduling margin.
  revoked.ws.send(JSON.stringify({type: 'open', request_id: crypto.randomUUID(), machine_id: fixture.machine_id}));
  const offline = await revoked.next();
  assert.equal(offline.type, 'offline', 'Stopped MobileServer must not reconnect');
  report('stopped');
  }
} finally {
  for (const ws of sockets) ws.terminate();
  await shutdown(server);
  clearTimeout(deadline);
  if (reviewUI) { rmSync(reviewUI, {force: true}); rmSync(reviewUI + ".finished", {force: true}); }
  rmSync(directory, {recursive: true, force: true});
  process.stdin.destroy();
}
