import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';

// Runs inside the existing production-relay probe. Foreground terminal bytes are explicit fixtures.
export async function checkThreadTerminal(phone, credentials, task, project, readOnly) {
  const peer = await phone();
  const auth = await peer.request({type: 'authenticate', ...credentials});
  assert.ok(auth.result.capabilities.includes('thread_task_terminal_v1'), 'Bound terminal read must be negotiated');
  assert.ok(auth.result.capabilities.includes('thread_task_terminal_control_v1'), 'Task-bound takeover must be negotiated separately');
  const scope = {collaboration_workspace_id: task.collaboration_workspace_id, thread_id: task.thread_id,
    operation_id: task.operation_id, task_ref: 'test-runtime:7', before: null, revision: null};
  const read = request => peer.request({type: 'thread_task_terminal', ...credentials, request});
  const first = await read(scope);
  assert.equal(first.result?.status, 'thread_task_terminal', 'Original task must read its exact terminal');
  const terminal = first.result.thread_terminal;
  assert.deepEqual(terminal.request, scope, 'Task scope must be echoed intact');
  assert.equal(terminal.scope.workspace_id, project, 'Project must come from the persisted command');
  assert.equal(terminal.scope.route_id, 'route-a', 'A new snapshot route cannot replace the original route');
  assert.equal(terminal.scope.node_id, 2, 'The reused numeric task ID on node 3 must not be read');
  assert.equal(terminal.key.turn_id, 'original-turn', 'Bind the Provider-confirmed turn, not just its session');
  assert.equal(terminal.page.rows.at(-1), 'fixture-terminal-199', 'Return the original bounded page');
  assert.equal(terminal.page.input_ready, false, 'Read capability is not takeover authorization');
  const older = await read({...scope, before: terminal.page.start_line, revision: terminal.page.revision});
  assert.equal(older.result.thread_terminal.page.rows[0], 'fixture-terminal-0', 'History retains the same task binding');
  assert.equal((await read({...scope, before: 80})).error, 'invalid_request', 'History requires its revision');
  assert.equal((await read({...scope, before: 199, revision: 9})).error, 'result_too_large',
    'An oversized terminal page must fail explicitly, never lose text by truncation');
  assert.equal((await read({...scope, task_ref: 'other-runtime:7'})).error, 'not_found', 'Full task_ref is authoritative');
  assert.equal((await read({...scope, thread_id: randomUUID()})).error, 'not_found', 'Another thread cannot read the terminal');
  const reader = await phone();
  await reader.request({type: 'authenticate', ...readOnly});
  assert.equal((await reader.request({type: 'thread_task_terminal', ...readOnly, request: scope})).error, 'not_found',
    'Project permission alone cannot borrow another paired devices task');
  reader.ws.close();
  assert.equal((await peer.request({type: 'terminal', ...credentials, request: terminal.scope})).type, 'close',
    'Generic terminal RPC must stay unavailable through relay');
  peer.ws.close();
  return scope;
}
