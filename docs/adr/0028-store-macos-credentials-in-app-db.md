# ADR-0028：macOS 凭据保存在 AppDB

- 日期：2026-09-29
- 状态：Accepted；取代 ADR-0017 的 macOS Keychain 存储决策

## 决策

Mac 端 Voice Provider、Azure ASR、MiniMax TTS、豆包、手机推送配置与手机服务 TLS 身份统一保存在本机 `app.sqlite`。Provider 密钥使用 `credential.agent-provider.*` 命名空间，设置仍保存凭据引用；推送配置与 TLS 身份各使用一个设置键。AppDB 文件权限为 `0600`。iPhone 端的 Keychain 不受此决策影响。

启动时不读取旧 macOS Keychain，避免再次触发授权弹窗。旧 Provider 与推送密钥需要用户重新输入；旧手机服务身份不会自动恢复，已配对手机需要重新配对。旧钥匙串条目不自动删除，避免删除时触发系统授权或误删其他应用仍使用的项目。

## 取舍

凭据会作为明文进入本机数据库及其备份；文件权限只能阻止其他本地用户读取，无法防止同一用户权限下的程序读取。常规日志、诊断与 UI 快照仍不得输出密钥。
