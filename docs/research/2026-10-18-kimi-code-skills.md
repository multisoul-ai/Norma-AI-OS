# Kimi Code CLI 2.2.0 技能（skill）发现目录实测

日期：2026-10-18　CLI：`/Users/user/.kimi-code/bin/kimi`（`kimi --version` 输出 `2.2.0`）

## 结论

- 用户级技能目录：`$KIMI_CODE_HOME/skills/<name>/SKILL.md`，未设置时为 `~/.kimi-code/skills/`。
- 环境变量 `KIMI_CODE_HOME` 可覆盖数据根目录，技能目录随之移动（官方文档与实测一致）。
- `~/.agents/skills/` 也是用户级扫描目录，但固定在真实 OS HOME 下，不随 `KIMI_CODE_HOME` 变化。
- Norma 应写入 `$KIMI_CODE_HOME/skills/{norma-orchestration,norma-presentation,ask-norma}/SKILL.md`。
- 目录形式的 `SKILL.md` 必须同时有 frontmatter `name` 和 `description`，缺一则该技能被跳过（实测）。

## 官方文档依据

来源：`curl -sL https://moonshotai.github.io/kimi-code/en/customization/skills.md`、`.../en/configuration/env-vars.md`、`.../en/reference/slash-commands.md`。

- 扫描四层，优先级为 Project > User > Extra > Built-in。
  - 用户级：`$KIMI_CODE_HOME/skills/`（默认 `~/.kimi-code/skills/`）与 `~/.agents/skills/`。
  - 项目级：`.kimi-code/skills/`、`.agents/skills/`（项目根为向上查找到的最近含 `.git` 的目录）。
  - 额外目录：`config.toml` 顶层 `extra_skill_dirs = [...]`。
- 文档原文：Kimi 专属用户技能目录随 `KIMI_CODE_HOME` 移动，通用的 `~/.agents/skills/` 仍在真实 OS HOME 下。
- `KIMI_CODE_HOME`：覆盖数据根目录，默认 `~/.kimi-code`，配置、会话、日志、OAuth 凭据都落在其下。
- 目录形式 `<name>/SKILL.md`：`name`、`description` 必填。扁平形式 `<name>.md` 可省略。同一目录下 `<name>/SKILL.md` 与 `<name>.md` 并存时目录形式优先。
- 可选 frontmatter：`type`（`prompt` 默认 / `inline` / `flow`）、`whenToUse`、`disableModelInvocation`、`arguments`。
- 调用方式：斜杠命令 `/skill:<name>`；模型也可按 `description`/`whenToUse` 自动调用。
- `kimi --help` 有 `--skills-dir <dir>`：指定后只加载该目录，替代自动发现的用户和项目目录。没有 `kimi skills` 之类的列出子命令；`kimi --help` 的子命令只有 export、fork、provider、session、acp、web、rc、login、doctor、vis、migrate、upgrade 等。`kimi doctor` 只有 `config`、`tui` 两个子命令，不检查技能。

## 隔离实测

隔离方式：临时目录 `T=/tmp/kimiprobe.KWGT`，`KIMI_CODE_HOME=$T/home`，`HOME=$T/fakehome`，工作目录 `$T/proj`。未触碰真实 `~/.kimi-code`。因 `kimi` 没有非交互的列出命令，用 Python `pty` 启动 TUI（120x40），回车确认 "Trust this folder?"，再输入 `/skill:` 触发斜杠命令面板，剥离 ANSI 后截取输出。无需登录和模型（首屏提示 "Run /login or /provider to get started"，面板仍可用）。

### 1. 发现

放入 `$T/home/skills/norma-test/SKILL.md`（frontmatter 仅 `name`、`description`）和 `$T/home/skills/clash/SKILL.md`，输入 `/skill:` 后面板：

```
│ → skill:clash Foreign-written skill (not Norma) in temp dir │
│ skill:norma-test Minimal probe skill for Norma discovery test │
```

两个技能均被发现，说明 `KIMI_CODE_HOME` 生效，技能目录确为 `$KIMI_CODE_HOME/skills/`。

对照：第一次没设 `HOME`，面板显示 `(1/71)`，混入真实 `~/.agents/skills` 里的技能，证实 `~/.agents/skills` 在真实 HOME 下、不随 `KIMI_CODE_HOME` 变化。

### 2. 同名冲突（仅记录）

在临时目录里构造，未改任何真实用户文件。

- `$T/fakehome/.agents/skills/norma-test`（description 为 `AGENTS-DIR version`）与 `$T/home/skills/norma-test` 同名：面板仍显示 `Minimal probe skill for Norma discovery test`，即 `KIMI_CODE_HOME/skills` 优先于 `~/.agents/skills`，且只显示一条。
- `$T/proj/.kimi-code/skills/clash`（`.git` 在 `$T/proj/.git`，description 为 `PROJECT version`）与用户级 `clash` 同名：面板显示 `skill:clash PROJECT version`，项目级覆盖用户级，与文档优先级一致。
- 这些覆盖是静默的，面板没有警告。因此 Norma 在用户级目录写入时，若发现已有不是 Norma 写入的同名目录，不应覆盖。同名项目级技能会遮蔽 Norma 的技能。

### 3. 缺少 description

`$T/home/skills/bad/SKILL.md` 只有 `name: bad`：再次输入 `/skill:`，面板只列出 clash（项目级版本）和 norma-test，没有 `bad`，该技能被静默跳过。三份 Norma 技能的 frontmatter 都必须含 `name` 与 `description`，且 `name` 与目录名一致。

## limitations

- 模型是否真的按技能调用 norma 命令行未验证：没有登录凭据，无法启动模型对话，仅证明技能被发现并出现在斜杠命令面板。
- 只通过交互 TUI 的 `/skill:` 面板确认发现，没有非交互的列出命令；面板是截取 PTY 输出，格式可能随版本变化。
- 未测 `--skills-dir`、`extra_skill_dirs` 的实际行为（仅见于帮助和文档）。
- 未测技能目录写入时的热加载：每次均是新启动进程后才看到技能，运行中的会话是否感知新文件未验证。
- 未测三份 Norma 技能的实际文件内容（只用最小 SKILL.md），也未测 `/skill:<name>` 实际执行。
- 同名冲突只测了 `~/.agents/skills` 与 KIMI_CODE_HOME、用户级与项目级两组；未测与内置技能同名，也未测 `.agents/skills` 项目级。
- 清理：`rm -rf` 被权限系统拒绝，以下临时文件残留，需手动删除：`/tmp/kimiprobe.KWGT`（内含 home、fakehome、proj）、`/tmp/kimipty.py`、`/tmp/ks_en_customization_skills.html`、`/tmp/ks_customization_skills.html`。真实 `~/.kimi-code` 未被写入技能文件（仅 kimi 自身运行产生的数据，本测试均使用临时 KIMI_CODE_HOME）。
