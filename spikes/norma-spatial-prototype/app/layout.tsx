import './globals.css';
export const metadata = {
  title: 'Norma Spatial · 你的空间 AI 团队',
  description:
    '与 Norma 实时对话，在空间中协调多个 Agent，探索你的下一个想法。交互原型，任务与数据为模拟。',
};
export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="zh-CN">
      <body>{children}</body>
    </html>
  );
}
