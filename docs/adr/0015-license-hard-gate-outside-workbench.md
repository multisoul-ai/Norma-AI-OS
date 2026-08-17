# 硬门控放在工作台之外的 GateRoot

产品要求未激活不能进入工作台。我们决定：启动时用同窗 **GateRoot**（激活壳 | 工作台）做硬门控，仅在进入工作台（或 `NORMA_SKIP_LICENSE`）时才构造 `MultisoulApp`；授权纯逻辑放在零 GPUI 的 `license` crate，公钥内嵌、offline token 落盘、离线验签校验硬件 `did`。否决「在已创建的 MultisoulApp 上加 Activation 屏」——那样未激活仍会拉起 Runtime/PTY/agent 等基础设施，与硬门控语义冲突，也会让快捷键与生命周期到处打洞。

2026-07-28 起，硬门控由 app crate 的 `licenses` Cargo feature 控制；关闭时启动决策在读取本地凭证前直接进入工作台，原 GateRoot、激活客户端与 License 领域代码继续编译和测试。该 feature 曾于 2026-07-29 恢复为默认开启；2026-08-11 起再次默认关闭，普通开发构建和发行打包均无需 License 授权。如需专用授权版，可显式使用 `--features licenses` 构建。发行打包命令额外使用 `--no-default-features`，防止未来默认 feature 变化误开启硬门控。
