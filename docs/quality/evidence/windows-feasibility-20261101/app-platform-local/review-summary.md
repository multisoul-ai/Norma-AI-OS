# 本轮独立只读审查

范围：本轮基线 1dd8965b 后的 App IPC/图片边界与测试、OpenCode 夹具及输出背压关闭见证。最终测试提交 120492ca。审查者为独立 subagent，未委派实现或编辑源码。

已核验并收口：

- 真实底层输出连续 WouldBlock 见证保留；关闭前 relay 存活是实际正断言。
- 陈旧探测保留身份，路径交换不能删除替代文件；Windows 通过 pinned HANDLE 删除。
- 请求慢滴与 UI 回执等待可取消；Drop 不再无界同步 connect。
- OpenCode 临时目录覆盖 worker/runtime 生命周期；解码后恰好一个原 directory；原 session、唯一 POST、404 不重试、输入门闩和回包丢失归属断言保留。
- 关闭见证最初仍有 blocking source writer 持有句柄，旧诊断“final source handle”命名不准确。现为非阻塞 writer，drop(native) 后 ack、join 并 drop 最后 clone，再开始原 3 秒窗口。

审查未把只读结论当成平台通过。最终 Windows 新强关闭见证仍失败；原 shutdown 本轮通过也不覆盖其历史 FAIL。App 98 条原生后端诊断、Mac 14 项既有语言/锁隔离和真实产品验收继续保持未关闭。
