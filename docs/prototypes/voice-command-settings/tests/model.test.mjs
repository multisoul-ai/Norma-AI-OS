import test from 'node:test';
import assert from 'node:assert/strict';
import {DEFAULT_SETTINGS, deriveRuntime, interpretDemo, canSubmit} from '../src/model.mjs';

// 场景：本地单次与持续聆听共享一个来源。构造两种交互模式，预期内部选不同引擎。
test('one local source preserves single and continuous capability differences', () => {
  assert.equal(deriveRuntime(DEFAULT_SETTINGS).engine, 'a1', '单次演示应采用 A1 内部链路');
  assert.equal(deriveRuntime({...DEFAULT_SETTINGS, interaction: 'continuous'}).engine, 'sherpa', '持续聆听不得错误采用 A1');
});

// 场景：本地资源不足。执行派发资格检查，预期阻止提交且不切换云端。
test('resource failure blocks submission without cloud fallback', () => {
  const settings = {...DEFAULT_SETTINGS, status: 'failed'};
  const proposal = interpretDemo('Bruno，继续刚才的登录任务');
  assert.equal(deriveRuntime(settings).source, 'local', '失败后来源仍应保持本地');
  assert.equal(deriveRuntime(settings).ready, false, '资源不足应标记不可用');
  assert.equal(canSubmit(proposal, settings), false, '不可用时不得模拟提交');
});

// 场景：Henry 是执行者，Bruno 是方案作者。用固定演示句验证角色不会混淆。
test('executor differs from the person mentioned in the task', () => {
  const proposal = interpretDemo('Henry，帮我检查 Bruno 写的方案');
  assert.equal(proposal.target, 'henry', '执行者应为 Henry');
  assert.deepEqual(proposal.mentioned, ['bruno'], 'Bruno 应保留为被提及者');
  assert.equal(proposal.kind, 'handoff', '复杂角色关系首版应交主模型');
  assert.equal(canSubmit(proposal, DEFAULT_SETTINGS), false, '主模型未处理前不得模拟派发');
});

// 场景：用户要求继续且禁止新建。构造完整句，预期保留约束并使用回复动作。
test('continue with no-new-task stays a reply', () => {
  const proposal = interpretDemo('Bruno，不要新建，继续刚才的登录任务');
  assert.equal(proposal.kind, 'reply', '期望语义为回复现有任务');
  assert.equal(proposal.constraint, 'no-new-task', '禁止新建约束不得丢失');
  assert.equal(proposal.route, 'main', '含否定的句子不走首版快速路径');
  assert.equal(canSubmit(proposal, DEFAULT_SETTINGS), false, '主模型路径仅展示预期语义，不模拟直接派发');
});

// 场景：没有可靠目标。选择一次澄清结果，预期只补齐目标并保留动作。
test('unknown target requires clarification before a demo submission', () => {
  const before = interpretDemo('继续刚才那个');
  assert.equal(before.kind, 'clarify', '未点名且无上下文时应澄清');
  assert.equal(canSubmit(before, DEFAULT_SETTINGS), false, '澄清前不得模拟提交');
  const after = interpretDemo('继续刚才那个', 'henry');
  assert.equal(after.target, 'henry', '澄清后应绑定所选 Henry');
  assert.equal(after.kind, 'reply', '补齐目标不应更改为新建动作');
});

// 场景：已知成员不在画布。预设 Alice 句，预期不替换为 Bruno 或 Henry。
test('outside-canvas target never resolves to an arbitrary present agent', () => {
  const proposal = interpretDemo('让 Alice 来做');
  assert.equal(proposal.targetKind, 'outside', 'Alice 应标记为画布外成员');
  assert.equal(proposal.target, null, '画布外成员不得伪造现有节点');
  assert.equal(canSubmit(proposal, DEFAULT_SETTINGS), false, '未完成作用域处理不得提交');
});

// 场景：用户只询问 Henry 的状态。预设查询句，预期不产生新建或回复动作。
test('status question is not a task assignment', () => {
  const proposal = interpretDemo('Henry 修登录的任务做完了吗？');
  assert.equal(proposal.kind, 'status', '应选择读取任务状态');
  assert.notEqual(proposal.kind, 'assign', '查询不能变成新建任务');
});

// 场景：用户输入未预设的自由文本。执行演示解析，预期明确交回而非伪称模型识别成功。
test('unlisted text is labelled unsupported by the demo', () => {
  const proposal = interpretDemo('这是一句不在演示集里的输入');
  assert.equal(proposal.kind, 'unsupported', '未预设输入必须明确标记为未覆盖');
  assert.equal(canSubmit(proposal, DEFAULT_SETTINGS), false, '未覆盖输入不得模拟派发');
});

// 场景：预览后用户又编辑原话。传入不同当前输入，预期旧候选失效，不能模拟提交。
test('editing the command invalidates an earlier proposal', () => {
  const proposal = interpretDemo('Bruno，继续刚才的登录任务');
  assert.equal(canSubmit(proposal, DEFAULT_SETTINGS, proposal.input), true, '未修改的明确候选应允许演示');
  assert.equal(canSubmit(proposal, DEFAULT_SETTINGS, '改成交给 Henry'), false, '修改输入后旧候选必须失效');
});
