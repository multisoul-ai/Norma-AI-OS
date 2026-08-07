# Keygate MVP Spike

可丢弃的可行性验证：本机 Docker 起 [Keygate](https://github.com/tabloy/keygate)，用脚本完成 **买断发证 → 首次联网激活 → 离线验签**。

依据：`docs/adr/0014-spike-keygate-before-app-licensing.md`、`CONTEXT.md`（Keygate 可行性 MVP）。

## 过关标准

1. Keygate 能自托管起来  
2. 能手动签发买断（perpetual）License，并可在 admin 看到列表/统计  
3. 脚本激活成功并落盘离线 token  
4. **断网**下本地验签通过（接受首次联网）  
5. 不接支付、不控设备（plan `max_activations=10000`）

时间盒：约半天（上限 1 日）。失败 → 停 Keygate，改试 Keygen CE。

## 快速跑 go/no-go

```bash
cd spikes/keygate-mvp
cp .env.example .env
# 填入两个不同的 64 hex secrets：
openssl rand -hex 32   # -> JWT_SECRET
openssl rand -hex 32   # -> LICENSE_SIGNING_KEY

chmod +x scripts/*.sh
./scripts/check.sh
```

`docker-compose.yml` 从 GitHub `tabloy/keygate@v0.1.1` **本地 build**（匿名拉 `ghcr.io/tabloy/keygate` 目前 401）。首次 `--build` 较慢。

成功打印 `GO: Keygate spike criteria met`。

## Spike 结论（2026-07-14）

**GO** — `./scripts/check.sh` 已通过：自托管 + 买断发证/admin stats + 首次联网激活 + 断网离线验签。

已知坑（spike 已绕过，薄接入 App 前要知情）：

| 问题 | 处理 |
|------|------|
| `ghcr.io/tabloy/keygate` 匿名 401 | compose 从 `github.com/tabloy/keygate.git#v0.1.1` 本地 build |
| Postgres 18 volume 布局不兼容 | 钉 `postgres:16-alpine` |
| `POST /setup/initialize` 500（`plans.checkout_id` NOT NULL 但 INSERT 省略） | `bootstrap.sh` SQL seed fallback |
| `jq -e` 读 JSON `false` 会 exit 1 | `lib.sh` `json_get` 只用 `jq -r` |

下一手：薄接入 App（粘贴 Key + 本地 token），见 ADR 0014。支付 / Path 2（港企 + Lemon Squeezy）仍延后。

## 分步命令

| 脚本 | 作用 |
|------|------|
| `scripts/up.sh` | `docker compose up -d` + health |
| `scripts/bootstrap.sh` | `setup/initialize` + `dev-login` + perpetual plan + create license |
| `scripts/activate.sh` | `POST /api/v1/license/activate` → `data/offline.token` |
| `scripts/offline_verify.sh` | 本地 Ed25519 验签（默认毒化 proxy 断网） |
| `scripts/check.sh` | 单测 + 上述端到端 |

Admin UI：<http://localhost:9000>（development 下也可用 OTP；脚本用 `dev-login`）。

## 单测（离线验签缝）

```bash
python3 -m venv .venv
.venv/bin/pip install -r requirements.txt
.venv/bin/python -m unittest discover -s tests -v
```

## 产物（gitignore）

- `data/state.env` — LICENSE_KEY、IDENTIFIER 等  
- `data/offline.token` — 激活返回的签名 token  
- `data/pubkey.hex` — `GET /api/v1/license/pubkey`  
- `.env` / `.venv` — 本地密钥与依赖  

## 薄接入 App（spike 过关后）

复用同一契约：首次 `activate` 联网 → 存 token+pubkey → 启动时 `offline_verify`；设置里粘贴 Key；可用 env 跳过门控。见 ADR 0014。

## 清理

```bash
docker compose --env-file .env down -v
rm -rf data .venv
```
