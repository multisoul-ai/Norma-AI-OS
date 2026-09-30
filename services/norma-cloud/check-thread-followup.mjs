import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';

// Real relay/Mac transport; the Rust foreground is explicitly a test substitute.
export async function checkThreadFollowup(phone, credentials, readOnly, valid, task, source, report, lines, nativeCheck) {
  const message = {...source, operation_id: randomUUID(), message_id: randomUUID(), text: '为什么这样修改首屏？'};
  await valid.request({type: 'thread_message', ...credentials, message});
  const followup = {operation_id: randomUUID(), collaboration_workspace_id: task.collaboration_workspace_id,
    thread_id: task.thread_id, source_operation_id: message.operation_id, source_message_id: message.message_id,
    parent_operation_id: task.operation_id, task_ref: 'test-runtime:7', text: message.text};
  const query = {type: 'thread_task_followup_receipt', ...credentials, followup};
  const submit = {type: 'thread_task_followup', ...credentials, followup, route_id: 'route-a', grant_revision: 1};
  const reader = await phone();
  await reader.request({type: 'authenticate', ...readOnly});
  assert.equal((await reader.request({...submit, ...readOnly})).error, 'not_found', 'Other devices cannot address this original task');
  reader.ws.close();
  const absent = await valid.request(query);
  assert.equal(absent.result?.status, 'thread_followup_receipt', 'Mac must implement the bound follow-up endpoint');
  assert.equal(absent.result.thread_followup, null, 'Read-before-send must not create a queue');
  assert.equal((await valid.request({...submit, followup: {...followup,
    source_operation_id: task.source_operation_id, source_message_id: task.source_message_id, text: source.text}})).error,
    'invalid_request', 'A follow-up needs a new visible source message, not the parent task source');
  assert.equal((await valid.request({...submit, followup: {...followup, thread_id: randomUUID()}})).error,
    'not_found', 'A follow-up cannot cross threads');
  assert.equal((await valid.request({...submit, followup: {...followup, task_ref: 'other-runtime:7'}})).error,
    'not_found', 'Same numeric task ID cannot select a later task');
  assert.equal((await valid.request({...submit, followup: {...followup, source_message_id: randomUUID()}})).error,
    'source_message_missing', 'An undelivered source cannot enqueue input');
  assert.equal((await valid.request({...submit, followup: {...followup, text: '替换用户正文'}})).error,
    'source_message_changed', 'Queued text must match the visible delivered message');
  assert.equal((await valid.request({...submit, grant_revision: 99})).error, 'grant_changed', 'Stale control grants cannot enqueue input');
  assert.equal((await valid.request({...submit, route_id: 'new-route'})).error, 'route_expired', 'New input cannot silently switch the original route');
  const queued = (await valid.request(submit)).result.thread_followup;
  assert.equal(queued.original_key.turn_id, 'original-turn', 'The immutable original turn comes from the Mac');
  assert.equal(queued.receipt.result.status, 'queued', 'Mac queue acknowledgement is not Provider reception');
  assert.equal(queued.queued_message.node_id, 2, 'Use the original task node');
  assert.equal(queued.queued_message.session_id, 'review-session', 'Use the original Provider session');
  assert.equal(queued.outcome, null, 'A completed parent cannot supply a follow-up reply');
  assert.equal(queued.delivery, null, 'A completed parent cannot supply follow-up artifacts');
  const retry = (await valid.request({...submit, route_id: 'retry-route'})).result.thread_followup;
  assert.deepEqual(retry.receipt, queued.receipt, 'Retry keeps the original durable queue acknowledgement');
  assert.equal(retry.queued_message.route_id, 'route-a', 'Retry never retargets execution');
  assert.equal((await valid.request({...submit, followup: {...followup, text: '覆盖同操作'}})).error,
    'idempotency_conflict', 'The same operation cannot change its text');
  report('followup-queued'); await lines.next();
  let result;
  //10 × 100ms =1s: two production queue polls (2Hz) plus local write margin.
  for (let i = 0; i < 10; i++) {
    result = (await valid.request(query)).result.thread_followup;
    if (result.queued_message.state === 'sent') break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.equal(result.queued_message.state, 'sent', 'Only Provider acknowledgement marks follow-up sent');
  assert.equal(result.queued_message.result.turn_id, 'followup-turn', 'Bind the newly acknowledged Provider turn');
  assert.equal(result.outcome.key.turn_id, 'followup-turn', 'Do not reuse the parent final reply');
  assert.equal(result.outcome.summary, '追问回复：首屏需要保留当前目标。', 'Return this turn’s persisted answer');
  assert.equal(result.delivery.outcome.key.turn_id, 'followup-turn', 'Artifacts must belong to the follow-up turn');
  valid.ws.close(); valid = await phone();
  const authenticated = await valid.request({type: 'authenticate', ...credentials});
  assert.ok(authenticated.result.capabilities.includes('thread_task_followup_v1'), 'Follow-up support must be negotiated');
  assert.deepEqual((await valid.request(query)).result.thread_followup, result, 'Reconnect returns the same reply without re-execution');
  if (nativeCheck) await nativeCheck(followup, source, message);
  const nextMessage = {...message, operation_id: randomUUID(), message_id: randomUUID(), text: '再解释一下交接状态。'};
  await valid.request({type: 'thread_message', ...credentials, message: nextMessage});
  const nextFollowup = {...followup, operation_id: randomUUID(), source_operation_id: nextMessage.operation_id,
    source_message_id: nextMessage.message_id, text: nextMessage.text};
  const next = await valid.request({...submit, followup: nextFollowup});
  assert.equal(next.result?.thread_followup?.queued_message.state, 'queued', 'A second follow-up must reuse the original task after its previous turn');
  report('followup-second-queued'); await lines.next();
  let second;
  //10×100ms =1s，覆盖原2Hz队列确认和本机数据库写入。
  for (let i = 0; i < 10; i++) {
    second = (await valid.request({...query, followup: nextFollowup})).result.thread_followup;
    if (second.queued_message.state === 'sent') break;
    await new Promise(resolve => setTimeout(resolve, 100));
  }
  assert.deepEqual(second.original_key, queued.original_key, 'A continuation cannot replace the original turn with its predecessor');
  assert.equal(second.outcome?.summary, '第二次回复：交接状态保持可见。', 'Second reply must not be swallowed by the first result');
  assert.equal(second.delivery?.outcome.key.turn_id, 'followup-second-turn', 'Second artifacts belong to its own confirmed turn');
  assert.deepEqual((await valid.request(query)).result.thread_followup, result, 'The first follow-up remains unchanged');
  report('followup-revoke'); await lines.next();
  assert.equal((await valid.next()).type, 'close', 'Revoke must close the active connection');
  valid.ws.close();
  const revoked = await phone();
  assert.equal((await revoked.request({type: 'authenticate', ...credentials})).type, 'close', 'Revoked phone cannot resume follow-up control');
  revoked.ws.close(); report('followup-finished');
}
