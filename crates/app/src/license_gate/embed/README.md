# License 嵌入默认值（发版时改这里）

打包进 App 的 **Keygate 默认 Base URL** 与 **离线验签公钥** 放在本目录。  
改文件后重新 `cargo build` / 打包即可，**不必改 Rust 逻辑**。

| 文件 | 内容 | 当前用途 |
|------|------|----------|
| `base_url.txt` | 一行 URL，无尾斜杠 | 激活请求的 Keygate 根地址 |
| `public_key.hex` | 一行 64 位 hex（Ed25519 公钥） | 本地验签信任根 |

## 怎么换

1. **换服务器地址**：编辑 `base_url.txt`（例如改成 `https://license.example.com`）。
2. **换签发密钥**：在 Keygate 侧轮换 `LICENSE_SIGNING_KEY` 后，执行：

   ```bash
   curl -s "$KEYGATE/api/v1/license/pubkey" | jq -r .data.public_key > public_key.hex
   ```

   确认文件只有一行 64 hex、无引号、无空格，然后重新构建 App。

3. **本地临时覆盖（不改文件）**：

   ```bash
   export NORMA_LICENSE_URL=http://localhost:9000
   export NORMA_LICENSE_PUBKEY_HEX=<另一把公钥>
   export NORMA_SKIP_LICENSE=1   # 仅开发跳过门控
   ```

优先级：**运行时环境变量** > **编译期 `option_env!`** > **本目录嵌入文件**。

## 注意

- 公钥必须与签发 Keygate 的 `LICENSE_SIGNING_KEY` 配对；换钥后旧包验签会失败，需发新版本。
- 不要把 **私钥** 放进本目录或 App 仓库。
