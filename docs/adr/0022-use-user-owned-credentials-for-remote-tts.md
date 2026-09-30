# 远程 TTS 使用用户自有凭据直连

我们决定首期远程 TTS 采用 BYOK：用户配置自己的供应商 API Key，App 直接连接供应商，调用费用与限额归属用户账户。不在桌面应用内置 Norma 共享密钥，也不为首期 MiniMax 接入建立服务端代理；后者会引入用户计费、配额、防滥用和数据处理责任，是不同的产品边界。macOS 密钥存储方式已由 [ADR-0028](0028-store-macos-credentials-in-app-db.md) 更新。
