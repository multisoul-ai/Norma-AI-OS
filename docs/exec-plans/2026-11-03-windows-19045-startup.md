# Windows 10 19045 启动缺少 WinSock 入口

用户实际系统为Windows10专业版22H2，19045.6093；旧94e4bc83便携包在窗口出现前报ProcessSocketNotifications入口缺失。优先处理此真实启动阻塞，手机命令并发工作树保留现场，其他范围不恢复。全部测试/构建在GitHub runner；正常提交hook照常，不发布或合并。

## 证据与边界

- 旧包内层SHA256：87ee66f8985766bbc611e5d4530e9edd3533fa27f26777037ffd3fcdcde9eba7。
- 直接分析实际PE导入表：仅norma-os.exe硬导入ws2_32.dll的ProcessSocketNotifications；另三个exe没有该导入。旧包不包含ws2_32.dll替代文件。
- [微软API要求](https://learn.microsoft.com/en-us/windows/win32/api/winsock2/nf-winsock2-processsocketnotifications)：该入口最低Build20348；用户19045不具备。Server2025测试不能证明19045可启动。
- 来源位于crates/norma/src/codex/relay/transport/windows.rs；必须移除函数的静态导入，不能只在调用外做版本if。

## 验收映射

| 条件 | 证据 |
|---|---|
| 旧包真实回归RED | 新import检查在新Windows runner下载同一94包，dumpbin识别强制导入并失败 |
| 新包不再强制依赖此入口 | 同包四个exe真实导入检查；缺API分支测试 |
| 保留关闭/取消/提问/背压契约 | 真实AF_UNIX测试、relay/backpressure/取消测试，强制API不可用的实例级用例；不放宽断言 |
| release交付 | 固定修复SHA、新release ZIP及hash，新runner下载相同hash实际启动与中文创建重开 |
| Windows10真实验证 | 用户19045设备复测；Server2022/2025或强制缺API测试都不标为Win10真机通过 |

状态：根因已由实际导入表、用户系统版本与官方API下限确认；正在建立runner RED，尚未修改生产实现。
