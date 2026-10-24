# Norma 可交互原型

当前入口：[iOS 想法与工作空间原型](norma-ios-workspace.html)。主状态使用 [P01–P05 五张设计图](../product-specs/norma-ios-ideas-workspaces-2026-09-28/)：想法库独立记录文字和语音转写；多个工作空间分别关联已有项目和 Agent；从想法可选择工作空间创建工作线程。底部仅保留工作空间、Norma、收件箱，其他模块从抽屉进入。所有数据与 Agent 回执仅为本地交互演示。

```bash
npm ci --prefix docs/prototypes
python3 -m http.server 8766 --bind 127.0.0.1 --directory docs
BROWSER_CHANNEL=chrome node docs/prototypes/tests/norma-ios-ideas-workspaces.cjs
```

浏览器入口：http://127.0.0.1:8766/prototypes/norma-ios-workspace.html

测试依赖由本目录的 package.json 与 package-lock.json 管理，使用本机 Google Chrome；运行两套检查可执行 `BROWSER_CHANNEL=chrome npm test --prefix docs/prototypes`。

测试在 Google Chrome 的 390×844 触控视口中覆盖文字及语音记录、想法与工作空间分离、多个工作空间独立配置、来源想法和线程消息。

[旧版九屏原型](archive/2026-09-28-norma-ios-workspace/norma-ios-workspace.html)已归档，可运行 `BROWSER_CHANNEL=chrome node docs/prototypes/tests/norma-ios-workspace.cjs` 验证。旧版截图与记录见 [验收记录](assets/norma-ios-workspace/validation.md)。

此前的 [实时虚拟人指挥台](norma-realtime.html) 仍保留作历史方案参考。

v28 手机工作台（项目 / 决策 / 配对）已于 2026-09-20 归档，见 [archive/2026-09-06-norma-ios](archive/2026-09-06-norma-ios/README.md)。
