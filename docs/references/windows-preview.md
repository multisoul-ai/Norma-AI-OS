# Norma OS Windows 预览包

解压整个文件夹，双击 `norma-os.exe`。请保留同目录的三个 CLI/helper、DLL 与 `Resources/`。
兼容目标为 Windows 10 22H2（Build 19045）及以上的 x64 系统，需要 Microsoft Edge WebView2 Evergreen Runtime。实际覆盖的系统以本候选包验收记录为准；Server runner 结果不替代 Windows 10 / 11 真机验收。

这是未签名的便携预览包。版本、源码提交和逐文件 SHA-256 记录在 `manifest.json`，ZIP 校验和位于旁边的 `.sha256` 文件。
无需安装 WSL 或 Git Bash；终端使用 Windows PowerShell。Claude/Codex 等外部 CLI 仍需用户自行安装并登录。

Windows 迁移仍在进行。Ego 屏幕预览、macOS 视频壁纸、Sparkle 自动更新和物理音频流程尚未提供完整 Windows 验收；本包不代表这些能力已可用。
真实模型任务不由协议夹具或 CLI 版本检查代替。仅声明当前候选包附带验收记录实际覆盖的行为。

## 真实 Agent 验收（由用户执行）

runner 冻结的 CLI 版本为 Codex `0.166.0`、Claude Code `2.1.316`。先记录本机 `codex --version`、`claude --version` 和包内 `manifest.json` 的源码提交；其他版本的结果请单独记录。

1. 新建带中文和空格路径的空项目，分别创建 Claude、Codex 卡片，使用自己的账号登录。
2. 各发送一个最小任务：创建不同的文本文件并写入唯一标记。检查真实磁盘内容、完成状态及会话归属。
3. 要求 Agent 在写入前通过内置提问工具询问两个选项。在 Norma 回答，确认只有原卡片继续、磁盘内容采用所选答案。
4. 运行可中断的多步骤文件任务，点击取消。确认任务停止，等待片刻没有迟到写入，也没有重复派工。
5. 关闭项目、退出并重开 App，恢复原卡片。确认仍为同一会话，继续一个小任务，检查先前上下文和文件保持一致。
6. 记录每项结果、CLI 版本、源码提交、文件标记及必要截图。不要在回执中包含登录凭据或密钥。

这些真实模型结果在用户反馈前保持 `USER_VALIDATION_PENDING`；runner 的协议夹具通过不替代上述验收。

## English

Extract the entire folder and launch `norma-os.exe` with Microsoft Edge WebView2 Evergreen Runtime installed. The compatibility target is x64 Windows 10 22H2 (build 19045) or newer; consult the candidate's acceptance evidence for tested systems. Server runner results do not replace Windows 10 / 11 device acceptance.
Keep the bundled helpers, DLLs and Resources directory together. This unsigned portable preview includes its source commit and file hashes in `manifest.json`.
PowerShell is used for terminals; external Agent CLIs require a separate installation and sign-in.
Ego screen capture, macOS video wallpaper, Sparkle updates and physical audio have not completed Windows acceptance.
Live Claude/Codex task, question, cancellation and resume checks are user-owned and remain pending until reported. Record CLI versions and the package source commit with the results; never include credentials.
