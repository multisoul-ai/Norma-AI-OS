# A1模型安装与Jev配置

- `a1-managed-install-zh.png` / `a1-cancelled-zh.png`：无需Python配置，单一安装入口，取消后可重试且未发布模型配置。
- `a1-installed-zh.png`：实际下载与完整校验后的安装状态，文件就绪与模型加载分开。
- `a1-reopened-en.png` / `a1-rechecked-en.png`：英文重开保留受管引擎配置，不再显示Python选择器或安装长路径；检查通过但未激活、未录音。
- `a1-installed-integrity.json`：14个真实文件长度/SHA256、Python哈希及manifest哈希复查；不是布局证据。
- `jev-invalid-zh.png`：真实401/403结果、明确恢复提示、密钥掩码、生产入口禁用。
- `jev-keychain-saved-zh.png` / `jev-verified-zh.png`：安全保存与真实连接结果分开；实际返回模型和耗时。
- `jev-reopened-en.png` / `jev-reverified-en.png`：重开从Keychain加载，须重新验证，不明文显示密钥。
- `jev-cleared-en.png`：清除后旧成功状态失效，验证按钮禁用。
- `a1-missing-files-en.png` / `jev-invalid-en.png`：英文真实失败原因、重试入口和未持久化错误配置。

仅使用固定合成Choice请求；不录音、不发送真人转写或画布内容、不启用生产Jev。
不以这两条配置场景声称A1真实麦克风全链路通过。
