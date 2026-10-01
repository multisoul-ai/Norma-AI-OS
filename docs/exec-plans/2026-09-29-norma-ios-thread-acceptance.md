# #56–#57 接续：关联选择、工作线程与简报

状态：实施中，**Goal 保持 active，#56/#57 未验收完成**。分支 `codex/norma-ios-companion`，2026-09-29。

## 已实现

- P03 的 Workspace 卡进入线程列表；成员旁提供独立项目/Agent 配置入口。选择器标明 Agent 所属电脑和项目，失败可重试；只有保存成功才关闭，失败保留勾选。
- 想法菜单「在工作空间推进」明确选择 Workspace 并创建线程；原想法不移动，保存来源 ID 和当时正文快照。来源页可打开原件，读取失败可重试。
- P05 本机线程：长期消息、跨日日期、待发送状态、按线程保留当前页面草稿；无实际 Mac 回执时不显示执行、完成或 Agent 回复。消息可记为共同决定，保留来源。
- W05：线程目标、Workspace 项目、共同事实/决定、逐版本历史与源消息正文；版本冲突不能覆盖当前值，保留用户草稿并允许重新读取核对。
- SQLite 复用 `CollaborationWorkspaceStore`。两个线程和两个 Workspace 隔离，稳定消息 ID 去重；归档可读、拒绝新写入；原有想法、配对、私聊数据不迁移或删除。

## 测试与构建范围

| 验证 | 结果与证据 |
|---|---|
| `swift test --disable-sandbox --package-path ios --cache-path target/swift-cache --filter 'WorkThreadStoreTests\|CollaborationWorkspaceStoreTests'` | 4 项通过。首次接口未实现时失败；跨线程来源查询也先新增失败测试再实现。日志：`target/norma-ios-typecheck/source-red.log`、`thread-tests.log`。 |
| iOS Simulator SDK 构建 `NormaCore` | `swift build --disable-sandbox --package-path ios --cache-path target/swift-cache --scratch-path target/norma-ios-typecheck/cross-build --triple arm64-apple-ios17.0-simulator --sdk /Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/SDKs/iPhoneSimulator26.2.sdk --target NormaCore` 通过；`target/norma-ios-typecheck/cross-build.log`。 |
| Swift iOS SDK 全部 App 源文件类型检查 | 使用上述 Core 模块、已有 WebRTC framework、Swift 6 / iOS 17 target；通过。响应文件与日志为 `target/norma-ios-typecheck/app-sources.txt`、`app.log`。**仅编译语义检查，没有运行 App，也没有生成新的安装包。** |
| Swift iOS SDK 全部 UITests 类型检查 | 通过；`target/norma-ios-typecheck/ui-test-sources.txt`、`ui-tests.log`。新 `WorkThreadFlowTests`、`WorkspaceAssociationFlowTests` 已加入 Xcode 工程，但尚未在模拟器执行。 |
| `git diff --check` | 通过；未 commit/push。 |

初次直接调用 Swift 编译器时，宏插件的嵌套沙箱失败；按 SwiftPM 在受限环境中的标准构建参数处理后 Core 与类型检查通过。以上操作不涉及启动 Simulator，不替代真实 App 验收。

## 本轮环境阻断

隔离 Rust TLS fixture 的第三次启动在 09:46 终止，退出码 101。具体原因：

```text
CoreSimulatorService connection became invalid
Error opening log file .../Library/Logs/CoreSimulator/...: Operation not permitted
Could not resolve package dependencies:
cannot open .../Library/Caches/org.swift.swiftpm/manifests/ManifestLoading/webrtc.dia
for diagnostics emission (Operation not permitted)
```

`scripts/check-norma-ios-transport.sh` 现在提供工作区 package cache 并把 stderr 写入原生日志，避免隐藏实际失败原因；没有变更配对或凭据保护。三个失败 fixture 都已终止，不能把它们当作在线 Mac。此前 Computer Use 选择 Simulator 被拒，本轮没有尝试绕过。

## 独立只读复核

`/root/new_shell_ui_review` 初审为 NEEDS WORK，指出读取失败假空库、保存中继续输入会丢字符、版本冲突无法恢复、长期历史日期不完整、VoiceOver 编辑标签缺失。修复后，复核对**这些代码项**给出 PASS，未发现新增阻断。

**视觉、键盘、手势与实际 VoiceOver 仍为 NEEDS WORK。** 本轮没有新增截图，不能把代码复核称为原型还原通过。Apple 设计落实为：系统 NavigationStack/Sheet 转场，至少 44pt 主要按钮，正文实底，输入功能层采用 iOS 26 原生玻璃并为旧版/减少透明度回退；实际观感还需截图确认。

## 下一步验收

1. 环境允许后重新启动独立 TLS fixture，运行 `WorkspaceAssociationFlowTests`，精确清理测试电脑。
2. 运行 `WorkThreadFlowTests`、更新后的 `WorkspaceLibraryFlowTests`，以及首页和单聊入口回归；截图 P03/P04/P05/W03/W05，核对线程返回、Sheet、键盘和归档。
3. 用 Computer Use 操作 App；把相同机型的新截图交独立子 Agent 按原型复核，偏差继续返工。
4. 真机 Apple Speech/扫码、真实云服务及消息重连送达仍待验收。当前线程输入只落本机；#59 的派工、语音输入和真实任务/成果尚未交付，不因本机测试通过而关闭这些门槛。

## 接续状态（覆盖上文历史阻断，2026-09-29）

完整执行权限已恢复，Xcode/Simulator 构建和原生测试均通过；上文缓存拒绝属于历史。Computer Use 窗口连接也已恢复，根任务已实际操作标准字号的线程、Agent 回复、执行完成卡、任务详情及返回。最新 @Agent、冻结上下文、任务卡与复核记录统一见 [逐票进度文末](2026-09-29-norma-ios-local-test.md)。

执行状态、具名回复及成果元数据现已按完整任务引用回原线程，事务去重、重启恢复及结果查询共用预算已测试。独立复核提出的部分回复辅助语义、等待成果文案已修复；最终原生专项 8 通过、1 fixture 跳过、0 失败。标准字号截图来自 Computer Use 操作；最大字号线程/详情截图来自原生 UI 测试附件，尚不能声称最大字号手势通过。Workspace 标题/筛选区在最大字号下拥挤，已留证待修；完整视觉仍 NEEDS WORK。

结果网络专项使用真实本机中继与 MobileServer，但执行数据为显式 fixture。真实 Coding Agent、成果正文/文件对比、VoiceOver 实操和剩余页面验收尚未完成，#57/#59 与 Goal 保持进行中；Norma 语音通话最后，真机项目由用户后置。

## W06a终端键盘切片（2026-09-30，最新）

范围：原任务W06工作情况/终端切换中的W06a阅读、接管、系统键盘、功能键、后台草稿恢复、重新接管与交还。参考`docs/product-specs/norma-ios-prototypes-2026-09-26/W06a-terminal-control.png`，依现行产品规则移除旧硬约束文案。真实App为iPhone16e/iOS26.3、1170×2532截图；AX5为最大辅助字号。

Apple设计落实：内容用浅色实底/深色终端，功能键用系统玻璃；键盘时任务标题进入系统导航，最大字号将成功反馈和接管状态放一行。长身份名称与项目可原生横向滚动，项目有独立视觉边界。保留动态字体、44pt操作区和原生输入法，光标与视口边界保留8pt留白。

### 初审→复核

独立只读`thread_delivery_ui_review`初审NEEDS WORK：只读误报执行中、退出/失联残留接管提示、AX5输出被键盘挤压、圆角裁字、身份混读、右侧光标证据不足。修复后`ui-final-check`标准/AX5 2/2通过，Agent最终对**该终端UI切片**给出PASS，无新增必改项。AX5输出114pt、标准113pt，断言要求整个第71列光标位于视口内，90pt/60pt最小输出要求未降低。

截图目录：`target/norma-ios-evidence/terminal-ui-20260930/screenshots-final-check/`。

| 状态 | 文件 | 证据 |
|---|---|---|
| 标准只读 | `FDD1E789-2028-49AE-A8D3-32BE2CD93634.png` | 只读状态、接管入口、首字与圆角留白。 |
| 标准键盘 | `F918DA77-6060-493B-9B90-551AB95421F2.png` | 系统键盘保持、第71列光标完整、写入反馈与交还可达。 |
| AX5只读 | `CDA0A00C-1D0C-4F76-B882-4472B226C457.png` | 大字号状态与终端区域。 |
| AX5键盘 | `98CE270E-5418-4D42-BA74-1F359178A500.png` | 后台返回只读、草稿恢复后显式接管；实际横向滚动露出完整Mac名称、独立项目标签、长行末端光标。 |

几何记录为同目录`ACDED118-5CA5-409D-95B9-333A437CA551.txt`（AX5）和`41E8D00C-D8DD-4CF4-84EA-41CEF6327BB7.txt`（标准）。失败与修复日志保留在上级证据目录，详见[逐票进度文末](2026-09-29-norma-ios-local-test.md)。清理原生临时配对/云凭据1/1后，正式网络fixture完成输入次数、撤权、停服断言并退出0。

### 未通过的范围

本轮终端输出与执行目标是显式前台fixture，网络为正式本地中继/MobileServer/SQLite；不等同于真实PTY或Coding Agent端到端交付。Computer Use最后重查仍明确Mac锁屏，必须解锁后补实际操作。历史/实时切换、工作情况阅读位置、长项目名滚动、完整VoiceOver、错误/未决状态的实际界面操作仍待补证据。不能将此局部PASS扩大为完整W06a、所有原型或Goal完成。
