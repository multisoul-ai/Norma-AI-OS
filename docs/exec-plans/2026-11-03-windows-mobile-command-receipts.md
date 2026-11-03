# Windows 手机命令并发回执

仅恢复用户授权的新增并发回执失败排查；其他 Windows 范围、真实模型验收、打包、发布和合并继续暂停。全部测试、编译与检查在 GitHub runner；本地只编辑、读取历史证据，正常提交 hook 照常执行。

## 验收映射

| 条件 | 验证入口 |
|---|---|
| 归因新增失败，区分源码/偶发/环境 | 原始两个完整 job 日志；同一 windows-2025 VM、Rust1.91.1，串行重复两个历史来源的未修改 mobile_commands 集成测试 |
| 不丢失、不重复、状态正确 | 正式 MobileCommandStore::reserve/get/finish；先捕获根因的 RED，再最小修复，验证唯一 New、其余 Existing、完成结果不可覆盖、重开身份和终态 |
| 稳定性 | 保留每次重复运行的源码、run/attempt、退出码、测试计数与完整日志，不用后续通过覆盖失败 |
| 无新增存储失败 | 修复后 Windows 完整 storage 与267通过/15失败逐项对照；macOS runner 验证受影响 crate；剩余失败保留完整名单 |

## 历史来源

- 基线 f232ac0b7af551d2bf3899dc60c6b91964f50559：[41611131460/job125832997701](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41611131460/job/125832997701)，完整 storage 199通过/82失败，手机命令3/0/0。
- 本轮 7cf61b94a846c750f6355e1465d1d4d275eb27b8：[41617015436/job125852886298](https://github.com/multisoul-ai/Norma-AI-OS/actions/runs/41617015436/job/125852886298)，完整 storage267通过/15失败；手机命令2/1/0，次线程 reserve 返回 DatabaseBusy/extended_code5。
- 两份原始日志均为 Windows Server2025 10.0.26100、windows-2025-vs2026 镜像20261030.294.1、Rust1.91.1/LLVM21.1.2；不同历史 VM 的地区与工作负载仍有差别。不能仅凭相同镜像断言原因是代码回归或环境。
- 新任务基于643502e19436cc3d942574ed992d4893f040c476独立工作树；旧候选包与此前证据不改写。

## 状态

正在建立同机历史对照；尚无根因结论，尚未修改生产代码。Server2025结果不作为Windows11真机验收。
