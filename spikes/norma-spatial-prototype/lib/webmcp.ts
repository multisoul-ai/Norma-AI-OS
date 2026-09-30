import type { State } from './model';
type Tool = {
  name: string;
  title: string;
  description: string;
  inputSchema: object;
  annotations: { readOnlyHint: boolean };
  execute(input: unknown): unknown;
};
type Context = {
  registerTool(
    tool: Tool,
    options: { signal: AbortSignal },
  ): void | Promise<void>;
};
export function registerSpatialTools(
  read: () => State,
  send: (text: string) => State,
) {
  const context = (document as Document & { modelContext?: Context })
    .modelContext;
  if (!context) return () => {};
  const lifecycle = new AbortController();
  const register = (tool: Tool) => {
    try {
      Promise.resolve(
        context.registerTool(tool, { signal: lifecycle.signal }),
      ).catch(() => {});
    } catch {
      /* Optional browser capability; UI remains available. */
    }
  };
  register({
    name: 'read_norma_workspace',
    title: '查看 Norma 空间',
    description: '读取当前模拟任务、Agent 状态和主面板。',
    inputSchema: {
      type: 'object',
      properties: {},
      additionalProperties: false,
    },
    annotations: { readOnlyHint: true },
    execute: () => {
      const s = read();
      return {
        mission: s.mission,
        agents: s.agents,
        view: s.view,
        approved: s.approved,
        activeCli: s.activeCli,
        canvas: {
          nodes: s.canvas.nodes,
          annotationCount: s.canvas.strokes.length,
        },
      };
    },
  });
  register({
    name: 'send_norma_mock_command',
    title: '指挥模拟 Agent',
    description:
      '向原型团队发送自然语言指令。仅修改内存中的演示状态，不执行任何外部任务。',
    inputSchema: {
      type: 'object',
      properties: { text: { type: 'string', minLength: 1, maxLength: 500 } },
      required: ['text'],
      additionalProperties: false,
    },
    annotations: { readOnlyHint: false },
    execute: (input) => {
      if (
        !input ||
        typeof input !== 'object' ||
        !('text' in input) ||
        typeof input.text !== 'string' ||
        !input.text.trim() ||
        input.text.length > 500
      )
        throw new Error('请提供 1–500 字的 text 指令');
      const s = send(input.text);
      return {
        mission: s.mission,
        agents: s.agents,
        view: s.view,
        reply: s.messages.at(-1)?.text,
      };
    },
  });
  return () => lifecycle.abort();
}
