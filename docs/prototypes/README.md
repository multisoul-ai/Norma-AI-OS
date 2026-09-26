# Norma 可交互原型

当前入口：[iOS 工作空间九屏原型](norma-ios-workspace.html)。W01–W09 主状态直接使用 [九张设计图](../product-specs/norma-ios-prototypes-2026-09-26/)；工作空间创建、扫码配对、设置、编辑、搜索、消息与验收采用可交互的补充状态。电脑配对、Agent 执行和通话为本地模拟。

```bash
python3 -m http.server 8766 --bind 127.0.0.1 --directory docs
BROWSER_CHANNEL=chrome node crates/e2e/tests/browser/norma-ios-workspace.cjs
```

浏览器入口：http://127.0.0.1:8766/prototypes/norma-ios-workspace.html

测试在 Google Chrome 中覆盖九屏导航、工作空间创建与项目关联、消息回执、简报更新、通话、配对、收件箱、成果验收及归档重开；移动端检查使用 390×844 触控视口。截图和验证结果见 [验收记录](assets/norma-ios-workspace/validation.md)。

此前的 [实时虚拟人指挥台](norma-realtime.html) 仍保留作历史方案参考。

v28 手机工作台（项目 / 决策 / 配对）已于 2026-09-20 归档，见 [archive/2026-09-06-norma-ios](archive/2026-09-06-norma-ios/README.md)。
