#!/usr/bin/env node
// Live G1 check: two independent clients share one disposable Codex thread.
// Data: two connections, one exact temporary write, deny first then stale allow.
// Flow: start loopback server -> attach both -> observe the same approval ->
// decline from client A -> resolve on both -> send late allow from client B.
// Expected: request identities agree, file is absent, and both history pages
// belong to this thread. No existing user thread is resumed. An optional real TUI
// can attach before the turn; --wait-for-tui pauses until stdin receives a line.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const net = require('node:net');
const readline = require('node:readline');
const {spawn, execFileSync} = require('node:child_process');

class Client {
  constructor(url) {
    this.events = [];
    this.id = 0;
    this.pending = new Map();
    this.waiters = new Set();
    this.socket = new WebSocket(url);
    this.opened = new Promise((resolve, reject) => {
      this.socket.addEventListener('open', resolve, {once:true});
      this.socket.addEventListener('error', () => reject(new Error('WebSocket connection failed')), {once:true});
    });
    this.socket.addEventListener('message', ({data}) => {
      const event = JSON.parse(data);
      this.events.push(event);
      if (!event.method && this.pending.has(event.id)) {
        this.pending.get(event.id)(event);
        this.pending.delete(event.id);
      }
      for (const wake of this.waiters) wake();
    });
  }
  send(event) { this.socket.send(JSON.stringify(event)); }
  async request(method, params) {
    const id = ++this.id;
    const result = new Promise(resolve => this.pending.set(id, resolve));
    this.send({id, method, params});
    const event = await deadline(result, 30000, method);
    assert.equal(event.error, undefined, `${method} must succeed`);
    return event.result;
  }
  async wait(predicate, label) {
    const find = () => this.events.find(predicate);
    if (find()) return find();
    let wake;
    const ready = new Promise(resolve => {
      wake = () => { const event = find(); if (event) resolve(event); };
      this.waiters.add(wake);
    });
    try { return await deadline(ready, 180000, label); }
    finally { this.waiters.delete(wake); }
  }
  async initialize(name) {
    await this.opened;
    await this.request('initialize', {clientInfo:{name, version:'0.1.0'}, capabilities:{experimentalApi:true}});
    this.send({method:'initialized', params:{}});
  }
}

async function deadline(promise, ms, label) {
  // Local requests: 30s for startup; real model turns: 180s for startup + output.
  let timer;
  try { return await Promise.race([promise, new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`Timeout: ${label}`)), ms);
  })]); } finally { clearTimeout(timer); }
}

async function availablePort() {
  const listener = net.createServer();
  await new Promise(resolve => listener.listen(0, '127.0.0.1', resolve));
  const port = listener.address().port;
  await new Promise(resolve => listener.close(resolve));
  return port;
}

async function main() {
  assert(process.argv.includes('--isolate-codex-hooks'), 'Explicit authorization is required to isolate test hooks');
  const output = process.argv[process.argv.indexOf('--output') + 1];
  assert(process.argv.includes('--output') && output, 'An evidence output path is required');
  const workspace = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'norma-codex-multi-')));
  const url = `ws://127.0.0.1:${await availablePort()}`;
  const server = spawn('codex', ['--disable','hooks','app-server','--listen',url],
    {cwd:workspace, stdio:['ignore','ignore','pipe'], detached:true});
  server.stderr.on('data', () => {}); // Never emit configuration or credentials.
  let first, second, threadId;
  let result = {provider:'codex', version:execFileSync('codex',['--version'],{encoding:'utf8'}).trim(),
    case:'multiclient-approval', transport:'two independent loopback WebSocket clients', status:'failed',
    external_hooks:'isolated_for_probe', real_mobile_client:false};
  try {
    // The server normally starts within 1s; bounded readiness retries cover 15s.
    for (let attempt = 0; attempt < 60; attempt++) {
      try { if ((await fetch(url.replace('ws:', 'http:')+'/readyz')).ok) break; } catch {}
      assert(server.exitCode === null, 'The disposable server must remain alive');
      await new Promise(resolve => setTimeout(resolve,250));
    }
    first = new Client(url);
    await first.initialize('norma_probe_desktop');
    const start = await first.request('thread/start', {cwd:workspace, sandbox:'read-only',
      approvalPolicy:'untrusted', approvalsReviewer:'user'});
    threadId = start.thread.id;
    result.model = start.model;
    // Codex persists the rollout after a first turn; an empty new thread cannot
    // yet be resumed by another connection (verified -32600 from the real CLI).
    const hello = await first.request('turn/start',{threadId,input:[{type:'text',
      text:'Protocol check. Do not use any tools. Reply with exactly NORMA_MULTI_READY.'}]});
    const ready = await first.wait(event => event.method === 'turn/completed'
      && event.params.threadId === threadId && event.params.turn.id === hello.turn.id,'initial turn');
    assert.equal(ready.params.turn.status,'completed','Initial no-tool turn must complete');
    second = new Client(url);
    await second.initialize('norma_probe_mobile');
    const attached = await second.request('thread/resume', {threadId});
    assert.equal(attached.thread.id, threadId, 'Second connection must attach to the same live thread');
    result.thread_id = threadId;
    console.log(JSON.stringify({stage:'attached',url,threadId,workspace}));
    if (process.argv.includes('--wait-for-tui')) {
      const input = readline.createInterface({input:process.stdin, output:process.stdout});
      await deadline(new Promise(resolve => input.once('line', resolve)), 180000, 'TUI attachment');
      input.close();
    }
    const target = path.join(workspace,'permission-check.txt');
    const command = `printf NORMA_APPROVAL_OK > ${target}`;
    const turn = await first.request('turn/start', {threadId, input:[{type:'text',text:
      `Protocol check in a disposable directory. Use exec_command exactly once: ${command}\n`+
      'Request approval. Do not run other commands. If denied, do not retry. Reply briefly and stop.'}]});
    const turnId = turn.turn.id;
    const isApproval = event => event.method === 'item/commandExecution/requestApproval'
      && event.params.threadId === threadId && event.params.turnId === turnId;
    const [a,b] = await Promise.all([first.wait(isApproval,'first approval'),second.wait(isApproval,'second approval')]);
    assert.equal(a.id,b.id,'Both clients must observe the same request ID');
    assert.equal(a.params.itemId,b.params.itemId,'Both clients must observe the same command item');
    // Only send an allow if the provider returns this exact safe command wrapper.
    assert([command, `/bin/zsh -lc '${command}'`, `/bin/bash -lc '${command}'`].includes(a.params.command),
      'Only the exact temporary write may be decided');
    assert(!fs.existsSync(target), 'File must not exist before permission');
    first.send({id:a.id,result:{decision:'decline'}});
    const isResolved = event => event.method === 'serverRequest/resolved'
      && event.params.threadId === threadId && event.params.requestId === a.id;
    await Promise.all([first.wait(isResolved,'first resolution'),second.wait(isResolved,'second resolution')]);
    second.send({id:b.id,result:{decision:'accept'}});
    const completion = await first.wait(event => event.method === 'turn/completed'
      && event.params.threadId === threadId && event.params.turn.id === turnId,'turn completion');
    assert.equal(completion.params.turn.status,'completed','Declined turn must complete');
    assert(!fs.existsSync(target),'A late conflicting approval must never execute the denied command');
    const history = await second.request('thread/read',{threadId,includeTurns:true});
    assert.equal(history.thread.id,threadId,'Second client history must keep the same identity');
    const commands = first.events.filter(event => event.method === 'item/started'
      && event.params.threadId === threadId && event.params.turnId === turnId
      && event.params.item.type === 'commandExecution');
    assert.equal(commands.length,1,'Only one requested command may appear in live events');
    assert.equal(commands[0].params.item.id,a.params.itemId,'The command must match the approved item identity');
    // Provider history can omit denied commands. Record that limit explicitly;
    // Norma will need its own request/outcome audit, not inferred transcript text.
    result.history_stores_decided_command = history.thread.turns.flatMap(turn => turn.items)
      .some(item => item.type === 'commandExecution' && item.id === a.params.itemId);
    const page = await second.request('thread/turns/list',{threadId,limit:1,itemsView:'full'});
    assert.equal(page.data.length,1,'First page must contain exactly one turn');
    assert.equal(page.data[0].id,turnId,'First page must contain our latest turn');
    assert(page.nextCursor,'Two turns with page size one must produce an older-page cursor');
    const older = await second.request('thread/turns/list',{threadId,limit:1,itemsView:'full',cursor:page.nextCursor});
    assert.equal(older.data[0].id,hello.turn.id,'Second page must contain the initial turn');
    assert.equal(older.nextCursor,null,'Only two turns exist; no third page may appear');
    assert.notEqual(older.data[0].id,turnId,'Pagination must not repeat the newest turn');
    Object.assign(result,{status:'passed',turn_id:turnId,approval_request_id:a.id,
      both_clients_observed_resolution:true,conflicting_late_allow_ignored:true,
      two_page_history:'passed',
      terminal_ui_attachment:process.argv.includes('--wait-for-tui') ? 'manual_observation_required' : 'not_tested'});
  } catch (error) {
    result.error = error.message;
    result.event_types = second?.events.slice(-30).map(event => event.method || 'response');
  } finally {
    if (first && threadId) {
      try { await first.request('thread/archive',{threadId}); } catch {}
    }
    first?.socket.close(); second?.socket.close();
    try { process.kill(-server.pid,'SIGTERM'); } catch {}
    await deadline(new Promise(resolve => server.exitCode !== null ? resolve() : server.once('exit',resolve)),3000,'server exit')
      .catch(() => { try {process.kill(-server.pid,'SIGKILL');} catch {} });
    fs.writeFileSync(output,JSON.stringify(result,null,2)+'\n');
    fs.rmSync(workspace,{recursive:true,force:true});
  }
  console.log(JSON.stringify(result));
  process.exit(result.status === 'passed' ? 0 : 1);
}
main().catch(error => {console.error(error.message);process.exit(1);});
