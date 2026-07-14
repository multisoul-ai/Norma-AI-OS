# 先用脚本级 Keygate spike 验证授权可行性

在接入 Norma OS 主应用与支付之前，用本机 Docker + `spikes/keygate-mvp/` 下的脚本级客户端验证自托管 Keygate 是否满足买断手动发证、首次联网激活与之后离线复验。时间盒约半天（上限 1 个工作日）。过关后再薄接入 App（粘贴 License、本地激活态、可跳过门控）；no-go 则停 Keygate，下一手优先 Keygen CE。不在 spike 阶段接支付、控设备、做全功能门控或上云。

## Outcome（2026-07-14）

**GO.** `spikes/keygate-mvp/scripts/check.sh` 通过。已知需绕过的 Keygate v0.1.1 问题见 spike README（`setup/initialize` 的 `checkout_id`、GHCR 匿名拉取、Postgres 16）。下一决策：薄接入 App，不换 Keygen CE。
