# Provider API Key 存入 macOS Keychain

> macOS 存储决策已由 [ADR-0028](0028-store-macos-credentials-in-app-db.md) 取代；本文保留历史背景。

首次 Voice Setup 默认要求用户配置 OpenAI-compatible Provider，因此 API Key 会成为常规用户凭据，而不再是少数高级用户的可选字段。我们决定把 API Key 存入 macOS Keychain，SQLite 只保存 Provider、Base URL、模型及不可反查密钥明文的 Keychain 引用；界面只显示凭据已保存，并提供替换或移除操作。相比继续把密钥写入应用设置数据库，这增加了平台适配与错误恢复成本，但避免本地数据库备份、复制或诊断时泄露长期凭据。
