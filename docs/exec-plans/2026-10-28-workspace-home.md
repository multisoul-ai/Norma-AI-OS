# 工作空间首页：Combo B+D 实施与验收

## 推送时与远端 dev 集成（2026-10-28）

用户已授权合并及推送。远端13个新增提交包含主题一致性改造，与新首页产生3处冲突。本次仅完成集成和定向验证，不回退已验收的新布局，也不重新实施全仓功能。

- 首页保留用户明确指定的全局壁纸、66%深色玻璃、搜索面板与拆分结构；远端旧首页的固定背景和旧工具栏不恢复。
- 保留远端全局主题、目录/弹窗配色和共享顶部材质；将上游拆出的对比测试与新版首页材质断言合并，保留顶部主题对比检查。首页材质额外覆盖3个旧浅色夹具。
- 远端可选主题目录现为6个深色主题，旧paper-blue保存值会迁移为InkBlack；首页E2E第二主题改为Graphite并同步截图名/清单，不把旧浅色别名误报为真实浅色运行。
- 集成验证范围：首页/目录/顶部材质聚焦单测与正常提交钩子。此前截图仅作为合并前证据，合并后的主题目录已变化，不能沿用为新主题视觉通过；本轮不重复完整E2E或release。
- 主工作区保留其他任务未提交内容；集成和推送在独立工作树完成。命令输出与推送结果归档到 `artifacts/workspace-home-push-2026-10-28/`。
- 首次集成回归31通过、1失败：全局壁纸绑定仍被旧首页默认图覆盖。对比确认上一笔提交遗漏了主工作区中已验收的 `app/wallpaper_actions/sync.rs` 修复，并非远端新引入；本次补入相同修复，失败日志保留为 `merged-tests.log`。
- 补齐后 `cargo test -p app --lib -- desktop_home desktop_directory_ desktop_workspace_card desktop_surface_theme desktop_top_bar`：34 passed / 0 failed，7.40s；另跑 `cargo test -p app --lib surface_theme_tests`：3 passed / 0 failed。`git diff --cached --check origin/dev` 通过；设置页重复 `Effects` 匹配警告已确认存在于远端基线，不属于本次改动。


## 交叉审查修复计划（2026-10-28）

本轮仅修复两项 P2：其他输入框被首页搜索抢焦点；从面板按钮关闭搜索后被下一帧 Focus 事件重新展开。沿用已授权的 GPUI Root 输入测试与真实 desktop-e2e 两个验证入口，采用 TDD、gpui-actions/testing、apple-design 和 norma-desktop-e2e：保持输入焦点归属，关闭可逆且最终状态稳定，不新增动效或控件。

1. 新建独立首页焦点测试文件，先用真实Root键盘派发复现命令Dock输入被抢；补表单输入隔离，复用既有桌面导航阻断条件。原输入测试文件接近500行，只共享夹具，不继续堆测试。
2. 修正关闭测试假阳性：Tab到关闭/显示全部按钮，再Esc或Enter关闭；处理后续绘制/事件后断言仍关闭、焦点仍可操作搜索入口，重新输入/点击能展开。先观察失败，再实现最小焦点修复。
3. 在现有 workspace-home-design 场景补上述两条真实输入路径，深浅主题均覆盖；硬断言等待稳定状态，增加对应关键图，不使用 Computer Use。
4. 跑首页/目录相关聚焦单测、app check/debug/release、行数/no-allow/性能/i18n检查与定向E2E；仅查看新增状态图，其他视觉复用前次证据。验收后启动修复版，不提交，不操作用户数据库。

增量分诊：修复版首次真实 E2E 中，命令输入及两条 Tab→Esc 路径通过，鼠标点击关闭仍失败（`2026-10-29T05-00-44Z`，exit 1）。截图与源码核对点击命中关闭按钮；补鼠标点击聚焦回归，检查关闭与重叠搜索入口的事件传播，先观察失败再修复。此时中止仅属于本任务、尚未完成的 release 构建，待最终修复后重建；保留失败日志，不改弱 E2E 断言。 新增鼠标回归按预期失败（`mouse-close-red.log`，0 passed / 1 failed）；关闭按钮消费点击后，首页/目录回归通过（`app-tests-final.log`，32 passed / 0 failed，6.87s）。

## 交叉审查修复结果（2026-10-28 本地时间）

两类焦点问题已修复：⌘K命令输入及克隆表单不再被首页搜索抢焦点；按钮聚焦后Esc/Enter及鼠标点击关闭均稳定收起，后续输入或重新聚焦仍可展开。关闭按钮还消费点击，防止重叠搜索入口再次响应。单测夹具显式激活窗口，跨后续Focus/Blur事件断言，避免未激活窗口造成假阳性。

- `cargo test -p app --lib -- desktop_home desktop_directory_ desktop_workspace_card`：exit 0，32 passed / 0 failed，6.87s。
- 最终 `cargo check -p app` / `cargo build -p app`：均exit 0，4.88s / 40.53s；`cargo build -p app --release --jobs 2`也exit 0，5m57s。
- 行数、no-allow、performance护栏及相关Rust格式检查：均exit 0。i18n仍为既有Effects空翻译和GPT-Live硬编码失败，本轮未新增文案。
- `bash scripts/desktop-e2e.sh workspace-home-design`：exit 0，1/1，38步骤全部通过，185.02s。证据 `artifacts/desktop-workspace-e2e/2026-10-29T05-08-22Z/`。本轮未使用Computer Use。
- 已逐张查看深浅主题8张新增关键图，焦点与搜索最终状态符合；另发现浅色命令输入框文字对比偏低，未调整Dock样式，单列为未处理视觉问题。其余首页/壁纸/示意终端/四方向导航视觉复用下方前轮证据。
- 已启动通过E2E的debug预览：PID53806，本任务独立数据、socket和签名bundle；保留用户App及旧预览PID15871。复制二进制SHA-256逐个一致，签名验证exit 0。

完整红绿日志、命令结论、逐图矩阵、启动清单和未验证范围见 [本轮验收记录](../../artifacts/workspace-home-focus-fix-2026-10-28/verification.md)。头像仍需正式透明底素材，真实IME/系统减动效开关/英文真实UI/窄窗菜单/全仓测试未补验；本轮无提交。

## 前轮结果：E2E 补验通过（2026-10-28 本地时间）

首页、示意终端预览和目录四方向/⌘↓导航已完成。最后的焦点边框修正及搜索面板点击穿透修复均已纳入最终构建。真实 App 的 `workspace-home-preview`、`workspace-home-design` 以及减少动态效果模式均 exit 0；本轮没有使用 Computer Use。用户“非必要不使用 Computer Use”的偏好已写入 AGENTS.md 和 CLAUDE.md。

下方各节保留此前失败和降级尝试的历史，不代表当前状态。先前黑图、原生异常和签名阻塞本轮未重现；没有定位黑图链路根因，不能把恢复运行写成已修复其根因。

本轮修复与证据：

- E2E 夹具反复安装曾遗留关联记录；补失败测试后开启 SQLite foreign_keys，使连续深12→浅12→浅1夹具安装通过。日志 `e2e-fixture-red.log` / `e2e-fixture-green.log`。
- 搜索面板曾将空白处点击传给背后的卡片；补失败回归后为面板添加 `.occlude()`，验证点击不会打开背景空间。日志 `e2e-panel-click-red.log` / `e2e-panel-green.log`。
- E2E 增加转场结束等待、按下后绘制等待、旧列表末项可见断言，以及四方向回到首项后的实际⌘↓打开断言，避免仅凭截图或中间态判定通过。

| 最终命令 | 输出结论 | 日志（均在 `artifacts/workspace-home-followup-2026-10-28/`） |
| --- | --- | --- |
| `cargo test -p app --lib -- desktop_home desktop_directory_ desktop_workspace_card` | exit 0；28 passed，0 failed，6.29s | `e2e-panel-green.log` |
| `cargo check -p app` | exit 0；Finished dev，5.28s | `e2e-panel-check.log` |
| `cargo build -p app` | exit 0；Finished dev，47.94s | `e2e-panel-build.log` |
| `cargo build -p app --release --jobs 2` | exit 0；Finished release，10m41s | `e2e-panel-release.log` |
| `bash scripts/check-max-file-lines.sh` | exit 0 | `e2e-max-lines.log` |
| `bash scripts/check-no-allow.sh` | exit 0 | `e2e-no-allow.log` |
| `bash scripts/check-performance-guardrails.sh` | exit 0 | `e2e-performance.log` |
| guide-lines、相关文件 diff-check 与 rustfmt | 全部 exit 0 | `e2e-guide-lines.log`、`e2e-diff-check.log`、`e2e-format.log` |
| `bash scripts/desktop-e2e.sh workspace-home-preview` | exit 0；1/1，88.30s | `e2e-preview-settled.log` |
| `bash scripts/desktop-e2e.sh workspace-home-design` | exit 0；1/1，120.93s；30步骤通过 | `e2e-design-settled.log` |
| `MULTISOUL_REDUCE_MOTION=1 bash scripts/desktop-e2e.sh workspace-home-design` | exit 0；1/1，101.47s | `e2e-design-reduced-motion.log` |

预览/导航8张图、普通首页50张图已逐张查看；减少动态效果另检查5张关键图，相关范围符合。证据目录依次为 `artifacts/desktop-workspace-e2e/2026-10-29T03-35-53Z/`、`2026-10-29T03-46-13Z/`、`2026-10-29T03-48-48Z/`。逐图矩阵见 [本轮视觉验收记录](../../artifacts/workspace-home-followup-2026-10-28/e2e-visual-review.md)。预览场景早于最后搜索 `.occlude()` 修复，预览/目录实现没有变化，其证据复用；此前65项App相关回归、runtime导航2项、UI15项结果也保留。

验证边界：中文真实UI覆盖深浅主题×默认地球/Blue Horizon、0/1/12空间、搜索与卡片交互、重命名/删除持久化；环境变量启用的减少动态效果路径通过。未验证真实IME候选窗口、macOS系统减少动态效果开关、窄窗真实菜单、英文真实UI及全仓测试。临时头像仍需设计提供正式透明底素材。已有 `settings/content.rs` Effects 重复分支 warning、全仓 i18n 的 Effects 空翻译/GPT-Live 硬编码不在本任务范围，未改动。

最终启动：已把通过 E2E 的 `target/debug/norma-os` 复制到本任务 `preview-app/norma-os`，校验 SHA-256 一致，再经正常开发签名启动器启动。预览 App PID 15871，使用本任务 `manual-data` 的12空间隔离数据；没有结束用户App或操作用户数据库。启动日志与校验清单为 `e2e-verified-preview-launch.log` / `e2e-verified-preview-launch.json`。开发构建没有 Sparkle，且手机连接端口被占用；首页验收不依赖这两项服务。先前 `final-sha256.json` 指向旧release副本，当前启动版本以新的清单为准。

## E2E 优先补验（用户追加要求）

用户明确要求非必要不使用 Computer Use。本轮后续以真实 App desktop-e2e 为验收路径；现有脚本能表达的首页、菜单、搜索及键盘行为均不转为手动操作。

1. 检查 Metal 工具链、debug 二进制与 E2E 前置条件；仅使用隔离数据和 owned 进程登记，不结束用户 App。
2. 重现首页场景的黑图失败，检查控制通道截图与窗口渲染链路；失败仍按原退出码记录，不弱化 OCR 或后置断言。若需修脚手架，先补能失败的聚焦测试。
3. 跑 `workspace-home-preview`，覆盖真实节点数量/布局、深浅主题与两壁纸、四方向、跨屏滚动及⌘↓进入；跑 `workspace-home-design` 覆盖1/12空间、搜索、菜单与焦点。逐张查看对应PNG，记录视觉结论。
4. 复跑最后修正受影响的聚焦单测、check及三项护栏，保存本轮退出码和证据。E2E通过后使用正常启动器启动App供用户验收；环境阻塞则明确报告，不以Computer Use替代。

## 本轮追加：示意终端预览与目录键盘导航

历史状态（已由文首最新 E2E 证据补验）：两项追加功能完成后，此前65项相关回归与真实窗口定向检查通过，包含最后焦点描边修正的release构建也已通过。随后受限环境中曾出现GPUI测试原生异常退出、Metal工具链缺失、签名启动及Computer Use权限阻塞；这些历史失败仍按原结果保留。

1. 先补首页预览失败测试：保存快照也能显示示意卡；数量、相对位置、比例与真实节点一致；已加载会话优先，空会话不回退旧快照。再独立实现 `home/preview.rs`，使用原型深色外壳、色点及短线，不读取终端输出，不增加前台 IO。项目区域外框与相机锚点不变，空项目不虚构终端。
2. 先补所有工作空间网格键盘失败测试。按当前筛选后的实际网格左右循环、上下同列移动，缺格落到末项，边界保持；选中态与当前运行空间分开，离屏目标自动滚入视野。⌘↓ 复用可配置的 ExpandSelectedProject 命令，在目录中进入选中空间，在项目总览中保持原行为；更新设置里的中英文说明。搜索输入、菜单、表单、忙碌和进入动画期间不劫持键盘。
3. 增量自动验证：首页示意预览、真实项目预览、相机锚点、目录/项目键盘导航及快捷键测试；runtime 纯网格路由测试；app check；行数/no-allow/性能/i18n 护栏；更新 debug/release 构建。
4. 定向真实 App 验收：已有 workspace 场景补真实保存节点夹具，在深浅主题看默认首页示意布局；所有空间网格验证四方向、跨屏滚动和⌘↓打开所选空间。使用独立 E2E 产物与进程登记，保留用户正在查看的 App。失败如实记录，不以单测替代看图；完成后启动本次版本供用户查看。

设计原则沿用 apple-design 与本地 gpui-*：简化小尺寸内容提高可读性，真实几何保持空间记忆，明确选中描边且即时响应；不新增装饰动画，沿用已有减少动态效果策略。所有新增用户文案走 i18n。

追加验证记录：首次两项测试均按预期失败（示意节点缺失、目录选中环缺失）；实现后首轮6项中5项通过，示意预览/实际终端预览/相机定位通过，切换器默认输入焦点消费了方向键。已沿输入组件 action capture 接入同一导航路由，并补空输入⌘↓、非空输入隔离、响应式列数、菜单阻断及顶部安全区域回归。红灯证据：`artifacts/workspace-home-followup-2026-10-28/red-tests.log`。原始用户开发版继续保留，真实窗口目前已可由 Computer Use 读取。

### 追加真实窗口验收记录

- `bash scripts/desktop-e2e.sh workspace-home-preview`：exit 1，79.70s。仍在欢迎页等待失败，截图全黑，visualChecks=0；报告为 PRODUCT，不能改写为自动验收通过。证据：`artifacts/desktop-workspace-e2e/2026-10-29T01-18-51Z/report.json`。
- 降级 Computer Use 使用独立签名 bundle `artifacts/workspace-home-followup-2026-10-28/preview-app/Norma OS Dev.app` 和隔离数据库，没有结束用户原有三个 App。Cua 可正常读取真实画面；同一可见签名窗口经 debug capture 保存的图仍全黑，见 `artifacts/workspace-home-followup-2026-10-28/capture-black-light-home.png`，所以黑图不等于真实页面是黑色。自动截图路径问题未定位。
- Sapphire Night + 默认地球壁纸、12空间：首页依次看到真实3/5/6/3个示意节点。第一个项目左大右上下，窗口比例、位置与夹具一致，无真实输出文字。通过。
- 深色目录：↓→↓←↑ 回到第一张“Norma OS”的清晰3px选中环；再连按三次↓，“性能调优”完整滚入屏幕；⌘↓后标题为“性能调优 / 所有项目”，其余工作空间卡消失。通过。
- Paper Blue + 蓝色地平线壁纸、12空间：首页暗玻璃仍可读，空空间保持空预览，旧空间示意布局不变。搜索展开品牌/最近访问弱化；粘贴“官网”只剩“官网改版”且命中高亮；Esc关闭；Tab进入卡片菜单，Return展开“重命名/删除”，删除为语义红。通过。Cua的中文typeText没有送入文本，改用paste验证；不能据此宣称真实中文输入法候选窗口通过。
- 浅色目录：↓→选择“官网改版”，再三次↓、⌘↓进入“发布流水线 / 所有项目”。通过。
- 浅色 + 蓝色壁纸、1空间：真实三窗口预览和新建虚线卡整体居中，搜索显示1个空间。通过。
- 看图发现原透明按钮用实心BoxShadow模拟焦点环，会透过12%背景使省略号对比不足；改为固定3px透明边框，聚焦时只改变边框色，避免布局跳动。该修正已通过release编译，但最终看图受下述权限阻塞，尚未复核。

以上 Cua 画面在会话中已逐张查看；本轮保存到文件的 debug 截图仍为黑图，不能作为正向视觉证据。本轮视觉范围不包括真实输入法候选窗口、系统减少动态效果开关与所有主题/壁纸的笛卡尔组合；原头像仍需设计提供正式透明底素材。

### 追加自动验证记录

证据目录：`artifacts/workspace-home-followup-2026-10-28/`。以下不包含最后的按钮焦点描边修正；该修正的最终结果另行补记。

| 命令 | 结果 | 日志 |
| --- | --- | --- |
| `cargo test -p app --lib -- desktop_home desktop_layout desktop_directory_ desktop_overview_ desktop_navigation_ desktop_camera_ desktop_workspace_card live_preview_tests shortcuts` | 65 passed，0 failed | `app-tests.log` |
| `cargo test -p app --lib desktop_directory_arrows` | 1 passed，0 failed；含命令输入框隔离回归 | `directory-final.log` |
| `cargo test -p runtime --lib desktop_navigation` | 2 passed，0 failed | `runtime-tests.log` |
| `cargo test -p ui` | 7+2+6 共15项通过，doctest 0项 | `ui-tests.log` |
| `cargo check -p app` | Finished dev，1m 23s | `app-check.log` |
| `cargo build -p app` | Finished dev，7m 31s | `debug-final.log` |
| `cargo build -p app --release --jobs 2` | Finished release，17m 03s | `release-final.log` |
| `python3 scripts/check-i18n.py` | exit 1；既有 Effects 空翻译、GPT-Live 硬编码，本次快捷键文案无新增错误 | `i18n.log` |

不提交，不 stash，不清理共享 target。未跑全仓测试或整个 runtime 测试套件。

### 最后焦点修正：恢复后的最终结果

原 `focus-final-*` 会话在服务恢复后无法连接，日志没有完成标记，不计通过。重新执行后结果如下；本轮没有再修改产品代码。

| 命令/操作 | 最终结果 | 证据 |
| --- | --- | --- |
| `cargo build -p app --release --jobs 2` | exit 0；Finished release，3m 38s；包含最后焦点边框修正 | `resumed-final-release.log` |
| `cargo test -p app --lib -- desktop_home desktop_directory_ desktop_workspace_card` | 测试编译成功；运行27项，3项输出ok后原生foreign exception/SIGABRT，不能算通过 | `resumed-final-tests.log` |
| `RUST_BACKTRACE=1 cargo test -p app --lib desktop_directory_arrows -- --test-threads=1` | exit 101；单独导航用例同样在原生层SIGABRT，没有Rust断言失败 | `resumed-directory-diagnostic.log` |
| 直接运行上述已编译测试二进制的同一导航用例 | exit 134；同样foreign exception | `final-directory-binary.log` |
| `cargo check -p app` | exit 101；GPUI shader build报missing Metal Toolchain；`xcrun metal --version`也报告相同错误 | `resumed-final-check.log` |
| max-file-lines / no-allow / performance / 本轮新增文件与material的rustfmt检查 | 全部exit 0 | `final-max-lines.log`、`final-no-allow.log`、`final-performance.log`、`final-format.log` |
| 标准签名启动器 | exit 69；当前环境找不到可用Apple Development签名证书；没有修改签名策略 | `resumed-final-app.log` |
| 隔离数据直接启动已编译App | exit 134；系统服务连接无效，未确认有效窗口 | `resumed-direct-app.log` |
| Computer Use连接最新版 | 工具明确返回“Computer Use was not approved to use Norma OS Dev”；停止后续UI操作 | 本会话工具结果 |

因此不能声明最新版已启动或整体验收通过。原先两项功能的真实窗口证据仍保留；最后焦点修正、真实IME、系统减少动态效果、完整主题/壁纸组合未完成最终验收。原生异常的具体根因未定位，不将其归为已证实的产品缺陷或已证实的纯环境故障。

最终release副本位于 `artifacts/workspace-home-followup-2026-10-28/preview-app/norma-os`，与 `target/release/norma-os` 的SHA-256一致；清单见 `final-sha256.json`。已准备 `artifacts/workspace-home-followup-2026-10-28/launch-preview.sh`：在具备开发签名证书的正常终端用bash运行，即可通过现有启动器加载12空间隔离数据，不触碰用户真实数据库。脚本通过 `bash -n`，未绕过签名要求或工具授权。

## 设计依据与边界

- 用户提供的 Combo.dc.html 最终稿、ComboStates.dc.html 状态规范、ComboFew.dc.html 单空间稿。本会话没有 Claude Artifact read 工具，使用给定 scratchpad 本地源文件；冲突时以用户明确要求与最终 Combo 为准（卡片 66%、搜索 62%、品牌淡到 45%）。
- 首页直接调用全局 render_wallpaper_layer，仅增加轻微上下暗角；不改变壁纸选择与视频渲染链路。
- 0 个空间保留欢迎页；1–2 个空间居中加新建卡；更多空间最多显示最近四个（恰有 3 个时只显示三个，不虚构数据）。搜索覆盖全部空间。
- 保留现有后台创建、打开、重命名、删除确认和相机导航接口；只改呈现与首页交互状态。目录数据读取保持现有后台路径。
- 头像从品牌参考图的冷静格确定性裁切，保留发色、X 发夹、蝴蝶结；临时资产需设计替换为正式透明底头像。

## 技能落地

- apple-design：信息层级为问候→搜索→最近访问；单击直接打开；Input/Button 复用 ui::component；焦点可见、取消可逆，删除保留确认。
- emil-design-eng：搜索从胶囊原位展开，菜单锚定触发器；键盘和筛选即时响应，不等待动画；卡片仅上浮 2px，减少动态效果禁位移。
- gpui-elements/styling/actions/testing：本地未安装，已从规范指向的 cnwzhu/gpui-skills 拉取到 /tmp/norma-home-gpui-skills 阅读；以固定 0.2.2/0.5.1 源码和编译结果为准。
- TDD：沿用用户指定的 desktop_home/layout 单元测试 seam 与真实 App 场景，逐片红→绿；不把内部状态断言当作真实 App 验收。
- norma-desktop-e2e：新场景归 workspace 模块，隔离数据，默认虚拟显示器，不结束用户 App。

## 步骤与验收映射

| 步骤 | 实现 | 自动验证 | 真实 App 证据 |
| --- | --- | --- | --- |
| 1 | 固定等宽最近卡片、1–2 个居中、窄窗重排，预览/相机锚点一致 | desktop_layout：1440/1280/窄窗、0/1/2/12 个、预览边界 | 1 个与12 个、默认首页 |
| 2 | 拆 home 为品牌/材质、搜索、卡片；注册裁切头像；移除旧标题/分组/主按钮 | desktop_home：壁纸覆盖、品牌/搜索/最近布局；assets 加载 | 深浅主题×默认/另一壁纸 |
| 3 | 最近排序、运行中/7天/更早分组、折叠、Unicode 筛选高亮 | 纯目录模型：时间边界、运行优先、折叠、查询、选择边界 | 展开/搜索/无结果/显示全部 |
| 4 | 原位面板、35%遮罩、45%/20%降级；上下/Enter/Esc/直接输入/Tab | GPUI 输入路径：筛选、导航、取消恢复、无结果预填创建 | 键盘选择、关闭重开、创建 |
| 5 | 卡片单击、悬停浮起/阴影、隐藏菜单、键盘进入、重命名删除 | GPUI 菜单不冒泡打开、重命名/删除取消，减少动态效果 | 悬停/聚焦/菜单/删除确认 |
| 6 | 最小充分检查、真实窗口逐图判定、启动 App | app 聚焦单测 + cargo check；必要 release build；三项指定护栏 | workspace-home-design + lifecycle 回归 |

## 验收命令与完成标准

- 开工 df -h：可用约 40 GiB。复用当前工作树开发 target 以供 desktop-e2e；空间紧张时仅定向管理登记的任务缓存，禁止 cargo clean 共享 target。
- cargo test -p app desktop_home；cargo test -p app desktop_layout；补充受影响相机/导航测试；cargo check -p app；cargo build -p app；cargo build -p app --release。
- bash scripts/check-max-file-lines.sh；bash scripts/check-no-allow.sh；bash scripts/check-performance-guardrails.sh。
- bash scripts/desktop-e2e.sh workspace-home-design；按实际流程补 workspace-lifecycle。关键图全部读图，结果写为符合/不符合/不确定。
- 深/浅色，默认壁纸/至少另一壁纸，默认/搜索展开/1个/12个，悬停/按下/聚焦/菜单，减少动态效果均有证据。自动化覆盖不到时使用真实 App computer-use。
- 未通过和未验证分别记录；最后启动本次构建供用户验收。不提交。

## 上一轮历史结果记录

### 实现与审查

- 主页面拆到 `crates/app/src/workspace/desktop/home/`，分别维护品牌、材质、搜索数据、输入、菜单、动效和欢迎页；主文件低于500行。
- 全局壁纸直出，淡上下暗角；品牌头像与状态问候；640宽搜索入口；最近四张252×204卡片；1–2个空间追加新建槽；0个保留欢迎页。
- 搜索支持分组、折叠、Unicode匹配高亮、无结果预填创建、方向键/Enter/Esc；背景控件在搜索展开时退出Tab序列，面板内正反Tab可循环。
- 卡片与菜单复用既有打开、重命名和删除确认流程；菜单使用窗口安全边距并独立绘制，焦点项与箭头选择项分别处理，避免Tab到删除后误执行重命名。
- 采用Material状态层与3px焦点环；低频面板展开260ms、背景200ms、菜单150ms、卡片上浮2px，减少动态效果时取消位移/过渡。关闭搜索即时收起，防止清查询后退出列表短暂增高。
- 独立对抗审查已核验并修复：Tab停靠与面板焦点边界、菜单焦点误触、测试遗漏KeyUp、相机锚点/排序/滚动、非最近项打开测试假阳性，以及对比度断言。
- 新增从Root派发的IME Esc回归测试后，复现了首次Esc误关搜索（32通过、1失败）。进一步加入Esc前置断言，明确复现重绘把“工作”覆盖为空：Input组合更新只notify、不emit Change，旧查询回写会提前清掉组合文本。现同步查询时跳过marked范围；搜索面板另通过action capture消费用于取消组合的Esc，避免Input先unmark并传播后再次触发首页关闭。第二次正常Esc仍关闭面板。独立审查确认这两层修复无需额外状态，且隔离在搜索输入路径；修复后结果见最终记录。
- 更早组键盘测试的失败最终定位为夹具竞态：返回首页异步刷新覆盖只在内存设置的旧时间。夹具现持久化到隔离SQLite，并断言刷新后仍是4条旧记录、默认显示2条。

### 真实 App 尝试与阻塞

- 新增 `workspace-home-design` 场景及配套夹具/看图清单，计划覆盖深浅主题×地球/蓝色壁纸、12/1/0个空间、默认/搜索/悬停/按下/聚焦/菜单、重命名/删除/重启。场景已通过Swift编译，但没有完成流程验收。
- 首次通过 `scripts/desktop-e2e.sh workspace-home-design` 运行时发现虚拟显示器不支持1440×900，场景已改1440×840。
- 第二次默认入口遭遇另一E2E任务共用owned登记而结束本任务App；日志已确认该原因。之后改用独立登记目录的同一driver，未结束用户App。
- 独立运行 `target/desktop-e2e-spm/debug/norma-e2e run workspace-home-design target/debug/norma-os artifacts/desktop-home-design-e2e`，普通/foreground/visible模式均在欢迎页等待阶段失败：截图全黑、OCR无文字。最终报告为product失败，原因尚未确定，不能改写为已证实环境问题。
- 最终debug构建完成后，再运行同一隔离场景：`target/desktop-e2e-spm/debug/norma-e2e run workspace-home-design target/debug/norma-os artifacts/desktop-home-final-e2e`。exit 1，30.37s，仍在欢迎页OCR等待失败。
- 最终失败证据：`artifacts/desktop-home-final-e2e/2026-10-28T18-39-36Z/report.json`、`failure-1793212806.png`、`app.log`。已经实际读图，整张全黑；报告product失败、visualChecks=0。深浅主题、两壁纸和交互画面均未验收，原因仍未确定。
- 最终Computer Use降级：将12空间测试数据库备份到独立 `artifacts/workspace-home-verification-2026-10-28/manual-data/`，通过 `scripts/run-macos-dev-app.sh target/debug/norma-os` 签名启动最终开发版；`cua.getApp` 使用完整bundle路径仍报 `Computer Use server error -10005: timeoutReached`，未获得有效画面。记录见 `computer-use.txt`、`manual-app.log`。已向用户询问桌面解锁/窗口显示情况，尚无确认。
- `2026-10-28T17-37-45Z/data/app.sqlite` 后来复用为手动12空间夹具；该次原截图/日志未变，但SQLite不能再作为失败当时的原始快照。
- 本任务手动/测试App均已停止；保留用户原有App。未达到“验收通过后启动供用户验收”的条件，不能宣称已完成最终启动。

### 限制与后续验收

- 头像 `crates/app/assets/icons/norma-calm.png` 是品牌参考图冷静格的临时裁切，保留发色、X发夹和蝴蝶结，仍有浅色底；需要设计提供正式透明底头像。
- 真实输入法候选窗口、动效衔接、悬停与相机锚点连续性、系统减少动态效果、全部主题/壁纸组合须在有效真实窗口上继续验收。
- 共享工作区其他改动产生 `crates/app/src/settings/content.rs` 重复Effects分支warning；全仓i18n检查还有Effects空翻译、GPT-Live硬编码和workbench规则测试失败，均非本次首页范围，未用allow抑制、未越界修改。
- 自动验证最终命令、退出码和日志见下节；任何单测/静态检查均不替代以上视觉验收。

### 自动验证最终记录

证据集中在 `artifacts/workspace-home-verification-2026-10-28/`，并保存首页源码SHA-256清单。以下为本次行为相关验证，未运行全仓测试。

| 命令 | 结果 | 日志 |
| --- | --- | --- |
| `cargo test -p app --lib -- desktop_home desktop_layout desktop_workspace_card desktop_camera desktop_workspace_entry desktop_top_bar embedded_assets` | exit 0；41 passed，0 failed；含完整Root IME与Tab/Menu回归 | `app-tests.log` |
| `cargo test -p ui` | exit 0；7+2+6共15项通过，doctest 0项 | `ui-tests.log` |
| `cargo check -p app` | exit 0；Finished dev，11.14s；有上述既有warning | `app-check.log` |
| `cargo build -p app` | exit 0；Finished dev，5m 50s | `debug-build.log` |
| `cargo build -p app --release --jobs 2` | exit 0；Finished release，12m 38s；之前主动停止的中间构建不计通过 | `release-build.log` |
| `bash scripts/check-max-file-lines.sh` | exit 0；主home.rs 264行，新增Rust文件均不超过500行 | `max-lines.log` |
| `bash scripts/check-no-allow.sh` | exit 0 | `no-allow.log` |
| `bash scripts/check-performance-guardrails.sh` | exit 0 | `performance.log` |
| `python3 scripts/check-video-wallpaper-performance.py` | exit 0 | `video.log` |
| 本次首页/资产文件 `rustfmt --edition 2021 --check` | exit 0 | `format.log` |
| 本次相关文件 `git diff --check` | exit 0 | `diff-check.log` |
| `swift build --package-path test/desktop-e2e --build-path target/desktop-e2e-spm` | exit 0，仅证明场景编译通过 | `swift-build.log` |
| 最终debug的隔离 `norma-e2e run workspace-home-design` | exit 1；product失败；欢迎页等待超时、截图全黑，visualChecks=0 | `e2e-final.log`、上述最终report.json |
| 最终签名开发版 Computer Use | timeoutReached，未取得窗口画面 | `computer-use.txt` |

红灯证据保留 `ime-root-red.log`、`ime-redraw-red.log`；视觉失败日志保留 `e2e-isolated.log`、`e2e-foreground.log`、`e2e-visible.log`、`e2e-final.log`。没有将黑图或窗口存在当作通过。
