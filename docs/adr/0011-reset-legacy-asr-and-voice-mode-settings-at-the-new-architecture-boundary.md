# 在新架构边界重置旧 ASR 与语音模式设置

新架构不迁移 `voice.paraformer.model_dir` 或 `voice.realtime.enabled`，也不把旧 Paraformer 默认目录探测保留为正式产品路径。升级后用户按新流程选择语音交互模式并从受控模型库安装识别模型；Agent Provider、LLM endpoint、模型名和 API key 仍保留。这是有意的破坏性边界，用来避免新领域模型长期背负路径设置和布尔模式的兼容分支。
