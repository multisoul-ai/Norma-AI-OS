# 硬门控放在工作台之外的 GateRoot

产品要求未激活不能进入工作台。我们决定：启动时用同窗 **GateRoot**（激活壳 | 工作台）做硬门控，仅在进入工作台（或 `NORMA_SKIP_LICENSE`）时才构造 `MultisoulApp`；授权纯逻辑放在零 GPUI 的 `license` crate，公钥内嵌、offline token 落盘、离线验签校验硬件 `did`。否决「在已创建的 MultisoulApp 上加 Activation 屏」——那样未激活仍会拉起 Runtime/PTY/agent 等基础设施，与硬门控语义冲突，也会让快捷键与生命周期到处打洞。
