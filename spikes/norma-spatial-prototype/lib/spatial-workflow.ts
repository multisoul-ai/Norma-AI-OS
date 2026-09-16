import type { State } from './model.ts';
import { developmentGoal } from './cli.ts';
export type SpatialMenu = 'space' | 'content' | 'layout' | null;
export const spaceMenu = [
  { label: '呈现内容', action: 'menu:content' },
  { label: '你的团队', action: 'team' },
  { label: '摆放与大小', action: 'menu:layout' },
  { label: '演示对话', action: 'dialogue' },
  { label: '重新居中', action: 'recenter' },
  { label: '离开空间', action: 'exit' },
];
export const contentMenu = [
  { label: 'Codex CLI', action: 'command:打开 Codex CLI' },
  { label: 'Claude CLI', action: 'command:打开 Claude CLI' },
  { label: '代码改动', action: 'view:code' },
  { label: '审查报告', action: 'view:research' },
  { label: '实时图表', action: 'view:data' },
  { label: '网页与图片', action: 'view:preview' },
  { label: '三维对象', action: 'view:model' },
  { label: '自由画布', action: 'view:canvas' },
];
export function conversationSuggestions(state: State) {
  const codex = state.agents.find((a) => a.id === 'codex')!;
  return [
    codex.status === 'working'
      ? { label: '先暂停 Codex', text: '暂停 Codex' }
      : codex.status === 'paused'
        ? { label: '继续刚才的任务', text: '继续 Codex' }
        : { label: '让团队开始协作', text: developmentGoal },
    { label: '看看 Claude 的审查', text: '打开 Claude CLI' },
    { label: '把测试结果放到眼前', text: '展示测试数据' },
  ];
}
export type SpatialVoice = {
  listening: boolean;
  speaking: boolean;
  transcript: string;
  readAloud: boolean;
  error: string;
};
