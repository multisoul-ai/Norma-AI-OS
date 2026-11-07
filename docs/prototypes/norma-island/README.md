# Norma Island · 任务工作台与灵动岛

本轮按用户提供的任务页面截图设计：左侧是大输入框和按状态分组的工作，右侧是当前工作的独立对话，顶部加入 Norma Island。原画布作为执行现场保留。默认背景仍复用仓库中的 `crates/app/assets/wallpapers/norma-awake.mp4`，任务页面用深色半透明材料降低背景干扰。

## 打开

本地服务从 worktree 根目录提供静态文件：

```bash
python3 -m http.server 8745 --bind 127.0.0.1
```

访问 `http://127.0.0.1:8745/docs/prototypes/norma-island/`。也可直接在浏览器打开本目录的 `index.html`。

## 体验路径

1. 主页面：在大输入框创建示例任务，左侧切换工作，右侧补充要求、查看推荐方案与依据。
2. 点屏幕顶部：灵动岛展开“需要你 / 全部工作 / 交付物”，打开事项后可以采用、调整、讨论或查看步骤。
3. 点“去看现场”：岛收起，原画布定位到对应卡片。再点顶部“返回任务”，恢复原场景和讨论。
4. 底部切换“其他 App”：演示跨应用查看与处理，模拟备忘录不会控制真实 App。
5. 点“模拟新消息”：Nova 的问题进入待办，正在看的任务和草稿保持原位。
6. 点“回来时的简报”：将待办和进展合并展示。交付物可以打开并实际下载一份示例 Markdown。
7. 切换“无刘海屏幕”：查看顶部悬浮胶囊适配；支持中英文、背景视频暂停与减少动态效果。

## 状态约定

- 任务页和岛共用工作、决定、消息与草稿。同一工作在两个入口中继续讨论；不同工作互不串线。
- 回答或授权仅进入“继续执行”；验收进入“待合并”，不会直接冒充合并、发布或完成。
- 新建工作进入“等待拆解”，不会实际派工。
- 画布接管为状态模拟，接管期间禁用采用建议。
- 刷新页面重置示例数据。URL 不代表实际 Agent 或真实 macOS 系统状态。

## 设计与实现边界

遵循项目 Apple Design：入口与面板从同一位置出现，收起返回原位，画布定位可返回。使用 animate 技能约束动效：背景形状用 clip-path、内容用 transform / opacity；展开 240ms、内容淡入 150ms；键盘操作和减少动态效果即时切换。原画布拖动直接跟手。

原型采用原生 HTML、CSS、JavaScript，无新增依赖。模态编辑和依据窗口使用原生 dialog。刘海中心保留不可交互的摄像头占位，内容放在两侧和下方。

此产物是浏览器内的 macOS 场景模拟，不是原生 NSPanel 或系统刘海扩展；没有连接真实 Agent、Notion 或其他 App，也没有修改正式 Rust App。

## 验证

7 项 Playwright 测试通过：展开收起、真实视频解码、工作与草稿隔离、新事件不切走任务、调整建议、验收阶段、跨场景定位返回、刘海与无刘海适配、390–1440px 布局、语言、减少动态效果、接管、示例文件下载、新建任务，以及跨入口发送后旧草稿不会恢复。

```bash
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules \
node /Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules/playwright/cli.js \
test docs/prototypes/norma-island/tests/flows.spec.cjs \
--workers=1 --reporter=line --timeout=30000 --output=/tmp/norma-island-tests

python3 scripts/check-i18n.py
```

截图已在浏览器中检查，保存在 `assets/`：任务页、岛总览、事项展开、卡片定位、其他 App 场景与窄窗口。截图暂停背景于第 3 秒，原型默认循环播放。

```bash
NODE_PATH=/Users/openclawd/Documents/code/norma-os/Norma-AI-OS/docs/prototypes/node_modules \
node docs/prototypes/norma-island/tests/capture.cjs
```

本次仅变更隔离的浏览器原型，未运行无关 Rust 构建或原生 App desktop-e2e；原生跨桌面、全屏、多显示器与实际刘海行为留待正式实现验收。
