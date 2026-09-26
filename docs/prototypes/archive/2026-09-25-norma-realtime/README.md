# 归档：Norma 实时虚拟人指挥台原型

2026-09-25 按用户要求归档。这里的 `norma-realtime.html` 是替换频道协作原型前的**工作区原样快照**，包含当时尚未提交的修改；没有从 Git 历史重建。

本地查看：

```bash
python3 -m http.server 8766 --bind 127.0.0.1 --directory docs
```

打开 `http://127.0.0.1:8766/prototypes/archive/2026-09-25-norma-realtime/norma-realtime.html`。原检查脚本与浏览器用例的原样副本分别放在 `scripts/archive/2026-09-25-norma-realtime/` 和 `crates/e2e/tests/browser/archive/2026-09-25-norma-realtime/`；它们保留历史引用，不作为新版入口的验收脚本。

新版实施计划：[Norma iOS 频道原型 Goal](../../../exec-plans/2026-09-25-norma-ios-channel-prototype-goal.md)。
