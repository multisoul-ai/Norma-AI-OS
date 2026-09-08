import { readFileSync } from 'node:fs';

const fixture = JSON.parse(readFileSync(new URL('../protocol/snapshot.v1.json', import.meta.url)));

export function createSession() {
  return structuredClone(fixture);
}

export function resolveDecision(snapshot, command) {
  const valid = command?.version === 1 && command.type === 'decision.resolve'
    && typeof command.requestId === 'string' && command.requestId.length > 0
    && command.requestId.length <= 128;
  const decision = valid && snapshot.decisions.find(d => d.id === command.decisionId);
  const option = decision && decision.options.find(o => o.id === command.optionId);
  if (!option) return { accepted: false, snapshot, reason: 'unknown_or_resolved_decision' };

  const decisions = snapshot.decisions.filter(d => d.id !== decision.id);
  const agents = snapshot.agents.map(agent => {
    if (agent.taskId !== decision.taskId) return agent;
    const waiting = decisions.some(d => d.taskId === agent.taskId);
    return { ...agent, status: waiting ? 'waiting' : 'working', step: `已选择「${option.label}」· 模拟执行` };
  });
  return {
    accepted: true,
    snapshot: {
      ...snapshot, sequence: snapshot.sequence + 1, agents, decisions,
      terminal: { ...snapshot.terminal, lines: [...snapshot.terminal.lines, `[mock] ${decision.title} → ${option.label}`].slice(-200) },
    },
  };
}
