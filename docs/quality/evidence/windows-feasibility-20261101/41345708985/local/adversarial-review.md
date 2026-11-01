# 本轮独立对抗审查

基线：457e5823e0e79b8c3f6df76edb09f7f4eaeb78b6。
审查者：独立 review_windows_probe；只读，未代替执行或制造 Windows 通过结果。

- 找到 Windows fresh_control_id 的 /dev/urandom 必败路径，补真实 OS UUID 回归后改用既有 ring OS CSPRNG；Unix 路径保留。
- 找到非阻塞 accepted socket 的分段握手问题；基线实际复现 WouldBlock 后恢复后台阻塞握手，5 秒超时保留。
- 核验 WinSock 句柄宽度、WSAPoll 事件和错误 API；Unix poll 未改。
- 找到两个测试假阳性：任意 read 错误冒充 EOF；计时推断背压。现均改为明确关闭结果和真实 WouldBlock，未放宽断言。
- Windows 首轮复验 41338252762 暴露 partial 夹具未进入 Writing；移至私有真实 Socket 资源构造边界，同时设置并读回发送/接收缓冲，原三分支正文与原 integration 测试一致。
- connect_inner 仍先生成随机身份，原路径只连接一次；权限、会话、握手、线程 flags 顺序保留，没有新 public 配置或测试 feature。
- CI 通过 --lib 实际运行迁移后的 partial 单测，未遗漏或新增 ignore。
- Runtime 夹具继续在原 3 秒内等待 session 身份与 SessionUsed 历史回执，原 WaitingQuestion、历史与阻塞断言保留。

上述发现与增量已由主 Agent 核对。审查没有宣称 Windows runner 已通过；以实际每轮原始日志为准。
生产 Windows launcher 的 POSIX Shell、短私有目录、权限及 ConPTY 后代清理仍未验，真实 Agent/GUI/候选包保持未验收。

## 实测反例后的增量审查

41341784394 证明双方 SO_SNDBUF/SO_RCVBUF 即使读回4096仍不足以强制512KiB答案部分写入。
已移除该夹具假设；e7a7a78a的test-only ControlledStream真实写出4KiB后注入WouldBlock。
生产cfg(not(test))仍直接使用UnixStream；正常路径连接一次；原Writing→Unknown/Sent与禁止重发断言保留。
新增budget==0正断言确认写出前缀，不把故障注入当作真实OS背压验收。
独立审查未发现该增量的阻塞；Mac norma/runtime 687/0/12实测通过。

真实Windows shutdown失败继续作为FAIL，不放宽3秒、不跳过分支。
微软WSAEventSelect文档要求读完积压才发布FD_CLOSE，不能把该API当作未经验证的修复。
微软Socket Options and IOCTLs文档说明provider可接受并回显缓冲选项而不使用设置值。
有限预读只移动缓冲边界；超时杀合法慢连接改变原语义，本轮未采用。
来源：https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-wsaeventselect
来源：https://learn.microsoft.com/en-us/windows/win32/winsock/socket-options-and-ioctls-2

Mac最终OpenCode host :215的输入锁断言不同于9b9的:210状态断言。
旧457基线复现了:210，但最终:215在额外20次精确对照中未复现；不能据此宣布:215是既有失败。
静态检查发现Worker::tick每次HTTP核对会先blocked=true，观察Answered后下一轮核对仍可能正在进行。
本轮未修改此OpenCode路径；受影响测试全过也不替代全workspace证据；:215归因继续未收束。

## 最新run与关闭覆盖缺口

41345708985控制job已实测PASS：控制50/0/0、Runtime11/0/0、身份1/0/0。
partial三分支均过，真实关闭测试本轮也过；必须如实保留此结果。
但生产readiness未改；shutdown夹具在外部writer第一次send/flush WouldBlock就发blocked信号。
这只证明writer→relay瞬时不能写，尚不证明relay→reader已WouldBlock或relay已暂停读。
单个512KiB帧可在relay尚未调度时触发该信号；立即关闭可能未覆盖前轮失败前提。
因此新PASS不能证明旧413417的关闭缺陷已修，可靠性仍为UNRESOLVED，不能盲重跑洗绿。
后续需在不增加生产公开配置、不改变慢读语义的前提下，为relay输出背压建立明确见证。
