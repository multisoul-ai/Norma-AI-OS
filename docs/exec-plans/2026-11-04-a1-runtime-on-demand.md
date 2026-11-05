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

## 初轮已有失败（后续已修复）

`bash scripts/test-packager-config.sh` 报壁纸原资源161,337,277 B超过146,800,640 B预算。
在未修改的主仓库 `dev` 上运行同一命令得到相同失败；本轮未修改该脚本或壁纸资源，也未放宽预算。
本轮新增默认profile回归与实际light staging单独通过，不能把既有全量资源预算失败记成通过。

## 后续收尾：真实体积与离线验收

用户明确允许构建 ad-hoc 本地测量包，明确选择本轮不发布 A1 资源。没有创建远端
Draft/tag、上传、公开发布、公证或合并 PR。以下仅为当前 PR 0.1.77 的本地测量，
不能作为公开 v0.1.82 的升级包或 Developer ID 正式发版结果。

壁纸超限根因是旧检查统计全部源文件，没有采用已经生效的可信 staging/profile。
现将默认 light 的实际随包原件、预览与 catalog 统一限制为 50 MiB（52,428,800 B）；
当前为 49,432,450 B。所有源仍先做白名单/长度/SHA 校验，超限在目录提升前失败，
保留旧输出；显式 full 继续可用。没有转码资源或放宽旧阈值吞掉失败。
先补预算失败回归，8/8 媒体准备测试及原打包检查通过。独立 review 找到 Bash 3.2
空数组回归，先复现再改为位置参数；light/full/wallpapers-on-demand 均通过。

| 本轮要求 | 证据与结果 |
| --- | --- |
| 完整 DMG | `./scripts/release-local.sh --ad-hoc`，Rust 1.91.1，87.14 秒，exit 0 |
| 实际压缩大小 | 116,132,191 B（116.13 MB / 110.75 MiB） |
| 安装占用 | 隔离复制普通文件 158,452,483 B；`du -sk` 为 158,695,424 B（158.70 MB） |
| DMG SHA-256 | `97d84780329863057b5eb0928119aabb84c46f8735a81f04759220281867db67` |
| 镜像/安装 | hdiutil、Finder 布局、deep/strict codesign 通过；114 文件与6链接复制一致 |
| 包内容 | 4程序、Sparkle、图标、boot/home、扩展齐全；28媒体匹配可信清单，16原件外置，无A1环境归档 |
| 损坏/取消 | 实际安装模块13/13；增强取消回归，旧指针/模型配置逐字节保留，重试不改模型配置 |
| 真实旧模型复用/离线执行 | 最终源码1/1，102.23秒；本机HTTP安装，禁网生产worker加载双模型并完成1秒PCM推理 |
| 媒体真实HTTPS | 16个原件完整匿名GET长度/SHA通过 |
| A1真实HTTPS | GET仍404，生产gate exit1；用户本轮不授权资源发布，未计线上通过 |

详细日志在 `artifacts/a1-closeout/`。真实离线测试编译实际安装模块并复用既有ASR依赖，
无GPUI壳；直接完整App测试因无关依赖重编及磁盘余量而终止，未计通过。实际验收的
源码/二进制执行期间SHA不变，保留二进制与源码副本，后续增强取消用例的重编结果另记。
复用已有完整App聚焦测试16/16及签名环境26,974文件/227原生签名证据；生产Rust未改。

离线推理只证明执行链，不覆盖准确度、麦克风或此DMG的GUI；未重新下载全新模型权重。
首次环境下载目前只有本机HTTP证据。A1公开地址、全新模型下载、新DMG GUI与正式签名/
公证仍待验收；当前钥匙串没有Developer ID，仅有Apple Development，ad-hoc不能作正式分发。
正式发版须同步目标代码与递增版本后重新构建验收，不能直接发布此旧版本测量包。

## 独立资源接通（2026-11-05 UTC）

用户后续要求“接通一下”，本轮仅公开独立A1资源，未发布主App、合并PR或开启自动合并。
`a1-runtime-v1` 已发布至 `multisoul-ai/norma-os-releases`，Release ID 为434752134，
target固定为 `05b4e4a73ed1da6ac97cfb2fbacd9c2238ee9732`。先Draft上传三项原件、
认证回读校验80.06秒通过，再以 `--latest=false` 公开；App latest仍为v0.1.82。

- 生产发布gate无本地参数，匿名HTTPS完整读取清单和408,602,523 B归档，长度/SHA通过。
- 公开SHA256SUMS匿名GET为200，159 B原字节与审核原件一致。
- 新增独立真实HTTPS安装用例，使用测量App原始描述及空安装目录，1/1通过，239.38秒。
  真实下载、环境校验/安装、旧权重复用、原配置重开与离线cached环境复用均通过；
  用户原配置SHA前后相同。该用例不加载模型推理，不当作GUI或全新模型下载证据。
- 原安装/取消/损坏用例13/13，另2项真实用例默认忽略；原完整推理用例的所有断言保留。
- 此轮先执行的完整HTTPS+推理用例在最后Finish报SessionClosed，296.22秒，未计通过；
  当时系统内存压力为WARN=2，生产资源watcher会释放模型。失败日志和源码/二进制原样
  保留，未绕过保护。此前102.23秒的独立离线推理通过记录仍按其原始范围引用。

新安装验收与原推理测试共用真实安装断言并分开命名，独立review确认没有跳过下载、
删除推理断言或丢失临时目录生命周期。明细位于资源审阅目录的
`evidence/connect-publication.json`、`evidence/connect-public-https-gate.log`、
`evidence/connect-https-install.json` 和 `evidence/full-inference-failed-connect-https-install.log`。

客户端固定URL/pins未修改，原116.13 MB测量包无需重新打包；A1远端不可用这一阻塞已解除。
主App正式签名/公证、递增版本、新DMG GUI和全新模型下载验收仍待后续处理。
