# Runtime/Codex Windows 修复与 Mac 对照

原始失败：[41327614822](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41327614822)。
本批基线：`457e5823e0e79b8c3f6df76edb09f7f4eaeb78b6`。
最终Windows证据：[41345708985](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41345708985)，源码`e7a7a78a9a871748f28becf1a6de32702ce41cd9`，attempt1。

## 原六条诊断逐项定位

| Runtime源码位置 | 错误 | 根因 | 最小边界修复 | 本轮 |
|---|---|---|---|---|
| `crates/runtime/src/nodes/agent/codex.rs:5` | E0433 | std::os::unix导入 | 统一Listener别名，Windows接uds_windows | 原诊断消除；App仍FAIL |
| `crates/runtime/src/nodes/agent/codex.rs:4` | E0432 | native_questions/relay被平台cfg隐藏 | 开放真实模块；WinSock readiness与OS随机身份 | 原诊断消除；App仍FAIL |
| `crates/runtime/src/runtime/questions/codex_host.rs:4` | E0432 | native_questions缺失 | 沿用真实控制host与后台握手 | 原诊断消除；App仍FAIL |
| `crates/runtime/src/runtime/questions/codex_host.rs:5` | E0432 | relay缺失 | 保留ticket锁、回答资格、取消与失败状态 | 原诊断消除；App仍FAIL |
| `crates/runtime/src/runtime/questions/codex_turns.rs:5` | E0432 | relay缺失 | 保留turn与session归属事件 | 原诊断消除；App仍FAIL |
| `crates/runtime/src/live.rs:85` | E0433 | native_questions缺失 | 保留NativeQuestionHost生命周期 | 原诊断消除；App仍FAIL |

原始六行见 `docs/quality/evidence/windows-feasibility-20261101/41327614822/compile/app_compile.log`。

Windows使用uds_windows AF_UNIX，协议、owner/generation、ticket锁与原生TUI透传保持原实现。
阻塞握手只在后台进行，接受流明确恢复阻塞且保留5秒超时；随后恢复非阻塞relay。
随机身份使用已有ring OS CSPRNG；Unix保留/dev/urandom。
Unix的poll路径未改；Windows使用WSAPoll及其错误API，5ms取消服务预算保留。
这不是原生Codex CLI启动、权限端点或进程树验收。

## 验收映射与当前边界

| 验收要求 | 实际检查/测试 | 当前判定 |
|---|---|---|
| 原6条与默认App编译 | `compile/app_compile.log`、`compiler-blockers.json` | 原6条消除；App116条新前沿，FAIL |
| 请求响应与错误后继续 | codex_control_platform、codex_control_handshake | 实测结果见本轮control/control.log |
| 权限、会话与唯一交付 | identity/permissions/host/connections及Runtime11项 | 不用fixture冒充真实Agent |
| 部分写入、Ping与不重发 | partial_answer_ping_and_disconnect_keep_transport_ownership_accurate | 真实Socket+test-only写额度；Writing→Unknown/Sent原断言保留 |
| 背压双方向、关闭写端或读端 | backpressured_relay_closes_when_either_peer_exits_in_both_directions | 本轮PASS、41341784394为FAIL；未确定relay输出背压，可靠性UNRESOLVED |
| 语音与平台回归 | speech、platform、compile的原始报告 | 实际计数见索引；既有忽略保留 |
| Mac原15项与新增症状 | 三次完整workspace、前后逐项二进制、只读基线实验 | 完整FAIL；不能用隔离绿覆盖 |
| 真实GUI、Agent、候选包 | 尚无实际程序/模型/候选包证据 | NOT_RUN |

## Windows仍未关闭的阻塞

1. 41341784394在native→server写端关闭后relay未在3秒内退出；本轮相同生产readiness下关闭测试PASS。其外部writer首次WouldBlock不能证明relay输出已被背压，因此不能据此判已修复。partial三分支本轮通过，属于故障注入票据验证。
2. App116条诊断按域计数：Browser原生后端59、App IPC13、手机服务23、Preview/workbench11、生命周期/brain10。
3. App的macOS Objective-C/Wry/Safari、Unix权限/文件身份/poll、缺失Windows后端导致的match及生命周期成员，需要真实实现。
4. 生产launcher仍有POSIX Shell、/tmp短私有路径与权限语义；ConPTY整树退出、原生Codex TUI启动尚未验收。

错误码为61 E0433、22 E0432、16 E0599、1 E0609、3 E0277、1 E0308、12 E0004。
完整逐行诊断保存在每轮 `compile/app_compile.log` 与 `compiler-blockers.json`。
未禁用Browser、VAD、Sherpa、Codex、手机服务或其他产品能力。

SO_SNDBUF/SO_RCVBUF的设置及回读成功，不能保证provider实际采用该值。
41341784394双方读回4096仍把512KiB整段写入；该轮失败保留。
最后改用明确故障注入，同时保持独立真实OS背压测试。
微软文档要求读完数据后才发布FD_CLOSE，不能直接替换WSAPoll并声称旁路关闭已修复。
有限lookahead只移动边界；超时丢弃合法慢连接会改变会话语义，本轮未采用。

- [Microsoft Socket Options and IOCTLs](https://learn.microsoft.com/en-us/windows/win32/winsock/socket-options-and-ioctls-2)
- [Microsoft WSAEventSelect](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsaeventselect)

## Mac原15项逐项前后对照

“旧完整”指41327614822批次的3415/15/52；“前隔离”为未改457二进制，“后隔离”为15b完整workspace实际编译二进制。
每项隔离均真实执行一次，0忽略；457完整列直接取本轮修改前全仓日志。
没有以Cargo单crate重编译切换App feature图。
文件序号对应最终运行local子目录的macos-before-case-NN.log与macos-final-case-NN.log。

| 序号 | 测试（完整名称） | 旧完整 | 457完整 | 前隔离 | 后隔离 | 15b完整 | 归类 |
|---|---|---|---|---|---|---|---|
| 01 | `app::brain_host::context::proposal_tests::brain_proposal_context_failed_save_does_not_restore_or_redispatch` | FAIL | PASS | PASS | PASS | FAIL | 全局语言隔离/文案断言 |
| 02 | `app::dialogue::app_dialogue_shell_tests::test_dialogue_listening_maps_bottom_label_to_continuous_listening` | FAIL | PASS | PASS | PASS | FAIL | 全局语言隔离/文案断言 |
| 03 | `app::dialogue::app_dialogue_shell_tests::test_dialogue_start_fails_cleanly_when_mic_permission_denied` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 04 | `app::dialogue::app_dialogue_shell_tests::test_dialogue_stop_returns_idle_but_models_remain_resident` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 05 | `app::dialogue::app_dialogue_shell_tests::test_dialogue_unclassified_speech_does_not_truncate_assistant_reply` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 06 | `app::dialogue::app_dialogue_shortcut_tests::test_continuous_listening_error_shortcut_dismisses_feedback_without_stopping` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 07 | `app::dialogue::app_dialogue_shortcut_tests::test_continuous_listening_shortcut_stops_an_active_session` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 08 | `app::dialogue::app_dialogue_shortcut_tests::test_dialogue_turn_failure_recovers_for_next_utterance` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 09 | `app::dialogue::app_dialogue_shortcut_tests::test_voice_shortcut_interrupts_assistant_reply_during_speaking` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 10 | `app::dialogue::reply_stream_backlog_tests::test_active_capacity_exhaustion_preserves_visible_reply_and_drains_accepted_prefix` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 11 | `app::dialogue::reply_stream_backlog_tests::test_one_delta_with_nine_segments_does_not_fail_bounded_reply_queue` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 12 | `app::dialogue::reply_stream_backlog_tests::test_slow_provider_drains_forty_segments_before_finish_without_truncation` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 13 | `app::dialogue::reply_stream_cancellation_tests::test_provider_failure_overrides_capacity_warning_and_cleans_up` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 14 | `app::dialogue::reply_stream_cancellation_tests::test_user_interruption_discards_unsent_backlog_and_queued_pcm_together` | FAIL | PASS | PASS | PASS | FAIL | DIALOGUE_SHELL_TEST_LOCK 锁中毒连锁 |
| 15 | `opencode_replies_only_through_own_instance_and_original_request` | FAIL | FAIL | PASS | PASS | FAIL | Mac accepted socket继承非阻塞，HTTP夹具读到errno35 |

原14项App源码未改；其中2项语言断言与12项锁中毒与前批完整失败一致。
OpenCode accepted Socket的非阻塞继承由原生fcntl/recv探针复现errno35；夹具源码未改。
它们不是由隔离通过就修复，全量失败继续保留。

## Mac新增观察、回归与未归因项

| 症状 | 修改前后证据 | 处理/判定 |
|---|---|---|
| Codex blocker拿到session后尚未收到历史回执 | 9b9完整失败；旧457额外20次未复现 | 不能说是既有失败；修正测试在原3秒内等双里程碑；15b完整及受影响测试通过 |
| OpenCode foreground的Unsent/Unknown | 未改457原二进制组第3次复现相同断言 | 既有时序失败，不修改无关产品路径 |
| OpenCode host的AnsweredElsewhere/Answered | 未改457只读基线实际复现 | 既有状态症状；不能证明下面不同症状也是既有 |
| OpenCode host的“消费后输入锁解除” | 15b完整失败；旧457额外20次18通过、2次失败于上一状态断言，未复现此症状 | 归因未收束；下一轮对Worker核对窗口与消费事件共同等待做聚焦验证，不能标Mac无新增回归 |
| SQLite DatabaseBusy与Runtime foreground POST0/1 | 修改前完整workspace已失败 | 既有完整基线波动，保留原日志 |

完整基线3427/3/52；9b9完整3418/17/52；15b完整3419/16/52；退出码均101。
最后仅变更测试写额度后，norma/runtime为687/0/12，退出0；它不覆盖App完整失败。
全仓52项、该子集12项与Windows语音16项均为既有忽略；新增忽略为0。
Mac Rust1.93.1、Windows Rust1.91.1分别有版本日志，未把环境混为同一基准。
既有i18n失败在 `crates/app/src/settings/voice/view/gpt_live.rs:70`，原始报告继续保留。
物理音频、真实VAD模型、A1模型、真实Norma窗口均未作本轮通过声明。

## 后续顺序

先建立Windows关闭通知/传输的可行方案并保留慢读语义与有界内存，再处理App IPC及生命周期。
随后逐域实现手机/Preview/Browser Windows后端，重新要求默认App check通过。
同时收束Mac语言/锁隔离与OpenCode双里程碑时序，再执行ConPTY整树生命周期与完整存储恢复矩阵。
最后验收真实Norma窗口、获授权的两个真实Agent及同版本候选包。
标准runner边界不变，不启用付费机器，不复制本机凭据，不新建Secrets或提交模型任务。
