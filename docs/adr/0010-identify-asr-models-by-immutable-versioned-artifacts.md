# 用不可变版本化 ID 标识识别模型

每个模型 ID 精确绑定一个不可变工件，例如 `zipformer-streaming-zh-int8-2025-06-30`；UI 仅显示友好名称“Zipformer Streaming ZH”。新权重版本作为新的模型库条目发布，而不在应用升级时把旧 ID 静默指向新文件。这使 Settings 选择、安装清单和归档哈希一一对应，用户必须明确安装并切换新版本。
