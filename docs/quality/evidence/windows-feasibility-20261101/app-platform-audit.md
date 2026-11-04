# App 平台边界与 Mac 16 项失败核验

Windows App 编译尚未通过，整体迁移未验收。本文件补充原 Runtime/Codex 审计，不覆盖任何历史 FAIL。

## 验收与证据映射

| 要求 | 实际执行证据 | 判定边界 |
|---|---|---|
| 116 条逐项定位 | `docs/quality/evidence/windows-feasibility-20261101/app-116-baseline.json`、同名 CSV | 原始路径、行列、代码、消息和 12 类根因完整保留 |
| 先失败后修复 | Windows RED 41377422839；Mac 分段读取、路径交换、慢滴和退出 RED 日志 | 不能用独立 probe 代替默认 App 编译 |
| App IPC/图片能力 | 实际 App 源文件通过 `#[path]` 接入独立 probe；标准 Windows runner 执行 | 没有替换为假实现；未禁用产品模块 |
| 真实 relay 输出背压与关闭 | `actual_relay_output_backpressure_detects_native_shutdown` | 观测真实底层连续 WouldBlock，原 3 秒断言保持 |
| Mac 原 16 项 | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/macos-16-comparison.json` | 两轮完整 workspace，逐项列出；隔离绿灯不覆盖完整失败 |
| 无新增忽略或能力裁剪 | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/implementation-integrity.json` | 默认产品 features、版本 pin 和既有忽略保持 |
| 真实 GUI、Agent、候选包 | 无实际产品证据 | 全部 NOT_RUN，不标完成 |

## Windows 实际运行

### 最终完整配置复验

最终复验：[41386261217](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41386261217)，源码 `120492ca29ec49b12ed4368eb0d10e01b5d34845`，attempt 1，整体 failure。
App 默认 check 仍为 98 条，原 18 条消除；norma 库/CLI/helper 编译通过。
原平台目标为 47/1/0，原 43 项通过名完整保留；失败只有新增的严格 close 见证。

控制 50/1/0、Runtime 11/0/0、identity 1/0/0；两个 OpenCode 目标各 1/0/0。
实际 App 适配 15/0/0、独立平台 30/0/0、canvas 52/0/0、语音 89/0/16。

强见证记录真实输出 8 次连续 WouldBlock、5 个源帧、全部 source 句柄释放 true，原 3 秒内仍未退出。
原 shutdown 测试本轮通过，但历史 FAIL 没有生产修复证据，可靠性仍未收束。

当前 AC 为 0 PASS、3 FAIL、9 NOT_RUN；AC01、AC02、AC11 不标完成。

中间运行 41385201505 已因关闭见证前提修正主动取消，不用作通过证据。
报告 SHA、run/attempt 一致，未缺报告；完整日志和哈希清单保存在对应 run 目录。

| 最终检查 | 通过/失败/忽略 | 日志入口（run 41386261217） |
|---|---|---|
| 默认 App | FAIL，98 条 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/compile/app_compile.log` |
| norma 原平台目标 | 47/1/0，原 43 通过保留 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/compile/norma_platform_tests.log` |
| 控制传输 | 50/1/0 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/control/control.log` |
| Runtime | 11/0/0 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/control/runtime.log` |
| OpenCode transport | 1/0/0 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/control/opencode_transport.log` |
| OpenCode runtime | 1/0/0 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/control/opencode_runtime.log` |
| App 适配/平台/canvas/语音 | 15/0/0、30/0/0、52/0/0、89/0/16 | `docs/quality/evidence/windows-feasibility-20261101/41386261217/compiler-blockers.json` |

### 实现阶段运行

实现运行：[41384140463](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41384140463)，源码 `a53698f635c1501dd37aa4799add30fc4f68b164`，attempt 1，整体 failure。

| 检查 | 通过/失败/忽略 | 结果 |
|---|---|---|
| App 默认 check | exit 101，98 条 | 原 13 条 IPC 与 5 条图片错误消除，另 10 类 98 条原诊断仍阻塞 |
| 实际 App 适配 | 15/0/0 | Windows 请求/响应、路径身份、超时、取消、图片读写实际执行 |
| norma 库及原平台目标 | 47/1/0 | 原 43 项通过名逐项保留；新增背压 shutdown 见证失败，命令 FAIL |
| Codex 控制 | 49/2/0 | 新强见证与原 shutdown-under-backpressure 测试失败；未新增 ignore |
| Runtime 控制 | 11/0/0 | 既有目标全执行 |
| 独立平台边界 | 30/0/0 | memory 1、workspace/Review/NT 28、A1 工厂 1 |
| canvas | 52/0/0 | 原 52 项完整执行 |
| 语音 | 89/0/16 | 默认 voice_input check 通过；16 个既有忽略，非真实音频验收 |
| 真实 Norma GUI / Agent / 候选包 | NOT_RUN | 图形示例、CLI version 和协议夹具不替代产品验收 |

原 116 项逐项对照与本次 98 条原文位于 `docs/quality/evidence/windows-feasibility-20261101/app-116-comparison.json`。
本运行没有执行两个 OpenCode Windows 目标，也没有完整 source-close 前提；它们仅以上面的最终运行单独判定。

## 原 116 条诊断按根因归类

基线运行：[41345708985](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41345708985)。
源码 `e7a7a78a9a871748f28becf1a6de32702ce41cd9`，完整默认 App 检查失败。
同一根因可产生多条诊断；数字不是独立功能数。

| 根因 | 原诊断数 | 本轮处理 |
|---|---|---|
| Chrome 认证端点与权限 | 4 | 阻塞保留，未禁用能力 |
| 原生 Browser/WebKit/Wry 后端 | 56 | 阻塞保留，未禁用能力 |
| 二维码与自动释放池 | 3 | 阻塞保留，未禁用能力 |
| Push token 私有权限 | 2 | 阻塞保留，未禁用能力 |
| 手机 reactor 事件注册 | 3 | 阻塞保留，未禁用能力 |
| 图片私有文件 | 5 | 完整 App 日志实测已消除，适配 15 项通过 |
| App IPC 租约与端点身份 | 13 | 完整 App 日志实测已消除，适配 15 项通过 |
| 内容打开与保存平台边界 | 10 | 阻塞保留，未禁用能力 |
| Brain 符号链接与本地时间 | 5 | 阻塞保留，未禁用能力 |
| App 原生生命周期成员 | 5 | 阻塞保留，未禁用能力 |
| 网络地址枚举 | 9 | 阻塞保留，未禁用能力 |
| 手机 SQLite 身份条件编译 | 1 | 阻塞保留，未禁用能力 |

### 仍阻塞的 98 条具体平台边界

| 根因 | 条数 | 当前诊断对应的根因 | 下一条聚焦验证 |
|---|---|---|---|
| Browser 原生后端 | 56 | WebKit/AppKit/objc2 直接导入；Wry/CEF 等依赖仅在 macOS 声明 | 实际 Windows WebView2 窗口、导航、认证、焦点与关闭；不能空实现 |
| Chrome 认证 | 4 | std::os::unix Socket/Permissions API | 受限 Windows 本地端点、同用户认证、陈旧/冲突与清理 |
| 内容打开/保存 | 10 | workspace::content 被 Unix 条件排除，App 仍调用 | Windows 文件替换、外部冲突、备份恢复及路径身份 |
| 网络地址枚举 | 9 | libc getifaddrs 与地址结构 | Windows 真实适配器枚举、IPv4/IPv6/回环及过滤 |
| Brain 配置/时间 | 5 | Unix symlink 与 libc 本地时间 | 配置身份、符号链接替换与本地时区格式 |
| App 生命周期 | 5 | ego_preview 模块/成员条件编译与调用不一致 | Windows 真实能力适配、项目清理与退出收口 |
| QR/自动释放池 | 3 | CoreImage/Foundation 与 autorelease 调用 | Windows 实际 QR 编解码和后台执行 |
| 手机 reactor | 3 | AsRawFd、tokio::io::unix::AsyncFd | Windows Socket readiness、取消和关闭，不忙等 |
| Push token 权限 | 2 | Unix PermissionsExt::mode | Windows 私有权限、已有 token 的拒绝/接受边界 |
| 手机 SQLite 身份 | 1 | MobileAccess::open 的平台声明边界 | Windows 可信路径与数据库身份核验 |

这 98 条均逐条匹配原诊断的规范化路径、行列和错误码，没有归为未知的新诊断。

## 本轮实现与红绿证据

本轮基线为 `1dd8965bb860aab7892c8e733426702bdc2d2408`。
先提交 Windows 编译 RED `1d5b8e2441f3a0f9515b589ee4f2bf0fe92b7d51`，再提交输入锁 RED `5b70a977a1f89e7ac078ad6a8d15392a4c598be7`。
实现提交 `a53698f635c1501dd37aa4799add30fc4f68b164`；测试目录可移植与 CI 覆盖提交 `bc8cdc1391abe2c6b87c61aa0fe3d23c740be5c5`。

| 行为 | 修复前证据 | 修复及不变条件 |
|---|---|---|
| 实际 App 传输编译 | RED runner 的独立 probe 13 条 IPC 错误 | 复用 norma 的 Unix/Windows AF_UNIX 类型；未开放无认证 TCP |
| 分段 NDJSON | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/norma-app-adapter-red.log`：3/1/0，BrokenPipe | accept 后恢复 blocking；完整 UTF8 帧解码，保持协议和 operation_id |
| 陈旧探测路径交换 | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/norma-app-socket-exchange-red.log`：替代正文遭删除 | 保留探测前身份并复核；Windows 通过 pinned HANDLE 删除 AF_UNIX reparse 本体 |
| 未知探测错误 | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/norma-app-probe-error-red.log`：原 bool 接口不能表达未知 | 200ms connect 探测；仅拒绝连接/不存在算失活，超时及权限错误保留端点 |
| 慢滴请求 | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/norma-app-lifecycle-red.log`：原路径耗时 4.12 秒 | 2 秒绝对读/写预算、25ms 取消检查，64MiB 帧上限 |
| 关闭与 UI 回执 | `docs/quality/evidence/windows-feasibility-20261101/app-platform-local/norma-app-shutdown-red.log`：退出等 10.003 秒 | 10 秒响应等待可取消；已入队副作用返回原 operation_id 的 OutcomeUnknown |
| 租约与释放 | 冲突、陈旧端点、普通文件、路径交换回归 | `File::try_lock`；Unix dev/ino；Windows volume/fileIndex 与 AF_UNIX tag；取消无界唤醒 connect |
| 图片读写边界 | 大文件/篡改/普通文件夹具 | 产品同一 2MiB LIMIT、2048² 像素上限；私有 DACL 与 no-reparse 打开；Unix NOFOLLOW/NONBLOCK 保持 |

表内日志位于 `docs/quality/evidence/windows-feasibility-20261101/app-platform-local`。
队列 32/64、4 个 worker、权限核验和会话归属保持；没有新增 allow、ignore 或关闭默认功能。
独立 probe 的图片解码只链接现有产品接受的 PNG/JPEG；产品 App 的默认 image features 未改变。
Unix 的身份复核后路径 unlink 不是完整的对抗式原子删除证明；AC02/AC08 仍需完整安全矩阵。

## 前轮关闭测试的实测归因

原运行 41345708985 的弱见证只看到外部 writer 瞬时背压，不能证明 relay 已在输出端背压。
新增测试观测真实 relay 底层 write 的连续 WouldBlock，任何写进展都清零。
额度为 usize::MAX，不注入写失败；最多 128×512KiB 帧，有界 64MiB 输入。
达到 8 次连续 WouldBlock 后，正断言 relay 仍存活，再执行 source 的 shutdown，保持原 3 秒 stopped 门槛。
RED runner 实际观测 8 次与 4 个完整源帧，测试失败；旧局部绿灯不覆盖此失败。

实现复验 41384140463 实际为控制 49/2/0、Runtime 11/0/0；两条失败为新强见证和原有关闭测试。
新见证在关闭后读取 `FIONREAD=131072`；`WSAPoll interest=0` 返回 0，`POLLRDNORM=256` 只返回可读。
丢弃主线程 shutdown clone 后 300ms 仍存活，但 writer 尚未结束；日志中的“final source handle”命名不准确，不能当作最后句柄已释放。
它仅证明 shutdown 调用后当前传输未在 3 秒内退出，不证明 native 全部句柄已关闭，也不证明任意改事件掩码或 FD_CLOSE 就可修复。
生产 Windows readiness 与本轮基线字节一致；新测试暴露原有缺口，而非本轮改写了该生产路径。
原 shutdown 测试只到首个 from_native=true/close_writer=true 分支就失败，不能称四分支均失败。
提交 120492ca 已将强见证改为可停止的非阻塞 writer，在 drop(native) 后 ack、join 并 drop 最后 clone，然后开始原 3 秒窗口。
Mac 此前提修正实测 1/0/0。
Windows 最终运行 41386261217 已记录：真实底层 8 次连续 WouldBlock、5 个完整源帧、all source handles released=true，原 3 秒关闭断言仍失败。
之后 FIONREAD 仍为 131072，mask0 无事件，RDNORM 只有可读；这次已排除 source 句柄仍存活的前提混淆。
控制命令 50/1/0，失败只有该强见证；原 shutdown 测试本轮通过，但前轮 FAIL 未经生产修复，不能据此宣称其可靠性已收束。
Runtime 控制 11/0/0、identity 1/0/0、OpenCode transport 1/0/0、OpenCode runtime 1/0/0；均无忽略。
OpenCode Windows 日志实际记录门闩期间 Answered+pending=true，放行后同预算联合观察 pending=false。
针对已复现的完整 close 失败，下一批必须验证 CLI 可支持的控制/生命周期通知或传输方案；不能无限读取积压、丢弃合法帧或放宽断言。

## Mac 16 项逐项前后对照

前轮完整为 3419/16/52，原始日志保留于 `docs/quality/evidence/windows-feasibility-20261101/41345708985/local/macos-final-workspace.log`。
本轮 A 为 3428/14/52、退出 101；B 为 3443/0/52、退出 0。
A 的源码可由 snapshot JSON 所列基准加 patch 重建，B 为最终生产源码。
随后只补关闭前存活正断言，定向 1/0/0；再改跨平台测试目录，定向 6/0/0，包含 4 项 support 测试。
随后完整 source-close 前提修正又定向通过 1/0/0；三次增量都不冒充另一轮完整 workspace。

| 测试 | 前轮完整 | 本轮A | 本轮B | 归因及处理 |
|---|---|---|---|---|
| `app::brain_host::context::proposal_tests::brain_proposal_context_failed_save_does_not_restore_or_redispatch` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shell_tests::test_dialogue_listening_maps_bottom_label_to_continuous_listening` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shell_tests::test_dialogue_start_fails_cleanly_when_mic_permission_denied` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shell_tests::test_dialogue_stop_returns_idle_but_models_remain_resident` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shell_tests::test_dialogue_unclassified_speech_does_not_truncate_assistant_reply` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shortcut_tests::test_continuous_listening_error_shortcut_dismisses_feedback_without_stopping` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shortcut_tests::test_continuous_listening_shortcut_stops_an_active_session` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shortcut_tests::test_dialogue_turn_failure_recovers_for_next_utterance` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::app_dialogue_shortcut_tests::test_voice_shortcut_interrupts_assistant_reply_during_speaking` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::reply_stream_backlog_tests::test_active_capacity_exhaustion_preserves_visible_reply_and_drains_accepted_prefix` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::reply_stream_backlog_tests::test_one_delta_with_nine_segments_does_not_fail_bounded_reply_queue` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::reply_stream_backlog_tests::test_slow_provider_drains_forty_segments_before_finish_without_truncation` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::reply_stream_cancellation_tests::test_provider_failure_overrides_capacity_warning_and_cleans_up` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `app::dialogue::reply_stream_cancellation_tests::test_user_interruption_discards_unsent_backlog_and_queued_pcm_together` | FAIL | FAIL | PASS | 前批已有相同失败，本轮App语言/测试锁相关源码未改；A仍失败、B通过，不能宣称隔离问题修复。 |
| `opencode_replies_only_through_own_instance_and_original_request` | FAIL | PASS | PASS | 既有Mac HTTP夹具继承非阻塞导致errno35；accept后显式恢复blocking，生产HTTP客户端未改。 |
| `owned_opencode_host_routes_current_question_through_runtime` | FAIL | PASS | PASS | 测试过早断言：消费事件与HTTP重核对是不同里程碑；确定性门闩复现，并共同等待，生产权限代码未改。 |

14 项 App 语言/测试锁源码与本轮基线字节相同，且前批有相同完整失败。
A 仍复现，B 通过，因此它们继续归为既有隔离问题，未宣称修复。
2 项 OpenCode 夹具均在 A、B 完整运行实际通过，不只依靠隔离重跑。
Mac 工具链 Rust 1.93.1，Windows 为 Rust 1.91.1，环境分别记录。

i18n 仍在未改的 `crates/app/src/settings/voice/view/gpt_live.rs:70` 失败。
完整 workspace 的 52 个与 Windows 语音的 16 个 ignored 为既有项，无新增忽略。
本轮没有可证实的新增 Mac 产品回归；既有不稳定与真实 App 未验仍阻止 AC11 完成。

### 输入锁症状的确定性复现

消费事件与 HTTP pending 重核对是两个里程碑。
RED 测试在消费后暂停第二轮 GET，实际记录 `state=Answered, input_pending=true`，旧即时解锁断言在 0.48 秒失败。
修复仅在原 5 秒预算内联合观察“精确消费且输入不再 pending”；门闩期间正断言输入仍 blocked。
生产客户端、宿主权限代码及输入闸门未改，唯一 POST、原 session 和回包丢失不重发断言保留。

### HTTP 夹具的 Mac 非阻塞继承

原 accepted Socket 继承 listener 非阻塞状态，read_line 可能先于 HTTP 数据到达而返回 errno35。
本轮显式恢复 blocking，并保留既有读取时限；生产 OpenCode 客户端未改。
两个测试使用生命周期覆盖 worker/runtime 的绝对临时目录，并保留空格。
query 解码后必须恰好一个 directory 且等于原目录，未弱化归属。

## 后续迁移顺序与未关闭门槛

1. 解决 Windows relay 在真实输出背压、native 未读积压下的关闭通知；保留有界内存与慢读语义。
2. 实现 App 生命周期、Brain 与内容打开/保存的 Windows 平台边界，并补对应 RED。
3. 实现手机 reactor、地址枚举、QR、Push token、SQLite 身份边界。
4. 实现 Chrome 认证与 Wry/WebView2 原生后端；保留 Browser 产品能力，默认 App 编译必须通过。
5. 收束 Mac 语言/全局锁隔离，再执行 ConPTY、权限、会话恢复和整树退出矩阵。
6. 默认与 release 构建通过后，实际验收真实 Norma GUI、获授权真实 Agent 和同一候选包。

不把 runner 图形样例、协议夹具、独立适配测试或 CLI version 当作产品 GUI/Agent 验收。
只使用标准 windows-2025 runner；没有启用付费 runner、复制凭据或发起真实模型任务。
本分支交付 Draft PR；不合并主线，迁移 Goal 不标完成。

## 原 116 条逐项清单

下面位置取自原日志；当前源码行号可能因拆文件变化。实际新诊断全文保存在新运行的编译日志与 JSON。

| ID | 原位置 | 错误码 | 根因 | 本轮状态 |
|---|---|---|---|---|
| APP001 | `crates/app/src/app/browser/chrome_auth_broker.rs:11:14` | E0433 | Chrome 认证端点与权限 | 仍失败 |
| APP002 | `crates/app/src/app/browser/chrome_auth_broker.rs:12:14` | E0433 | Chrome 认证端点与权限 | 仍失败 |
| APP003 | `crates/app/src/app/browser/safari_identity.rs:1:5` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP004 | `crates/app/src/app/browser/safari_identity.rs:2:5` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP005 | `crates/app/src/app/browser/safari_identity.rs:3:5` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP006 | `crates/app/src/app/browser/wry_backend.rs:10:5` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP007 | `crates/app/src/app/browser/safari_identity.rs:5:5` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP008 | `crates/app/src/app/browser/wry_backend.rs:11:5` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP009 | `crates/app/src/app/browser/wry_backend.rs:17:5` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP010 | `crates/app/src/app/browser/wry_backend.rs:345:9` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP011 | `crates/app/src/app/browser/wry_backend.rs:346:9` | E0432 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP012 | `crates/app/src/mobile/qr.rs:3:5` | E0432 | 二维码与自动释放池 | 仍失败 |
| APP013 | `crates/app/src/mobile/push/relay.rs:3:25` | E0433 | Push token 私有权限 | 仍失败 |
| APP014 | `crates/app/src/mobile/qr.rs:4:5` | E0432 | 二维码与自动释放池 | 仍失败 |
| APP015 | `crates/app/src/mobile/server/connection/watch_wait.rs:6:9` | E0432 | 手机 reactor 事件注册 | 仍失败 |
| APP016 | `crates/app/src/mobile/server/connection/image_store.rs:6:9` | E0433 | 图片私有文件 | 实测消除 |
| APP017 | `crates/app/src/mobile/server/connection/watch_wait.rs:10:17` | E0432 | 手机 reactor 事件注册 | 仍失败 |
| APP018 | `crates/app/src/norma/transport.rs:7:14` | E0433 | App IPC 租约与端点身份 | 实测消除 |
| APP019 | `crates/app/src/norma/transport.rs:8:14` | E0433 | App IPC 租约与端点身份 | 实测消除 |
| APP020 | `crates/app/src/norma/transport.rs:6:14` | E0432 | App IPC 租约与端点身份 | 实测消除 |
| APP021 | `crates/app/src/preview/content_copy.rs:3:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP022 | `crates/app/src/preview/local_content.rs:10:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP023 | `crates/app/src/preview/terminal_route.rs:5:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP024 | `crates/app/src/workspace/desktop/drawers/content/preview_load.rs:8:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP025 | `crates/app/src/workspace/desktop/drawers/content/workbench.rs:5:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP026 | `crates/app/src/workspace/desktop/drawers/content/workbench/controller.rs:7:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP027 | `crates/app/src/workspace/desktop/drawers/content/workbench/sidebar.rs:19:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP028 | `crates/app/src/workspace/desktop/drawers/content/workbench/sidebar_ops.rs:7:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP029 | `crates/app/src/preview/local_content.rs:11:5` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP030 | `crates/app/src/workspace/desktop/drawers/content/workbench/view.rs:6:16` | E0432 | 内容打开与保存平台边界 | 仍失败 |
| APP031 | `crates/app/src/app/brain_host/config.rs:375:21` | E0433 | Brain 符号链接与本地时间 | 仍失败 |
| APP032 | `crates/app/src/app/bootstrap.rs:175:34` | E0433 | App 原生生命周期成员 | 仍失败 |
| APP033 | `crates/app/src/app/browser/wry_backend.rs:40:52` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP034 | `crates/app/src/app/browser/wry_backend.rs:61:24` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP035 | `crates/app/src/app/browser/wry_backend.rs:81:52` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP036 | `crates/app/src/app/browser/wry_backend.rs:115:21` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP037 | `crates/app/src/app/browser/wry_backend.rs:116:21` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP038 | `crates/app/src/app/browser/wry_backend.rs:142:33` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP039 | `crates/app/src/app/browser/wry_backend.rs:230:26` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP040 | `crates/app/src/app/browser/wry_backend.rs:255:13` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP041 | `crates/app/src/app/browser/wry_backend.rs:279:21` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP042 | `crates/app/src/app/browser/wry_backend.rs:296:13` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP043 | `crates/app/src/app/browser/wry_backend.rs:299:13` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP044 | `crates/app/src/app/browser/wry_backend.rs:302:13` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP045 | `crates/app/src/app/browser/wry_backend.rs:313:13` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP046 | `crates/app/src/app/browser/wry_backend.rs:314:23` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP047 | `crates/app/src/app/browser/wry_backend.rs:323:60` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP048 | `crates/app/src/app/browser/wry_backend.rs:324:50` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP049 | `crates/app/src/app/browser/wry_backend.rs:325:53` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP050 | `crates/app/src/app/browser/wry_backend.rs:328:23` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP051 | `crates/app/src/app/browser/wry_backend.rs:341:5` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP052 | `crates/app/src/app/browser/wry_backend.rs:361:19` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP053 | `crates/app/src/app/browser/wry_backend.rs:362:15` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP054 | `crates/app/src/mobile/controller/work.rs:192:16` | E0433 | 二维码与自动释放池 | 仍失败 |
| APP055 | `crates/app/src/app/brain_host/activity_feed.rs:292:27` | E0433 | Brain 符号链接与本地时间 | 仍失败 |
| APP056 | `crates/app/src/app/background_project.rs:197:14` | E0599 | App 原生生命周期成员 | 仍失败 |
| APP057 | `crates/app/src/app/background_project.rs:198:14` | E0609 | App 原生生命周期成员 | 仍失败 |
| APP058 | `crates/app/src/app/browser/chrome_auth_broker.rs:143:64` | E0599 | Chrome 认证端点与权限 | 仍失败 |
| APP059 | `crates/app/src/app/browser/chrome_auth_broker.rs:168:66` | E0599 | Chrome 认证端点与权限 | 仍失败 |
| APP060 | `crates/app/src/app/browser/wry_backend.rs:89:44` | E0277 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP061 | `crates/app/src/app/browser/wry_backend.rs:97:34` | E0308 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP062 | `crates/app/src/app/browser/wry_backend.rs:191:74` | E0277 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP063 | `crates/app/src/mobile/interfaces.rs:19:18` | E0433 | 网络地址枚举 | 仍失败 |
| APP064 | `crates/app/src/mobile/interfaces.rs:35:58` | E0433 | 网络地址枚举 | 仍失败 |
| APP065 | `crates/app/src/mobile/interfaces.rs:54:48` | E0433 | 网络地址枚举 | 仍失败 |
| APP066 | `crates/app/src/mobile/interfaces.rs:25:22` | E0433 | 网络地址枚举 | 仍失败 |
| APP067 | `crates/app/src/mobile/controller/work.rs:67:40` | E0599 | 手机 SQLite 身份条件编译 | 仍失败 |
| APP068 | `crates/app/src/mobile/push/relay.rs:31:43` | E0599 | Push token 私有权限 | 仍失败 |
| APP069 | `crates/app/src/mobile/server/connection/watch_wait.rs:24:14` | E0599 | 手机 reactor 事件注册 | 仍失败 |
| APP070 | `crates/app/src/mobile/server/connection/image_store.rs:37:56` | E0599 | 图片私有文件 | 实测消除 |
| APP071 | `crates/app/src/mobile/server/connection/image_store.rs:125:14` | E0599 | 图片私有文件 | 实测消除 |
| APP072 | `crates/app/src/mobile/server/connection/image_store.rs:125:27` | E0433 | 图片私有文件 | 实测消除 |
| APP073 | `crates/app/src/mobile/server/connection/image_store.rs:125:46` | E0433 | 图片私有文件 | 实测消除 |
| APP074 | `crates/app/src/norma/transport.rs:41:36` | E0433 | App IPC 租约与端点身份 | 实测消除 |
| APP075 | `crates/app/src/norma/transport.rs:41:53` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP076 | `crates/app/src/norma/transport.rs:41:66` | E0433 | App IPC 租约与端点身份 | 实测消除 |
| APP077 | `crates/app/src/norma/transport.rs:41:82` | E0433 | App IPC 租约与端点身份 | 实测消除 |
| APP078 | `crates/app/src/norma/transport.rs:68:30` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP079 | `crates/app/src/norma/transport.rs:69:29` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP080 | `crates/app/src/norma/transport.rs:76:34` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP081 | `crates/app/src/norma/transport.rs:77:29` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP082 | `crates/app/src/norma/transport.rs:78:29` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP083 | `crates/app/src/norma/transport.rs:153:30` | E0599 | App IPC 租约与端点身份 | 实测消除 |
| APP084 | `crates/app/src/workspace/desktop/drawers/content/workbench/controller.rs:313:29` | E0277 | 内容打开与保存平台边界 | 仍失败 |
| APP085 | `crates/app/src/workspace/flow.rs:96:22` | E0599 | App 原生生命周期成员 | 仍失败 |
| APP086 | `crates/app/src/app/browser/native.rs:60:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP087 | `crates/app/src/app/browser/native.rs:98:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP088 | `crates/app/src/app/browser/native.rs:107:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP089 | `crates/app/src/app/browser/native.rs:118:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP090 | `crates/app/src/app/browser/native.rs:127:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP091 | `crates/app/src/app/browser/native.rs:136:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP092 | `crates/app/src/app/browser/native.rs:145:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP093 | `crates/app/src/app/browser/native.rs:154:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP094 | `crates/app/src/app/browser/native.rs:163:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP095 | `crates/app/src/app/browser/native.rs:172:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP096 | `crates/app/src/app/browser/native.rs:181:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP097 | `crates/app/src/app/browser/native.rs:198:15` | E0004 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP098 | `crates/app/src/app/brain_host/activity_feed.rs:289:15` | E0433 | Brain 符号链接与本地时间 | 仍失败 |
| APP099 | `crates/app/src/app/brain_host/activity_feed.rs:289:48` | E0433 | Brain 符号链接与本地时间 | 仍失败 |
| APP100 | `crates/app/src/app/brain_host/activity_feed.rs:290:20` | E0433 | Brain 符号链接与本地时间 | 仍失败 |
| APP101 | `crates/app/src/mobile/interfaces.rs:40:17` | E0433 | 网络地址枚举 | 仍失败 |
| APP102 | `crates/app/src/app/browser/safari_identity.rs:10:14` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP103 | `crates/app/src/app/browser/safari_identity.rs:11:6` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP104 | `crates/app/src/app/browser/wry_backend.rs:34:15` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP105 | `crates/app/src/app/browser/wry_backend.rs:38:50` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP106 | `crates/app/src/app/browser/wry_backend.rs:60:14` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP107 | `crates/app/src/app/browser/wry_backend.rs:73:10` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP108 | `crates/app/src/app/browser/wry_backend.rs:339:6` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP109 | `crates/app/src/app/browser/wry_backend.rs:344:52` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP110 | `crates/app/src/app/browser/wry_backend.rs:359:43` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP111 | `crates/app/src/app/browser/wry_backend.rs:360:5` | E0433 | 原生 Browser/WebKit/Wry 后端 | 仍失败 |
| APP112 | `crates/app/src/app/mod.rs:182:30` | E0433 | App 原生生命周期成员 | 仍失败 |
| APP113 | `crates/app/src/mobile/interfaces.rs:41:46` | E0433 | 网络地址枚举 | 仍失败 |
| APP114 | `crates/app/src/mobile/interfaces.rs:22:28` | E0433 | 网络地址枚举 | 仍失败 |
| APP115 | `crates/app/src/mobile/interfaces.rs:47:46` | E0433 | 网络地址枚举 | 仍失败 |
| APP116 | `crates/app/src/mobile/interfaces.rs:46:17` | E0433 | 网络地址枚举 | 仍失败 |
