# 豆包语音 2.0 音色

Norma 桌面的豆包模式使用 `seed-tts-2.0` 和方舟套餐 HTTP 单向流接口。设置 → Norma → 语音 → 豆包 2.0 语音 → 音色，可按官方名称、音色 ID 或语言搜索并选择；选择立即显示、后台自动保存，下次开始通话时生效。普通回复和任务结果播报使用同一选择。升级默认保留 Vivi 2.0。

## 音色范围与官方来源

2026-10-04 核对[官方音色列表](https://www.volcengine.com/docs/6561/1257544?lang=zh)，收录两个语音合成 2.0 表格：294 个通用/角色音色与 137 个外语音色，共 431 个唯一 ID。完整数据见 [`voices.json`](../../crates/tts/src/providers/doubao/voices.json)。

| 示例 | 音色 ID |
| --- | --- |
| Vivi 2.0（默认） | `zh_female_vv_uranus_bigtts` |
| 小何 2.0 | `zh_female_xiaohe_uranus_bigtts` |
| 云舟 2.0 | `zh_male_m191_uranus_bigtts` |
| 小天 2.0 | `zh_male_taocheng_uranus_bigtts` |
| 高冷御姐 2.0 | `zh_female_gaolengyujie_uranus_bigtts` |
| Tim（美式英语） | `en_male_tim_uranus_bigtts` |
| Dina（阿拉伯语） | `ar_female_dina_uranus_bigtts` |

[套餐接入文档](https://www.volcengine.com/docs/82379/2516286)确认该接口使用 `seed-tts-2.0`；[合成 API 文档](https://www.volcengine.com/docs/6561/2528925)将此资源指向上述 2.0 音色表。部分外语音色只支持单向流，Norma 当前 HTTP 单向流可覆盖该接口范围。官方目录里的 `ICL_uranus_*` 系统音色仍被收录；用户私有克隆音色需要独立的 `seed-icl-2.0` 资源，不在此功能范围内。目录不包含 1.0 或仅属于 S2S 的其他表格。

实际可调用音色以账号权限与服务端响应为准；目录记录官方模型兼容范围，不代表全部音色已逐一通过当前账号的云端合成。

## 实现与验收

- 菜单按用户指定的 Google Material 哲学设计，规则见 [`material-menus.md`](../design-docs/material-menus.md)。参考 `apple-design` 的平台行为、`emil-design-eng` 的反馈原则以及 GPUI 元素、样式、实体与 actions 技能：收起时只显示当前音色，展开后显示可搜索浮层与最多六行的虚拟列表；不增加装饰动效。
- 复用 `ui::component::Button`、`Popover`、`List` 与搜索输入，提供 hover、pressed、键盘焦点和右侧勾选。Enter/Space/↓ 打开，↑/↓ 浏览、Enter 确认、Escape 或 Tab/Shift-Tab 取消；浏览高亮与已保存值独立，取消不保存。每次打开清空搜索并定位当前音色；音色选择不依赖凭据。
- 单元测试覆盖默认值、目录完整性、搜索、无效 ID、后台保存和重载，以及实际 HTTP 请求的 speaker 与 PCM 格式。
- 2026-10-04：`cargo test -p app --lib doubao -- --test-threads=1` 26 项通过；`cargo test -p tts` 22 项通过、1 项忽略；`cargo check -p app` 和 release 构建通过。
- Computer Use 操作独立的真实 App：中文名称与语言搜索、无匹配提示、鼠标选择小何、Tab/Enter 选择云舟、Tab/Space 选择小天均成功；重启 release App 后仍显示小天已选，空搜索恢复完整可滚动目录。未配置凭据时也能选择音色，未进行云端音色合成。
- 全仓串行测试的 app 套件为 920 项通过、2 项失败、7 项忽略；失败属于工作区首页阅读衬底与 Orchestration 设置页头高度。国际化检查报告其他壁纸模块的硬编码文本，本次音色模块无报告。

## 2026-10-05 Material 菜单改版验收

- `cargo test -p app --lib doubao -- --test-threads=1`：31 项通过。新增真实 List 搜索 Input 的 Tab/Shift-Tab 回归测试，验证先浏览小何、退出后菜单关闭、字段恢复焦点，以及 App 的已保存 Vivi 保留；已确认原按键监听会失败，改为捕获 Input 动作后通过。
- `cargo check -p app` 与 `cargo build -p app --release` 通过。禁止 allow、Agent 指南行数与 `git diff --check` 通过；本次音色 Rust 文件均不超过 500 行。全仓行数检查另报 `crates/app/src/mobile/server/connection.rs` 为 523 行。
- Computer Use 操作隔离的 release App：鼠标打开后直接搜索“小何”，Tab 取消并保留小天；Space 重开恢复空查询与完整目录；搜索“云舟”后 Shift-Tab 取消、字段恢复焦点。重启后仍显示“小天 2.0”，浅色与深色下菜单、箭头、勾选与键盘浏览反馈均清晰。
- 为恢复当前工作区编译，补齐壁纸提示调用的 Context 参数及壁纸测试的 Window 参数；另运行壁纸确认测试时发现其自身的 App 重入借用错误，未作为音色菜单验收结果。

最终真实 App 截图目录：`/private/tmp/norma-doubao-voice-qa/evidence/`。

| 验收行为 | 截图文件 |
| --- | --- |
| 浅色菜单与已保存勾选 | `final-material-menu-light.png` |
| 浅色键盘浏览云舟，已保存勾仍在小天 | `final-material-menu-light-browsing.png` |
| 深色菜单与空查询重开 | `final-material-menu-dark.png` |
| Tab 取消并恢复字段焦点 | `final-material-tab-cancel.png` |
| Shift-Tab 取消并恢复字段焦点 | `final-material-shift-tab-cancel.png` |
| 重启后小天保留 | `final-material-voice-persisted.png` |
