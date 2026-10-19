# ZCode CLI 用户级技能目录调研

> 调研日期：2026-10-18。ZCode CLI 版本 `0.16.9`（源码 `zai-org/ZCode` 提交 `aac4755`，复用 [2026-10-18-zcode.md](2026-10-18-zcode.md) 的本地构建 `~/.zcode-probe`）。
> 用途：决定 Norma 能否把 `norma-orchestration`、`norma-presentation`、`ask-norma` 写进 ZCode 的用户级技能目录。

## 1. 结论

| 问题 | 结论 | 状态 |
| --- | --- | --- |
| 用户级技能目录 | `~/.zcode/skills/<name>/SKILL.md`（优先），`~/.agents/skills/<name>/SKILL.md` 也被发现 | 已验证 |
| 环境变量覆盖 | **无可用于技能目录的覆盖**。`ZCODE_DATA_BASE_DIR` 在源码中只用于数据基目录（默认 `homedir()`），实测它指向含 `.zcode/skills` 的目录时技能列表**不变**。隔离只能靠改 `HOME` | 已验证 |
| SKILL.md 格式 | `---` 包裹的扁平 `key: value` frontmatter；必需 `name`、`description`（≤1024 字符）；可选 `when_to_use`、`license`、`metadata` | 官方文档 + 部分实测 |
| 发现命令 | `zcode skills`（与 `zcode skills list` 输出相同），非 TTY、无需模型与登录 | 已验证 |

建议 Norma 写入 `~/.zcode/skills/<name>/SKILL.md`，frontmatter 带 `name` 与 `description`。

## 2. 官方文档依据

- 内置技能 `zcode-guide:diagnosing-skills`（官方插件缓存，运行时落在 `~/.zcode/cli/plugins/cache/zcode-plugins-official/zcode-guide/0.3.0/skills/diagnosing-skills/SKILL.md`）写明发现顺序：显式配置根 -> 用户 `~/.zcode/skills` -> 用户 `~/.agents/skills` -> 工作区 `.zcode/skills`（从 cwd 向上到仓库根）-> 工作区 `.agents/skills` -> 插件根。同层 `.zcode` 先于 `.agents`。以 `.` 开头的目录（`.system` 除外）与 `node_modules` 被跳过。
- 仓库 `docs/skills-product.md`、`docs/external-agent-skills-import.md` 与之一致（用户技能为 `~/.zcode/skills` 与 `~/.agents/skills`）。
- frontmatter：缺 `name` 或 `description`、或 `description` 超 1024 字符则**丢弃**；完全没有 frontmatter 则 `name` 取目录名、`description` 为空；仅读顶层标量，缩进键被忽略，多行用 `>` 或 `|`。

## 3. 隔离实测

方法：`mktemp -d` 得临时根 `$T`；`HOME=$T/home`，cwd=`$T/work`；PATH 使用 `~/.zcode-probe/env.sh`（只含探针 node 与 zcode 包装）。真实 `~/.zcode` 在测试开始前不存在（`ls ~/.zcode` -> `No such file or directory`），测试期间 HOME 已重定向，未触碰。

### 3.1 两个目录均被发现

放入 `$T/home/.zcode/skills/norma-orchestration/SKILL.md`（`description: zcode-dir test`）与 `$T/home/.agents/skills/norma-presentation/SKILL.md`。运行 `zcode skills </dev/null`：

```
Available skills (15)
...
- norma-orchestration (user/zcode)
  zcode-dir test
  /tmp/zcs.HbR2/home/.zcode/skills/norma-orchestration/SKILL.md
- norma-presentation (user/agents)
  agents-dir test
  /tmp/zcs.HbR2/home/.agents/skills/norma-presentation/SKILL.md
```

另 13 条是内置/系统/官方插件技能（首次运行自动安装到 `$T/home/.zcode/cli/plugins/...`）。

### 3.2 frontmatter 变体

- `description: >` 折叠多行：列出为 `line one line two`（已验证）。
- 无 frontmatter（只有正文）：列出 `nofm (user/zcode)`，描述为空（已验证）。
- 有 frontmatter 但缺 `name`（目录 `noname`）：**未出现在列表中**，被丢弃（已验证）。
- `description` 超 1024 字符丢弃：仅来自官方文档，未实测。

### 3.3 同名但非 Norma 写入的文件（仅记录）

在 `$T/home/.agents/skills/norma-orchestration/SKILL.md` 放 `description: FOREIGN agents copy`，同时 `.zcode/skills` 下已有 Norma 样式同名副本。`zcode skills` **两条都列出**：

```
- norma-orchestration (user/zcode)   zcode-dir test          .../home/.zcode/skills/...
- norma-orchestration (user/agents)  FOREIGN agents copy     .../home/.agents/skills/...
```

即身份按文件路径区分，不覆盖、不报错；按官方文档，加载时 `.zcode` 优先（先扫描）。**实测只验证了"都被列出"，未验证模型实际加载哪一份。** 反过来，若用户在 `~/.zcode/skills` 里有同名非 Norma 文件，Norma 覆写会改动用户文件，应在写入前检测并跳过或以标记识别。本次未改动任何真实用户文件。

### 3.4 ZCODE_DATA_BASE_DIR

`$T/data/.zcode/skills/norma-orchestration`（`description: from DATA_BASE_DIR`）存在，运行 `ZCODE_DATA_BASE_DIR=$T/data zcode skills`：列表仍是 `HOME` 下的 `zcode-dir test` 与 `FOREIGN agents copy`，**没有** `from DATA_BASE_DIR`。结论：该变量不影响用户技能目录。源码里该变量出现于 `options.env.ZCODE_DATA_BASE_DIR?.trim() ?? homedir()` 一类数据目录解析（`dist/zcode.cjs` 第 50450、187903、206899、590419 行），我没有定位到技能根的解析代码，因此"技能根固定取 `homedir()`"是由实测推断，不是读到的源码。

## 4. limitations

- 模型是否真的按技能调用 `norma` 命令行：**未验证**，没有模型凭据；只验证了技能被发现和列出。
- 同名冲突下模型实际加载哪一份：未验证（仅验证都被列出）。
- 描述超 1024 字符被丢弃：只读自官方文档。
- 隔离只能靠 `HOME`；这会让 ZCode 把其自身数据也写进该 HOME，所以 Norma 的真实写入路径必须用真实用户的 `~/.zcode/skills`，不能依赖环境变量重定向。
- 未测 TUI 内 `/` 菜单的 Skills 分组展示；未测工作区级目录。
- 结果仅针对 0.16.9 源码构建，桌面版 DMG 内的 `zcode` 命令未核实。
- 清理：`rm -rf` 被权限系统拒绝，**残留临时目录 `/tmp/zcs.HbR2`**（含 home/work/data，仅测试文件，需手动删除）；`~/.zcode-probe` 为上一轮调研的探针，保留未动。
