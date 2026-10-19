# Developer ID Application 证书与本机钥匙串配置

核对日期：2026-10-11。范围：为 Norma OS 本机签名打包和 Apple 公证准备签名身份；操作步骤及本次执行进度如下，不包含线上发布。

## 1. 本项目需要的材料

本机必须同时拥有团队 `2BF8G9AN4L` 的 **Developer ID Application 证书及与其配对的私钥**。只有 `.cer` 公共证书不能完成签名；数字身份由证书中的公钥和秘密保存的私钥组成。[Apple：数字身份](https://support.apple.com/guide/keychain-access/what-is-a-digital-identity-mchlp2695/mac)

`Packager.toml` 当前要求的完整名称是：

```text
Developer ID Application: Shanghai Jieyu Future Intelligent Network Technology Co., Ltd (2BF8G9AN4L)
```

本次现场检查结果由主流程记录：本机只有 Apple Development 身份，缺少有效 Developer ID Application 身份；Apple 公证 API 凭据已验证。GitHub `release` 环境存在 `DEVELOPER_ID_P12_BASE64` 和 `DEVELOPER_ID_P12_PASSWORD` 配置名，但查看 Secret 配置名不等于取得原始 `.p12` 或密码。优先找原证书持有者保留的 `.p12`，或在原 Mac 导出；找不到时再申请新证书。

Developer ID Application 用于签署 Mac App；Developer ID Installer 用于签署安装器 `.pkg`。当前 `.app` / `.dmg` 流程需要前者。Apple Development、Apple Distribution、App Store Connect `.p8` API Key 和 Sparkle 私钥各自用途不同，不能代替 Developer ID Application 签名私钥。[Apple：Developer ID](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)

## 2. 选择路径及需要本人完成的授权

| 情况 | 应做的操作 |
|---|---|
| 团队已有有效 `.p12` 及导出密码 | 直接按第 5 节导入本机 |
| 已有证书及私钥在另一台 Mac | 原持有者按第 4 节导出，再按第 5 节导入 |
| 没有可用私钥或没有已有证书 | 本机按第 3 节生成 CSR，由团队 Account Holder 上传申请 |
| 网站有证书，但只有 `.cer` | 找生成原 CSR 的 Mac 或 `.p12` 备份；下载 `.cer` 不能恢复丢失私钥 |

需要你或团队人员亲自输入/批准的项目如下。无需把密码、验证码或私钥发到聊天中。

| 操作 | 对应授权或材料 | 执行人 |
|---|---|---|
| 登录 Apple Developer 网站 | Apple Account 登录及双重认证验证码 | 团队 Account Holder |
| 新建手工 Developer ID Application 证书 | 团队 `2BF8G9AN4L` 的 Account Holder 权限；会员有效、相关协议已接受 | 团队 Account Holder |
| 创建 CSR、导入证书、解锁 `login` 钥匙串 | macOS 登录/钥匙串密码；若系统请求 Touch ID 则按提示确认 | 当前 Mac 用户 |
| 导出已有身份为 `.p12` | 原 Mac 钥匙串解锁或私钥访问批准；另设置一个 `.p12` 导出密码 | 原证书私钥持有者 |
| 导入已有 `.p12` | `.p12` 文件及其导出密码；如提示解锁钥匙串，再输入本机密码 | 当前 Mac 用户 |
| 首次执行本地签名 | 允许可信系统 `codesign` 使用该私钥；根据需要选择“允许”或“始终允许” | 当前 Mac 用户 |

Apple 要求 Account Holder 登录使用双重认证，并明确不要共享 Apple Account 密码或六位验证码。[Apple：登录开发者账号](https://developer.apple.com/help/account/access/sign-in-to-your-developer-account/)

**权限区分：**手工申请普通 Developer ID 证书要求 **Account Holder**。Admin 即使能管理其他发行证书，也不能据此推定能手工创建 Developer ID；Apple 单独允许被授予 cloud-managed Developer ID 权限的 Admin 使用云端管理证书。[Apple：Developer ID 角色要求](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)、[Apple：角色与权限](https://developer.apple.com/help/account/access/roles/)

云端管理证书由 Apple 远程管理，用于 Xcode Organizer archive/distribution 工作流。当前 `cargo-packager` / `codesign` 本机流程要查找本地签名身份；获得云端证书访问权限不会自动在本机产生可供该流程使用的证书私钥。本文采用本机 CSR 或 `.p12` 导入路径。[Apple：Cloud-managed certificates](https://developer.apple.com/help/account/certificates/cloud-managed-certificates/)

## 3. 申请新证书：本机生成私钥与 CSR，Account Holder 上传

### 3.1 在最终打包的这台 Mac 上生成 CSR

1. 按 `⌘Space` 搜索“钥匙串访问”或 `Keychain Access`，按回车打开。使用钥匙串访问，不是 Passwords /“密码” App。
2. 在左侧选择 `login` /“登录”钥匙串。如果已锁定，先按系统提示解锁。通常使用当前 macOS 登录密码；若钥匙串密码曾单独修改，则使用其实际密码。
3. 从屏幕顶部菜单选择 **钥匙串访问 → 证书助理 → 从证书颁发机构请求证书…**（`Keychain Access → Certificate Assistant → Request a Certificate From a Certificate Authority…`）。
4. 填写以下字段：

   | 字段 | 填写方式 |
   |---|---|
   | User Email Address / 用户电子邮件地址 | 你的工作邮箱 |
   | Common Name / 常用名称 | 便于识别这对密钥的名称，例如 `Norma OS Developer ID Local 2026` |
   | CA Email Address / CA 电子邮件地址 | **留空** |
   | Request is / 请求 | 选择 **Saved to disk / 存储到磁盘** |

5. 按 Apple Developer 的常规 CSR 步骤继续；不必套用 Apple Pay 的 ECC 配置。保存为 `NormaOS-DeveloperID.certSigningRequest`，可放在桌面临时目录。
6. 完成后，钥匙串访问会生成用于该 CSR 的密钥对；在当前钥匙串的 **Keys / 密钥** 分类查找刚填的 Common Name，并确认存在私钥。**私钥应留在本机钥匙串**；CSR 文件用于申请，不是私钥备份。
7. 如果不是你本人担任 Account Holder，只把 `.certSigningRequest` 交给团队 Account Holder 上传，不要共享 Apple 账号，也不需要把本机私钥交给对方。

CA Email 留空及 Saved to disk 是 Apple Developer 的 CSR 指引；它优先于面向普通 CA 的“通过邮件申请”通用步骤。[Apple：创建 CSR](https://developer.apple.com/help/account/certificates/create-a-certificate-signing-request/)。钥匙对生成由证书助理处理。[Apple：证书助理与密钥对](https://support.apple.com/guide/keychain-access/request-a-certificate-authority-kyca2793/mac)

### 3.2 Account Holder 登录并申请

1. 用团队 Account Holder 的 Apple Account 打开 [Apple Developer Account](https://developer.apple.com/account/)，本人完成登录和双重认证。
2. 如果该账号属于多个团队，点击页面右上角团队名称切换到 **Shanghai Jieyu Future Intelligent Network Technology Co., Ltd**，确认 Team ID 为 **`2BF8G9AN4L`**。不要选择个人团队或其他公司团队。
3. 进入 [Certificates, Identifiers & Profiles](https://developer.apple.com/account/resources/certificates/list)。左侧选择 **Certificates**，点击 **`+`**。
4. 在 **Software** 中选择 **Developer ID Application**，继续。不同网站版本可能先显示 Developer ID，再选择 Application；按“签 Mac App”的证书类型选择。不要选择 Developer ID Installer、Apple Development 或 Apple Distribution。
5. 如果出现 **Developer ID Certificate Intermediary** 选择，选择 **G2 Sub-CA (Xcode 11.4.1 or later)**。Apple 当前明确推荐 G2；旧 G1 Sub-CA 将于 2027-02-01 到期，之后其签发证书不能继续用于签名。[Apple：替换旧 Sub-CA 证书](https://developer.apple.com/help/account/certificates/replace-developer-id-certificates)
6. 点击 **Choose File**，上传 **这台打包 Mac 刚生成的** `NormaOS-DeveloperID.certSigningRequest`，点击 **Continue**。
7. 生成成功后点击 **Download**，下载 `.cer` 文件。记录证书到期日；Apple 当前文档说明 G2 叶证书有效期为 **一年，需要每年续签**，不要按旧教程假设五年有效期。[Apple：当前 G2 有效期](https://developer.apple.com/help/account/certificates/replace-developer-id-certificates)
8. 如果由另一位 Account Holder 操作，让其把下载的 `.cer` 返回给你。私钥仍在生成 CSR 的本机，证书要回到该 Mac 与其配对。

Apple 允许同一团队最多持有五张 Developer ID Application 和五张 Developer ID Installer 证书。已有证书不必为了本机测试直接吊销；遇到额度限制时先核对现有证书持有者及用途。[Apple：Developer ID 额度及申请步骤](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)

### 3.3 在生成 CSR 的 Mac 导入新 `.cer`

1. 打开钥匙串访问，选择 `login` /“登录”。
2. 选择 **File / 文件 → Import Items / 导入项目…**，选中下载的 `.cer`。
3. 如果弹出 **Destination Keychain / 目标钥匙串**，选择 **login / 登录**，点击 **Open / 打开**。也可双击 `.cer` 安装，但显式导入更便于确认目标钥匙串。
4. 选择 **My Certificates / 我的证书**，搜索 `Developer ID Application`。
5. 找到上述公司名称和 `2BF8G9AN4L` 对应的证书，展开左侧小三角，确认下面有 **private key / 私钥**。证书应显示有效，没有红色不可信或过期提示。
6. 如果只有公共证书，没有可展开的私钥，先确认是否用了本机创建的 CSR、是否导入正确钥匙串；导入某台其他 Mac 的 CSR 对应 `.cer` 无法在这台 Mac 产生私钥。按第 4、5 节迁移原身份，或使用本机 CSR 重新申请。

Apple 的安装步骤是双击下载证书并在 My Certificates 查看；显式导入到指定钥匙串按 Apple Support 操作。[Apple：Developer ID 安装](https://developer.apple.com/help/account/certificates/create-developer-id-certificates/)、[Apple：指定目标钥匙串导入](https://support.apple.com/guide/keychain-access/import-and-export-keychain-items-kyca35961/mac)

## 4. 已有证书在另一台 Mac：导出含私钥的 `.p12`

由持有证书及其配对私钥的人在原 Mac 完成：

1. 打开钥匙串访问，选择保存原身份的钥匙串，通常是 `login` /“登录”。
2. 进入 **My Certificates / 我的证书**，搜索 `Developer ID Application`。
3. 核对公司、Team ID `2BF8G9AN4L`、有效期；展开证书，确认下方存在配对私钥。只在 Certificates 分类里看到 `.cer` 不够。
4. 选中该证书及展开后的配对私钥；选择 **File / 文件 → Export Items / 导出项目…**。确保导出的是签名身份，而不是只导出公共证书。
5. 文件格式选择 **Personal Information Exchange (.p12)**，保存为例如 `NormaOS-DeveloperID-2BF8G9AN4L.p12`。
6. 输入一个专用于该 `.p12` 的强密码，重复确认。这个密码是**文件加密密码**，可以与本机登录密码不同。
7. 如果系统要求原钥匙串密码或批准私钥导出，由原持有者本人输入/确认。导出完成后，通过团队认可的安全渠道把 `.p12` 转移到目标 Mac，密码采用独立渠道传递。
8. 如果导出项目变灰或不能选择 `.p12`，检查是否实际选中了含私钥的身份；没有私钥就不能导出完整签名身份。

也可在原 Mac 的 Xcode **Settings / Preferences → Accounts → 团队 → Manage Certificates…** 中，Control-click 对应签名证书，选择 **Export Certificate**，输入文件名与密码，导出 `.p12`。菜单字样随 Xcode 版本而异。[Apple：导出签名证书与缺少私钥处理](https://help.apple.com/xcode/mac/current/en.lproj/dev154b28f09.html)

Apple 支持从一台电脑导出证书和密钥，再在另一台导入；导出密码用于下一台电脑导入时访问这些项目。[Apple：钥匙串项目导入导出](https://support.apple.com/guide/keychain-access/import-and-export-keychain-items-kyca35961/mac)

## 5. 在本机导入 `.p12`

1. 准备 `.p12` 文件和它的导出密码。只有 `.cer` 时返回第 3、4 节确认私钥来源。
2. 打开钥匙串访问，选择 **login / 登录** 钥匙串。
3. 选择 **File / 文件 → Import Items / 导入项目…**，选择 `.p12`。
4. 目标钥匙串选择 **login / 登录**，点击 **Open / 打开**。
5. 系统询问文件密码时，输入 **`.p12` 导出密码**；系统询问钥匙串解锁密码时，输入 **当前 Mac 的钥匙串密码**。两种提示用途不同，不能混填。
6. 进入 **My Certificates / 我的证书**，搜索 `Developer ID Application`。展开目标公司的证书，确认存在配对私钥，Team ID 为 `2BF8G9AN4L`，证书处于有效状态。
7. 运行第 6 节命令验证本机可用签名身份。导入成功后按团队规则保管 `.p12` 备份和密码，避免将其提交到 Git 或上传到聊天。

操作入口及目标钥匙串选择遵循 Apple 官方导入步骤。[Apple：导入项目](https://support.apple.com/guide/keychain-access/import-and-export-keychain-items-kyca35961/mac)

## 6. 验证身份及排错

在终端运行只读检查：

```bash
security find-identity -v -p codesigning
```

应出现包含完整名称的一行，例如：

```text
1) <40 位身份指纹> "Developer ID Application: Shanghai Jieyu Future Intelligent Network Technology Co., Ltd (2BF8G9AN4L)"
```

末尾 `valid identities found` 应大于 0；只有 Apple Development 或其他 Team ID 不满足本项目要求。该命令检查有效 identity，并非仅枚举 `.cer`。可通过本机 `man security` 查看 `find-identity`、`-v` 及 `-p codesigning` 的定义。

| 结果 | 下一步 |
|---|---|
| 找不到 Developer ID Application | 确认 `.p12` 已导入、目标钥匙串正确；如果只有 `.cer`，找配对私钥 |
| 名称正确但 Team ID 不同 | 切到 `2BF8G9AN4L` 团队申请或取得其 `.p12` |
| 证书没有配对私钥 | 回到创建 CSR 的原 Mac 导出身份；若原私钥丢失，用新 CSR 申请证书 |
| 证书过期、吊销或不受信任 | 核对证书有效期、Issuer、系统时间及中间证书，按问题根因处理 |
| 显示正确有效身份，签名时请求钥匙串访问 | 本人确认请求来自可信系统 `codesign`，再授权使用该私钥 |

如果怀疑缺少 Developer ID G2 中间证书：

1. 在 [Apple 官方 PKI 页面](https://www.apple.com/certificateauthority/) 找 **Developer ID - G2**，不要误下载 Worldwide Developer Relations G2。
2. 官方下载地址：[DeveloperIDG2CA.cer](https://www.apple.com/certificateauthority/DeveloperIDG2CA.cer)。核对本机目标叶证书的 Issuer 是否为 G2；中间证书的有效期不等于叶证书的有效期。
3. 通过钥匙串访问导入官方中间证书，再运行 `security find-identity -v -p codesigning`。如果仍无效，检查叶证书是否过期/吊销及配对私钥。
4. 不用把证书手工设置成 **Always Trust / 始终信任** 来掩盖证书链问题，不把私钥开放给所有应用。

Apple PKI 当前列出 Developer ID - G2 中间证书到期日为 2031-09-17；旧 G1 到期日为 2027-02-01。当前 Developer ID 叶证书按 Apple 替换文档每年续签。[Apple：PKI](https://www.apple.com/certificateauthority/)、[Apple：G2 更新说明](https://developer.apple.com/help/account/certificates/replace-developer-id-certificates)

首次签名时，钥匙串可能提示 `codesign` 要访问该身份私钥。选择 **Allow / 允许** 仅批准这次；如准备反复在本机签名且确认请求可信，可选 **Always Allow / 始终允许**，它只应授权对应可信请求。不要将此与证书的 Always Trust 混淆。具体按钮文字以当前 macOS 为准。[Apple：钥匙串访问授权含义](https://support.apple.com/guide/keychain-access/if-youre-asked-for-access-to-your-keychain-kyca1243/mac)

## 7. 准备完成后继续本地打包公证

证书导入并验证完成后，在本仓库使用：

```bash
SPARKLE_PUBLIC_ED_KEY="$(gh variable get SPARKLE_PUBLIC_ED_KEY --repo multisoul-ai/Norma-AI-OS --env release)" \
  ./scripts/release-local.sh --notarize
```

这个命令读取非敏感 Sparkle 公钥，执行仓库本地构建、签名、Apple 公证及票据验证，不是上传 GitHub Release 的命令。公证 API 环境按现有本机配置读取；若另开终端未继承配置，需要重新加载已有配置。此次无需 Sparkle 私钥；本机签名私钥则必须已在钥匙串就绪。项目执行细节以 [打包说明](packaging.md) 和 `scripts/release-local.sh` 为准。

## 8. 核对边界

- 本文依据 2026-10-11 拉取的 Apple 官方资料及本机现场检查整理。列表排版、按钮文字可能随界面语言或版本变化。
- 本机公证 API 认证已成功；已有团队 Developer ID Application 公共证书已通过官方 API 查到，但本机缺少它的配对私钥。
- 已使用 `okapi0129@outlook.com` 的 Chrome 会话实际下载该已有证书（ID `TY4BYK88GG`），文件为 `~/Downloads/developerID_application.cer`，并成功导入 `login` 钥匙串。证书的 Team ID 为 `2BF8G9AN4L`，本地到期日为 2027-02-02；它使用旧 Developer ID Sub-CA。
- 导入后分别检查全部匹配身份和有效签名身份，仍只有 Apple Development，没有 Developer ID Application。这确认当前可用钥匙串缺少该已有证书的配对私钥；其公钥也与本次新生成私钥的公钥不同，不能用新私钥替代。下载 `.cer` 本身无法完成签名。可从原签名 Mac 导出含原私钥的 `.p12`，按第 4、5 节导入后继续。
- 已在本机生成 RSA 2048 私钥及 CSR，并成功将私钥导入 `login` 钥匙串。公共 CSR 在桌面：`~/Desktop/NormaOS-DeveloperID-2BF8G9AN4L.certSigningRequest`。继续申请时使用这一份，不要重新生成另一对密钥。
- 私钥备份保存在权限受限的 `~/.norma/signing/developer-id-20261011-7hqv_bqd/` 中。私钥不上传 Apple；网页只上传公共 CSR。
- 实际 Apple Developer 会员详情显示当前“薇 曾”会话职能为“管理”，该团队 Account Holder 为 `fei lei`。网页中的 Developer ID Application 被禁用；官方创建 API 也返回仅 Account Holder 可操作的 HTTP 403。
- 新证书尚未签发，未导入新 `.cer`，尚未形成可用 Developer ID Application 签名身份。后续必须收到与上述 CSR 配对的证书，再按第 3.3、6 节验证后继续公证。
- 本机 `cargo build -p app --release` 已通过，当前版本仍为 `0.1.73`。实际执行 `./scripts/release-local.sh --notarize` 时，工具检查与 Git LFS 拉取通过，随后因缺少上述签名身份终止；此次未生成新 DMG，也未提交公证。
- Chrome 控制曾持续超时，期间另在 Safari 打开官方 Apple 登录页；之后 Chrome 已恢复，并按用户要求继续使用 Chrome。检查本机保存的登录信息时，“密码”App 提示已锁定，要求用户 `openclawd` 在本机输入 macOS 登录密码解锁；密码与验证码无需发送到聊天。
- 登录密码、双重认证及可能出现的钥匙串授权由本人处理；具体请求以实际界面为准。
- Apple 官方说明 cloud-managed 证书远程管理并由 Xcode Organizer 签名；本文不声称其私钥可下载或直接供本机 `codesign` 使用。
