import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';

// Production encrypted transport and persistent receipts; the Rust foreground is an explicit fixture.
export async function checkThreadTerminalControl(phone, credentials, task, report, lines, nativeCheck) {
  const scope = {collaboration_workspace_id: task.collaboration_workspace_id, thread_id: task.thread_id,
    operation_id: task.operation_id, task_ref: 'test-runtime:7', before: null, revision: null};
  const open = async () => {
    const peer = await phone();
    assert.ok((await peer.request({type: 'authenticate', ...credentials})).result.capabilities
      .includes('thread_task_terminal_control_v1'), 'Control requires a distinct negotiated capability');
    return peer;
  };
  const rpc = (peer, operation, request = scope, lookupOnly = false) => peer.request({type: 'thread_task_terminal_control',
    ...credentials, request, operation, lookup_only: lookupOnly});
  const finished = async (peer, operation) => {
    // 25 ×20ms =500ms for the existing asynchronous receipt writer; no extra command is created.
    for (let count = 0; count < 25; count++) {
      const reply = await rpc(peer, operation);
      assert.ok(reply.result?.receipt, `Expected durable receipt: ${JSON.stringify(reply)}`);
      if (reply.result.receipt.state !== 'accepted') return reply.result.receipt;
      await new Promise(resolve => setTimeout(resolve, 20));
    }
    assert.fail('Control receipt must finish within the fixture budget');
  };
  const first = await open();
  const claim = {type: 'claim', operation_id: randomUUID()};
  // 恢复查询没有落到Mac的Claim，必须不执行；之后同ID显式提交才取得输入权。
  assert.equal((await rpc(first, claim, scope, true)).error, 'not_found', 'Lookup must not submit a missing claim');
  const read = {type: 'read', claim_operation_id: claim.operation_id};
  assert.equal((await rpc(first, read)).error, 'terminal_control_expired', 'Read cannot acquire');
  const claimed = await finished(first, claim);
  assert.equal(claimed.result.status, 'controlled', 'Claim must reach the foreground');
  assert.deepEqual((await rpc(first, claim, scope, true)).result.receipt, claimed, 'Lookup returns the original receipt');
  assert.equal((await rpc(first, read, scope, true)).error, 'invalid_request', 'Lookup cannot renew a lease');
  const page = (await rpc(first, read)).result.thread_terminal.page;
  assert.equal(page.input_ready, true, 'Only the actual owner can type');
  const input = {type: 'input', operation_id: randomUUID(), claim_operation_id: claim.operation_id,
    expected: page.card, screen_signature: page.screen_signature, input_sequence: page.input_sequence,
    input: {type: 'text', text: 'draft'}};
  const written = await finished(first, input);
  assert.equal(written.result.input_count, 1, 'Exactly one write reaches the foreground fixture');
  assert.equal((await rpc(first, {...input, input: {type: 'enter'}}, scope, true)).error, 'idempotency_conflict', 'Lookup must compare the original input');
  assert.deepEqual(await finished(first, input), written, 'Exact duplicate must return the same receipt');
  assert.equal((await rpc(first, {...input, input: {type: 'enter'}})).error, 'idempotency_conflict',
    'Changed payload cannot reuse an operation');
  assert.equal((await rpc(first, read, {...scope, thread_id: randomUUID()})).error, 'not_found',
    'Ownership cannot cross thread boundaries');
  let second = await open();
  assert.deepEqual(await finished(second, claim), claimed, 'Reconnection can recover the old claim receipt');
  assert.equal((await rpc(second, read)).error, 'terminal_control_expired', 'Old success is not new ownership');
  const competition = {type: 'claim', operation_id: randomUUID()};
  assert.equal((await finished(second, competition)).error, 'terminal_controlled', 'Another connection cannot steal');
  first.ws.close();
  //100ms gives the production relay one event-loop turn to observe the Close frame.
  await new Promise(resolve => setTimeout(resolve, 100));
  const newClaim = {type: 'claim', operation_id: randomUUID()};
  assert.equal((await finished(second, newClaim)).result.status, 'controlled', 'Disconnect releases the prior owner');
  assert.deepEqual(await finished(second, input), written, 'Lost input receipt is queryable without replay');
  assert.equal((await rpc(second, {...input, operation_id: randomUUID()})).error, 'terminal_control_expired',
    'A fresh input cannot use the previous connection claim');
  const release = {type: 'release', operation_id: randomUUID(), claim_operation_id: newClaim.operation_id};
  assert.equal((await finished(second, release)).result.status, 'released', 'Return reaches the foreground');
  assert.equal((await rpc(second, {type: 'read', claim_operation_id: newClaim.operation_id})).error,
    'terminal_control_expired', 'Returned ownership is not renewable');
  if (nativeCheck) {
    // The idle channel can expire during the separate client's60s rate-window wait.
    second.ws.close();
    await nativeCheck();
    second = await open();
  }
  const queuedPeer = await open();
  const queuedClaim = {type: 'claim', operation_id: 'queued-claim'};
  assert.equal((await rpc(queuedPeer, queuedClaim)).result.receipt.state, 'accepted', 'Claim is pending foreground execution');
  queuedPeer.ws.close();
  await new Promise(resolve => setTimeout(resolve, 100));
  // A request on a surviving peer puts the close event before the following Rust observation.
  assert.equal((await rpc(second, {type: 'read', claim_operation_id: 'queued-claim'})).error, 'terminal_control_expired');
  report('terminal-queued-closed'); await lines.next();
  assert.equal((await finished(second, queuedClaim)).error, 'terminal_control_expired', 'Late claim cannot run after connection close');
  const finalClaim = {type: 'claim', operation_id: randomUUID()};
  assert.equal((await finished(second, finalClaim)).result.status, 'controlled', 'Explicit new claim may acquire');
  assert.equal((await finished(second, release)).result.status, 'released', 'Old return receipt remains readable');
  assert.equal((await rpc(second, {type: 'read', claim_operation_id: finalClaim.operation_id})).result.thread_terminal.page.input_ready,
    true, 'Replaying an old return cannot release the new owner');
  // The fixture holds this foreground read until Rust revokes the device, exercising a cancelled blocking RPC.
  const waiting = rpc(second, {type: 'read', claim_operation_id: finalClaim.operation_id},
    {...scope, before: 77, revision: 9});
  report('terminal-read-pending'); await lines.next();
  assert.equal((await waiting).type, 'close', 'Permission change must cancel the in-flight encrypted channel');
  second.ws.close();
  const denied = await phone();
  assert.equal((await denied.request({type: 'authenticate', ...credentials})).result.status, 'authorized', 'Read permission still allows authentication');
  assert.equal((await rpc(denied, {type: 'claim', operation_id: randomUUID()})).type, 'close', 'Read permission cannot acquire input');
  denied.ws.close();
  report('terminal-revoked'); await lines.next();
  const restored = await open();
  assert.equal((await rpc(restored, {type: 'read', claim_operation_id: finalClaim.operation_id})).error,
    'terminal_control_expired', 'Permission restoration cannot restore old ownership');
  assert.equal((await finished(restored, {type: 'claim', operation_id: randomUUID()})).result.status,
    'controlled', 'New explicit claim succeeds under restored authorization');
  report('terminal-stop-pending'); await lines.next();
  assert.equal((await restored.next()).type, 'close', 'Shutdown disconnects the current owner');
  restored.ws.close();
  report('terminal-stopped');
}
