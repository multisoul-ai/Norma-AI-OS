export const DEFAULT_SETTINGS = Object.freeze({
  source: 'local', interaction: 'single', trigger: 'hold', microphone: 'builtin',
  spokenReply: true, volume: 72, status: 'ready', cloudConfigured: false,
  clarify: 'target', shortcut: '⌘ E',
});

export const DEMO_AGENTS = Object.freeze([
  {id: 'bruno', name: 'Bruno', alias: '布鲁诺', node: 'demo-node-bruno', task: 'login', color: 'blue'},
  {id: 'henry', name: 'Henry', alias: '亨利', node: 'demo-node-henry', task: 'review', color: 'green'},
]);

export function deriveRuntime(settings) {
  return {
    ready: settings.status === 'ready' && (settings.source === 'local' || settings.cloudConfigured),
    engine: settings.source === 'cloud' ? 'azure' : settings.interaction === 'continuous' ? 'sherpa' : 'a1',
    source: settings.source,
  };
}

export function interpretDemo(text, clarifiedTarget = null) {
  const normalized = text.toLowerCase().replace(/[\s，、：:,!！?？。.]/g, '');
  const base = {target: null, targetKind: 'canvas', mentioned: [], constraint: null, route: 'demo', input: text};
  const fixtures = [
    [['Bruno，继续刚才的登录任务', 'Bruno, continue the login task'], {kind: 'reply', target: 'bruno'}],
    [['给 Bruno 新建一个任务，检查登录报错', 'Create a task for Bruno: check the login error'], {kind: 'assign', target: 'bruno'}],
    [['Henry，帮我检查 Bruno 写的方案', "Henry, review Bruno's proposal"], {kind: 'handoff', target: 'henry', mentioned: ['bruno'], route: 'main'}],
    [['Bruno，不要新建，继续刚才的登录任务', 'Bruno, continue the login task, do not create a new one'], {kind: 'reply', target: 'bruno', constraint: 'no-new-task', route: 'main'}],
    [['Henry 修登录的任务做完了吗？', 'Has Henry finished the login task?'], {kind: 'status', target: 'henry'}],
    [['你帮我解释一下这段代码', 'Explain this code to me'], {kind: 'answer', targetKind: 'self'}],
    [['让 Alice 来做', 'Ask Alice to do it'], {kind: 'clarify', targetKind: 'outside'}],
    [['继续刚才那个', 'Continue the previous task'], DEMO_AGENTS.some(a => a.id === clarifiedTarget)
      ? {kind: 'reply', target: clarifiedTarget, route: 'clarified'} : {kind: 'clarify'}],
  ];
  for (const [phrases, result] of fixtures) {
    if (phrases.some(phrase => phrase.toLowerCase().replace(/[\s，、：:,!！?？。.]/g, '') === normalized)) return {...base, ...result};
  }
  return {...base, kind: 'unsupported', targetKind: 'unknown', route: 'main'};
}

export function canSubmit(proposal, settings, currentInput = proposal.input) {
  if (currentInput !== proposal.input) return false;
  if (!deriveRuntime(settings).ready || proposal.route === 'main') return false;
  if (!['reply', 'assign', 'status', 'answer'].includes(proposal.kind)) return false;
  if (proposal.constraint === 'no-new-task' && proposal.kind === 'assign') return false;
  return proposal.targetKind === 'self' || DEMO_AGENTS.some(agent => agent.id === proposal.target);
}
