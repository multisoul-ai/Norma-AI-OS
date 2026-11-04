# A1 运行环境按需安装整改与验收

基准：`22c9526d`。用户要求完成后交叉 review 并提交 PR，明确不合并；本轮不上传独立资源、不发布 App。

## 结果与边界

主包移除完整 A1 环境归档，改为用户显式点击下载安装时获取预构建、已签名环境。
保留 393 B 固定下载描述与 1,345,706 B 无损压缩清单，后者解压为原样 5,762,156 B JSON，
用于旧受管安装的完整性校验和离线复用，不改变缓存版本前缀。原解压、文件校验和原子采用继续复用。
默认媒体改为 `light`，仍允许显式 `NORMA_MEDIA_PROFILE=full`。

- 下载只发生在显式安装入口；本地检查和启用已有环境不隐式联网安装。
- 可信描述固定 HTTPS 版本、清单长度/SHA、依赖指纹、平台与最低系统版本。
- 下载归档校验长度/SHA，展开文件逐项校验后才执行解释器；HTTPS 下载拒绝降级重定向。
- 同版本安装串行执行，等锁时可取消；下载失败/取消保留旧配置和模型。
- 旧完整环境离线复用；环境升级后优先验证并复用旧模型，不重新下载 1.21 GB 权重。
- 主包不再调用 Python 环境构建器；正式打包先核验本地清单和实际远端资源，开发 gate 不冒充线上验收。

## 验收映射

| 要求 | 证据 | 结果 |
| --- | --- | --- |
| 不内置/不构建完整环境，默认轻量且可显式full | `scripts/test-a1-package.py` | 2/2 |
| 固定来源与真实资源字节完整性 | `scripts/test-prepare-a1-engine.py` | 4/4 |
| 两种包装与开发App保持核心资源 | `scripts/test-media-packaging.py`、`scripts/test-macos-dev-app-runner.sh` | 2/2及shell契约通过 |
| 媒体 staging、profile 与原子更新 | `scripts/test-prepare-release-media.py` | 7/7 |
| 下载、损坏、取消重试、旧环境离线、原指针保留 | `cargo test -p app --lib voice_a1` | 16 passed、1 ignored；忽略项另行真实执行 |
| 正式签名环境下载与真实旧模型复用 | `voice_a1_real_engine_migration_reuses_existing_models --ignored` | 1/1，52.31秒 |
| 受影响 UI crate | `cargo test -p ui` | 22 passed |
| 编译 | `cargo check -p app`；Rust 1.91.1 的 production-data release build | 通过 |
| 双语、行数、no-allow、指南、格式 | 对应约束脚本、fmt与diff检查 | 通过 |
| 真实App的中英文安装说明与浏览副作用 | `scripts/desktop-e2e.sh settings-a1-runtime-download --visual` | exit 0，2/2步骤，266.90秒 |

真实安装使用本机HTTP提供**原始正式签名资源**，读取原有真实权重；仅临时配置隔离，未加载声学模型或录音。
先红：安装入口重新下载模型时，测试立即取消并报 `Cancelled`；后绿：复用旧权重且无新模型包目录。
普通聚焦测试在子进程边界使用脚本解释器夹具，不当作真实语音识别证据。

真实App采用 Apple 反馈原则：保持既有组件、布局与键盘入口，仅明确首次下载、无需Python配置和离线使用，
不增加动效。中文Sapphire Night、英文Graphite均逐图核验，浏览不改变provider、不生成下载缓存或模型配置；
这两个主题均不记作浅色主题验收。

本次关键本地日志位于 `artifacts/a1-on-demand/`；真实App记录为
`artifacts/desktop-workspace-e2e/2026-11-04T17-07-28Z/report.json`，`verification=model-assisted`。
早先复用的广泛语音场景两次因目标被滚出视口得到ENV，未计通过；最终新增本次差异的最小场景，保留原场景不改。

## 交叉 review

两名独立 reviewer 分别检查运行时信任链/并发和包装/测试假阳性。
首轮发现旧安装缺少新缓存导致离线检查失败，以及完整安装入口重复下载模型；均先补失败回归，再修复。
最终增量对 gzip CRC、截断、同长度错误内容、超长展开和正确离线复用进行了独立反例验证，全部符合预期。
并发准备仅下载一次、等待锁时取消零HTTP请求的独立反例亦通过。最终无未解决的新增阻塞代码问题。

## 独立资源与发版前置条件

资源复用公开 `v0.1.82` 的 Developer ID 签名环境；App及解释器签名团队均核对为 `2BF8G9AN4L`。
清单 SHA-256：`b2d8a72c64d54366b0b41733804e9b4c65422d1250b8640bb897e3574a5f6ba1`。
归档 408,602,523 B，SHA-256：`16ac3f3ea3682b1b66d17cdea738621076bd208b7115f9b5bd7ec9950e4db378`。
待发布原件及校验文件在本工作区 `build/a1-runtime-release/`。

`a1-runtime-v1` 尚未对外发布，匿名请求当前为HTTP 404；正式 gate 实测exit 1，会拒绝生成正式新包。
发版前须经授权发布独立资源，保持 `--latest=false`，然后重跑匿名HTTPS完整字节校验。
没有把本机HTTP、截图、release编译或旧DMG当作该公开下载路径及新DMG体积的验收。

## 已有失败

`bash scripts/test-packager-config.sh` 报壁纸原资源161,337,277 B超过146,800,640 B预算。
在未修改的主仓库 `dev` 上运行同一命令得到相同失败；本轮未修改该脚本或壁纸资源，也未放宽预算。
本轮新增默认profile回归与实际light staging单独通过，不能把既有全量资源预算失败记成通过。
