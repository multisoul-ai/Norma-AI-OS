# Lemon Squeezy 注册、非技术接入与香港个人结算调研

> 调研日期：2026-07-27  
> 范围：只讨论商户注册、店铺审核、消费者付款、商户结算与香港个人账户适用性；不讨论 API、Webhook 或代码实现。  
> 说明：以下结论以 Lemon Squeezy、Stripe 官方资料为准，不构成法律或税务意见。

## 结论摘要

1. Lemon Squeezy 是面向软件、SaaS 和数字内容的 Merchant of Record（MoR）。消费者法律上的交易对手是 Lemon Squeezy；平台负责收款、间接税、PCI、欺诈及大部分争议处理，卖家获得扣除税费后的净额。
2. 个人可以进入其审核和税务流程，并非必须先成立公司。非美国个人或实体需要提交适用的 W-8 税务资料；缺失税务资料可能导致结算暂停。
3. 香港列在 Lemon Squeezy 的银行结算支持地区中，HKD 也是可选结算币种。
4. “香港个人银行卡可以收款”应更准确地表述为：**香港居民个人用本人同名的香港银行账户，原则上具备支持条件，但仍须通过 Lemon Squeezy/Stripe 的身份、店铺与银行账户审核。**标准结算资料是银行账户信息，不是普通信用卡或借记卡卡号。
5. **中国内地居民仅持有香港个人银行卡，并不能据此推导为可按香港个人商户注册。**Stripe 对香港个人账户的公开要求提到，按香港个人身份申请时不能使用非香港身份证号码；没有 HKID、但有香港商业登记号码（BRN）时应选择 Sole Proprietorship。Lemon Squeezy 未公开承诺“内地身份 + 香港个人账户”一定可用，因此必须在投入技术接入前向官方取得书面确认。

## Lemon Squeezy 适合什么

官方通常接受可由平台交付或明确验证访问权的数字产品，例如：

- Software / SaaS
- eBook、PDF、设计资产、照片、音频、视频
- 通过订阅或许可证管理访问权的软件、课程、会员产品

官方通常不接受实体商品、咨询/设计/开发等服务、私域群访问、第三方商品市场、成人内容、加密/NFT、赌博及其他受监管产品。

对 Norma OS 这类 macOS 软件而言，软件许可、一次性数字下载或 SaaS/订阅属于平台列出的常见可接受类型；最终仍以店铺审核结果为准。

来源：

- [Activate Your Store](https://docs.lemonsqueezy.com/help/getting-started/activate-your-store)
- [Prohibited Products](https://docs.lemonsqueezy.com/help/getting-started/prohibited-products)

## 注册与非技术接入流程

1. 在 Lemon Squeezy 注册账号，可使用邮箱密码或第三方登录；验证邮箱。
2. 创建 Store，设置名称、子域名、实际国家/地区与展示币种。新店默认处于 Test mode。
3. 在后台创建产品，选择一次性付款或订阅，填写价格、产品说明、交付文件或购买后跳转地址。
4. 准备可公开访问的产品介绍、明确的交付方式、联系信息和退款政策。这些信息越完整，平台越容易理解业务真实性。
5. 点击 “Activate your store”，填写业务与客户问卷，并上传政府签发身份证件完成身份验证。官方称典型审核时间为 2–3 个工作日，但风险审核或产品不在支持范围时可能更久或被拒。
6. 在 Settings → Payouts 连接银行账户或已验证的 PayPal。
7. 非美国个人/实体提交适用的 W-8 税务资料；缺失时结算可能被暂停。
8. 无需技术开发也可以先使用 Lemon Squeezy 托管店铺或产品 Checkout Link，在官网、社交媒体或邮件中分享。

来源：

- [Getting Started](https://docs.lemonsqueezy.com/guides/getting-started)
- [Activate Your Store](https://docs.lemonsqueezy.com/help/getting-started/activate-your-store)
- [Verify Your Identity](https://docs.lemonsqueezy.com/help/getting-started/verify-your-identity)
- [Tax Forms](https://docs.lemonsqueezy.com/help/tax-forms)
- [Sharing Products](https://docs.lemonsqueezy.com/help/products/sharing-products)

## 消费者怎么付款

Lemon Squeezy 的当前官方列表包括：

- Visa、Mastercard、American Express、Discover、Diners Club、JCB、UnionPay
- PayPal
- Apple Pay、Google Pay
- Alipay、WeChat Pay
- Cash App Pay
- 部分银行扣款

平台会根据设备和地区显示可用方式。订阅当前只支持卡、Apple Pay、Google Pay 和 PayPal。

虽然店铺可以展示包括 CNY、HKD 在内的多种币种，Lemon Squeezy 表示交易最终以 USD 处理；银行结算可选择 HKD，由结算处理方按当时汇率转换。

来源：

- [Payment Methods](https://docs.lemonsqueezy.com/help/checkout/payment-methods)
- [Currencies](https://docs.lemonsqueezy.com/help/payments/currencies)

## 商户怎么收到钱

资金流为：

`消费者 → Lemon Squeezy（MoR）→ 扣除销售税、平台费、退款/争议等 → 银行账户或 PayPal`

当前公开规则：

- 净销售额先保留 13 天。
- 每月 1 日和 15 日创建结算，通常在 14 日和 28 日支付。
- 银行到账通常再需要 1–5 天。
- 最低结算门槛为 50 USD；不足会滚入下一周期。
- 银行和 PayPal 结算基准均为 USD；银行可转换为所选本地币种，PayPal 以 USD 入账。
- 每次结算可下载 reverse invoice；它可作为 Lemon Squeezy 向卖家付款的记录。

来源：

- [Getting Paid](https://docs.lemonsqueezy.com/help/getting-started/getting-paid)
- [Merchant of Record](https://docs.lemonsqueezy.com/help/payments/merchant-of-record)

## 费用与退款风险

标准电商费用为每笔 `5% + 0.50 USD`，没有电商月费。以下情况可能叠加：

- 美国以外的国际交易：+1.5%
- PayPal 交易：+1.5%
- 订阅交易：+0.5%
- 美国以外银行账户结算：每次结算 1%
- 美国以外 PayPal 结算：每次 3%，最高 30 USD

退款和争议不是“平台全部承担成本”。Lemon Squeezy 可以在购买后 60 天内为避免拒付而自行退款；退款会影响后续结算。大多数拒付由平台处理，但通常会从卖家结算中扣回相应金额，并收取 15 USD 争议费；拒付过多可能导致账户暂停。

来源：

- [Pricing](https://www.lemonsqueezy.com/pricing)
- [Fees](https://docs.lemonsqueezy.com/help/getting-started/fees)
- [Refunds and Chargebacks](https://docs.lemonsqueezy.com/help/payments/refunds-chargebacks)

## 香港个人银行账户：分场景判断

### 香港居民个人 + 本人同名香港银行账户

具备较强的可行性依据：

- 香港在 Lemon Squeezy 银行结算地区名单中。
- 非美国个人包含在平台税务流程内。
- Stripe 的银行账户所有权规则允许 Individual 使用账户所有人或登记 DBA 名下的银行账户。
- HKD 是 Lemon Squeezy 支持的银行结算币种。

但仍不能写成“无条件保证”。平台可能要求身份证件、地址、银行账户所有权证明或重新验证，最终以 Payouts 页面和人工审核结果为准。

### 中国内地居民 + 仅有香港个人银行卡

不能据此保证可用，原因是：

- 银行账户所在地区只是条件之一，KYC 还会核对个人身份、居住/经营信息、税务身份与账户户名。
- Stripe 的香港商户要求写明：按香港 Individual 申请时，不能使用非香港身份证号码；若没有 HKID 但有 BRN，应按 Sole Proprietorship 申请。
- 不应为通过审核而把真实居住地或经营主体虚报为香港。

因此，若实际情况是“内地身份、没有 HKID/香港主体、只有香港银行账户”，应先向 Lemon Squeezy 支持团队提交真实情况并取得书面确认，再决定是否投入接入。

### “银行卡号”与“银行账户”不是一回事

Lemon Squeezy 标准帮助文档公开的是：

- Bank account payout
- Verified Personal or Business PayPal payout

它没有承诺可以把标准结算绑定到一张普通信用卡或借记卡卡号。中文所说的“银行卡”如果实际指卡片背后的本人银行账户，应按银行账户资料连接；不要把信用卡/借记卡号当作标准收款路径。

来源：

- [Supported Countries](https://docs.lemonsqueezy.com/help/getting-started/supported-countries)
- [Getting Paid](https://docs.lemonsqueezy.com/help/getting-started/getting-paid)
- [Stripe: Verify your bank account ownership](https://support.stripe.com/questions/verify-your-bank-account-ownership?locale=en-GB)
- [Stripe: Requirements for Hong Kong-based businesses](https://support.stripe.com/questions/requirements-for-hong-kong-based-businesses?locale=en-GB)

## 2026 年的产品背景

Stripe 已于 2024 年收购 Lemon Squeezy。Lemon Squeezy 在 2026 年 1 月表示，团队正在推进 Stripe Managed Payments，并计划为 Lemon Squeezy 用户提供迁移路径。当前 Stripe Managed Payments 为 Public Preview，香港属于支持的商户地区。

这不表示 Lemon Squeezy 已停止服务；其帮助文档和注册流程仍在更新。对新项目而言，合理顺序是先验证 Lemon Squeezy 的店铺审批和香港结算，再比较 Stripe Managed Payments 的业务资格与客户地区限制，最后才选择长期接入方案。

来源：

- [Lemon Squeezy: 2026 Update](https://www.lemonsqueezy.com/blog/2026-update)
- [Stripe Managed Payments: How it works](https://docs.stripe.com/payments/managed-payments/how-it-works)

## 建议的零技术验证顺序

1. 准备一页真实产品介绍：卖什么、价格、交付方式、目标客户、联系方式、退款政策。
2. 用真实身份注册 Lemon Squeezy，创建 Test mode 店铺。
3. 先提交店铺激活和身份审核。
4. 审核通过后，绑定本人同名香港银行账户并完成 W-8。
5. 确认 Payouts 页面显示可结算；必要时要求支持团队书面确认账户组合。
6. 完成一次小额真实购买和一个完整结算周期后，再投入正式技术接入。

建议发给 Lemon Squeezy 支持团队的问题：

> I am an individual resident in [actual country/region] and I sell my own software/SaaS. I hold a personal bank account in Hong Kong under the same legal name as my identity document. Can I activate a Lemon Squeezy store as an individual and receive bank payouts to this Hong Kong account? Which store country and tax classification should I use, and which identity/bank documents will be required?

