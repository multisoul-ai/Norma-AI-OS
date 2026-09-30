import assert from 'node:assert/strict';
import {once} from 'node:events';
import {randomUUID} from 'node:crypto';
import {mkdtempSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {makeGateway} from './server.mjs';

/// 受邀手机通过正式HTTP获得具名候选，云端不执行Mac任务。
/// 数据：1台受邀设备、2个同名不同项目Agent；1轮执行+1轮讨论=2次供应商调用。
/// 执行：匿名拒绝→兑换邀请→非法上下文拒绝→生成候选→重放拒绝→讨论无派工。
/// 预期：四项来源身份准确，目标精确来自当前成员；回复无密钥、无执行状态，拒绝不调用模型。
const directory = mkdtempSync(join(tmpdir(), 'norma-thread-plan-'));
const admin = 'thread-plan-test-admin-more-than-24-characters';
const key = 'test-provider-key-stays-at-server';
const workspace = randomUUID(), thread = randomUUID(), message = randomUUID();
const agents = [{id: 'mac-a:ios:2', name: 'Alice', project: 'iOS', profile: 'Codex'},
  {id: 'mac-a:api:3', name: 'Alice', project: 'API', profile: 'Codex'}];
let calls = 0;
const server = makeGateway({env: {COMPANION_ACCESS_TOKEN: admin, DOUBAO_SPEECH_API_KEY: key,
  NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json'), NORMA_TEXT_DAILY_LIMIT: '2'},
fetchImpl: async (url, options) => {
  calls++;
  assert.equal(url, 'https://ark.cn-beijing.volces.com/api/plan/v3/chat/completions', '复用服务端已配置的豆包文字供应商');
  assert.equal(options.headers.Authorization, `Bearer ${key}`, '供应商Key仅送上游');
  const input = JSON.parse(options.body);
  assert.equal(input.stream, false, '候选完整验证后才交给手机');
  assert.equal(input.response_format.type, 'json_object', '请求结构化候选，不从自由文本猜工具');
  assert.match(input.messages[0].content, /讨论.*不.*派工/, '讨论与执行意图必须区分');
  assert.match(input.messages[0].content, /不能声称.*已执行/, '候选不得冒充执行回执');
  const payload = JSON.parse(input.messages[1].content);
  assert.deepEqual(payload.agents, agents, '同名Agent必须携带独立目标身份与项目');
  const plan = payload.message.includes('讨论') ? {reply: '先梳理首屏目标。', assignments: []}
    : {reply: '由 iOS 项目的 Alice 检查首屏。', assignments: [{target_id: agents[0].id, task: '检查首屏导航'}]};
  return Response.json({choices: [{finish_reason: 'stop', message: {content: JSON.stringify(plan)}}]});
}});
try {
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const base = `http://127.0.0.1:${server.address().port}`;
  const post = (route, body, token) => fetch(base + route, {method: 'POST', headers: {
    'Content-Type': 'application/json', ...(token ? {Authorization: `Bearer ${token}`} : {})}, body: JSON.stringify(body)});
  const input = {turn_id: '11111111-1111-4111-a111-111111111111', workspace_id: workspace, thread_id: thread, message_id: message,
    message: '@Norma 请检查首屏', context: JSON.stringify({workspaceID: workspace, threadID: thread, title: '首屏'}), agents};
  assert.equal((await post('/v1/threads/plan', input)).status, 401, '未授权手机不能请求编排');
  const invitation = await (await post('/v1/admin/devices/invite', {}, admin)).json();
  const {token} = await (await post('/v1/devices/claim', {code: invitation.code, name: '测试iPhone'})).json();
  assert.equal((await post('/v1/threads/plan', {...input, context: JSON.stringify({workspaceID: workspace, threadID: randomUUID()})}, token)).status,
    400, '其他线程快照不能混入当前编排');
  assert.equal(calls, 0, '身份不一致在调用模型之前拒绝');
  const response = await post('/v1/threads/plan', input, token);
  assert.equal(response.status, 200, '受邀设备应获得结构化候选');
  const result = await response.json();
  assert.deepEqual(result, {turn_id: input.turn_id, workspace_id: workspace, thread_id: thread, message_id: message,
    reply: '由 iOS 项目的 Alice 检查首屏。', assignments: [{target_id: agents[0].id, task: '检查首屏导航'}]}, '来源和精确目标必须完整回传');
  assert.equal(JSON.stringify(result).includes(key), false, '供应商Key不得返回手机');
  assert.equal('execution' in result, false, '云端推理不能伪造Mac执行状态');
  assert.equal((await post('/v1/threads/plan', {...input, turn_id: input.turn_id.toUpperCase()}, token)).status,
    409, 'UUID大小写不同仍是同一回合，不得重复生成候选');
  assert.equal((await post('/v1/threads/plan', input, token)).status, 409, '同回合重复请求不得再次推理');
  assert.equal((await post('/v1/text/turn', {turn_id: input.turn_id.toUpperCase(), messages: [{role: 'user', content: '你好'}]}, token)).status,
    409, '相同UUID不能通过换接口或改变大小写绕过去重');
  assert.equal(calls, 1, '执行候选和重放合计只调用一次供应商');
  const discussion = await post('/v1/threads/plan', {...input, turn_id: randomUUID(), message: '@Norma 先讨论，不开工'}, token);
  assert.deepEqual((await discussion.json()).assignments, [], '只讨论的回复不能产生任务');
  assert.equal((await post('/v1/text/turn', {turn_id: randomUUID(), messages: [{role: 'user', content: '你好'}]}, token)).status,
    429, '编排与普通文字必须共享每日2轮额度，不新增绕过额度的入口');
  assert.equal(calls, 2, '额度用尽不能产生第三次调用');
  console.log('PASS: thread plan HTTP, source scope, target identity, discussion, dedup and shared quota (test provider).');
} finally {
  server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
  rmSync(directory, {recursive: true, force: true});
}

/// 场景：无效模型候选不得到达手机；生成期间撤权立即生效。
/// 数据构造：1台设备、2个同名成员；候选上限32 KiB、单任务8 KiB；
///   超长任务 = 8192 + 1 ASCII字节；快照与任务合计也不能超过Mac既有32 KiB预算。
/// 执行过程：拒绝越界输入→逐种拒绝非法候选→合法回合证明并发锁已释放→挂起生成并撤权。
/// 预期结果：拒绝响应只有明确错误，无部分任务；拒绝输入不请求供应商，撤权后不返回候选。
async function checkInvalidPlansAndRevocation() {
  const directory = mkdtempSync(join(tmpdir(), 'norma-thread-plan-invalid-'));
  const input = {turn_id: randomUUID(), workspace_id: workspace, thread_id: thread, message_id: message,
    message: '@Norma 请检查首屏', context: JSON.stringify({workspaceID: workspace, threadID: thread}), agents};
  const valid = {reply: '由 iOS Alice 检查首屏。', assignments: [{target_id: agents[0].id, task: '检查导航'}]};
  const envelope = (plan, finish = 'stop') => ({choices: [{finish_reason: finish, message: {content: JSON.stringify(plan)}}]});
  let output = envelope(valid), calls = 0, started, release;
  const server = makeGateway({env: {COMPANION_ACCESS_TOKEN: admin, DOUBAO_SPEECH_API_KEY: key,
    NORMA_CLOUD_DEVICE_STATE: join(directory, 'devices.json')}, fetchImpl: async () => {
    calls++;
    if (started) { started(); await new Promise(resolve => { release = resolve; }); }
    return Response.json(output);
  }});
  try {
    server.listen(0, '127.0.0.1'); await once(server, 'listening');
    const base = `http://127.0.0.1:${server.address().port}`;
    const post = (route, body, token) => fetch(base + route, {method: 'POST', headers: {
      'Content-Type': 'application/json', ...(token ? {Authorization: `Bearer ${token}`} : {})}, body: JSON.stringify(body)});
    const {code} = await (await post('/v1/admin/devices/invite', {}, admin)).json();
    const {device_id, token} = await (await post('/v1/devices/claim', {code, name: '候选拒绝验收'})).json();
    const plan = patch => post('/v1/threads/plan', {...input, turn_id: randomUUID(), ...patch}, token);
    assert.equal((await plan({execute: true})).status, 400, '手机不能夹带执行字段');
    assert.equal((await plan({agents: [agents[0], agents[0]]})).status, 400, '重复目标身份必须在推理前拒绝');
    assert.equal((await plan({context: JSON.stringify({workspaceID: workspace, threadID: thread, addressedAgentIDs: ['unknown']})})).status,
      400, '未知的明确选择必须在推理前拒绝');
    assert.equal((await plan({context: JSON.stringify({workspaceID: workspace, threadID: thread, addressedAgentIDs: [agents[0].id, agents[0].id]})})).status,
      400, '重复的明确选择不能进入模型');
    assert.equal((await plan({context: JSON.stringify({workspaceID: workspace, threadID: thread, addressedAgentIDs: null})})).status,
      400, '存在但不是数组的明确选择不能当作未选择');
    // 32769 = 快照32 KiB上限 + 1；总请求仍低于64 KiB，验证的是快照边界。
    assert.equal((await plan({context: 'x'.repeat(32769)})).status, 400, '超大快照必须拒绝');
    // 65536字节正文再加JSON字段，超过整个HTTP请求的64 KiB上限。
    assert.equal((await plan({message: 'x'.repeat(65536)})).status, 413, '超大请求必须明确拒绝');
    assert.equal(calls, 0, '非法输入不得消耗模型调用');
    const rejected = async (label, candidate, patch = {}) => {
      output = candidate;
      const response = await plan(patch);
      assert.equal(response.status, 502, label + '必须拒绝');
      assert.deepEqual(await response.json(), {error: 'provider_invalid_plan'}, label + '不得返回部分任务或来源伪造');
    };
    await rejected('未知成员', envelope({...valid, assignments: [{target_id: 'mac-other:ios:2', task: '检查'}]}));
    await rejected('重复目标', envelope({...valid, assignments: [valid.assignments[0], valid.assignments[0]]}));
    await rejected('伪造来源', envelope({...valid, thread_id: randomUUID()}));
    await rejected('额外执行字段', envelope({...valid, assignments: [{...valid.assignments[0], execution: 'done'}]}));
    await rejected('截断结束', envelope(valid, 'length'));
    await rejected('半句JSON', {choices: [{finish_reason: 'stop', message: {content: '{"reply":"未完'}}]});
    await rejected('超长任务', envelope({...valid, assignments: [{target_id: agents[0].id, task: 'x'.repeat(8193)}]}));
    // 8192任务 + 25000背景 + JSON身份元数据 > Mac的32768字节总预算；各字段单独仍合法。
    await rejected('任务与快照总量越界', envelope({...valid, assignments: [{target_id: agents[0].id, task: 'x'.repeat(8192)}]}),
      {context: JSON.stringify({workspaceID: workspace, threadID: thread, summary: 'x'.repeat(25000)})});
    await rejected('改派给未选中的同名成员', envelope(valid),
      {context: JSON.stringify({workspaceID: workspace, threadID: thread, addressedAgentIDs: [agents[1].id]})});
    const bob = {id: 'mac-a:ios:4', name: 'Bob', project: 'iOS', profile: 'Codex'};
    output = envelope({...valid, assignments: [...valid.assignments, {target_id: bob.id, task: '复核结果'}]});
    const recovered = await plan({agents: [...agents, bob],
      context: JSON.stringify({workspaceID: workspace, threadID: thread, addressedAgentIDs: [agents[0].id]})});
    assert.equal(recovered.status, 200, '供应商候选无效不能遗留并发锁，下一有效回合应成功');
    assert.deepEqual((await recovered.json()).assignments.map(item => item.target_id), [agents[0].id, bob.id],
      '保留明确选择的iOS Alice，同时允许Norma安排不同名的Bob协助');
    output = envelope(valid);
    const arrived = new Promise(resolve => { started = resolve; });
    const pending = plan();
    await arrived;
    const busy = await post('/v1/text/turn', {turn_id: randomUUID(), messages: [{role: 'user', content: '你好'}]}, token);
    assert.equal(busy.status, 429, '编排占用期间普通文字也必须遵守同设备单并发');
    assert.equal((await busy.json()).error, 'busy', '必须是共享并发限制，不能误命中其他拒绝');
    assert.equal((await post('/v1/admin/devices/revoke', {device_id}, admin)).status, 200, '等待模型期间可撤销设备');
    release();
    const revoked = await pending;
    assert.equal(revoked.status, 403, '模型返回前撤销设备必须丢弃整个候选');
    assert.deepEqual(await revoked.json(), {error: 'revoked'}, '撤权响应不能包含任务或模型正文');
    assert.equal((await plan()).status, 403, '撤权后新回合也必须拒绝');
    console.log('PASS: strict candidate rejection, shared concurrency, recovery and in-flight revocation (test provider).');
  } finally {
    release?.(); server.closeAllConnections(); await new Promise(resolve => server.close(resolve));
    rmSync(directory, {recursive: true, force: true});
  }
}

await checkInvalidPlansAndRevocation();
