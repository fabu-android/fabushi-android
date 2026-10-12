# 09 — 购买、订阅、权益、恢复与业务连续性

## 目标与真实性边界

对 Desktop main 实际存在的账户权益和商业能力实现 Android 等价效果。不能因为手机支付界面不同，就省略权益刷新、跨设备账户关联、恢复购买、退款/撤销与离线状态。也不能把旧 iOS 任务中的商务流程未经源码核实直接当作 Android 已有需求或后端接口。

已确认 Android 构建声明了 `com.android.billingclient:billing-ktx:9.1.0`；这只是仓库的依赖声明，不是本轮已经验证该版本可构建或已接入产品。Desktop 审计从 `source/product/fabushi/fabushi-account-service.ts`、`fabushi-account-policy.ts`、相关 contract tests 和生产入口出发，查明目前哪些功能受何种权益约束。

## Owner 与数据权威

Billing adapter 管理 Android 商店连接、商品查询和系统购买界面，不直接授予 Fabushi 权益。Account/Entitlement 服务是应用内唯一投影 owner，消费经过服务端验证的账户权益；Host policy 在使用受限功能时检查当前状态，不能仅由 Compose 隐藏按钮。

商店商品身份、商店交易身份、Fabushi accountId、Fabushi entitlementId 和业务 orderId 不得混用。以服务端合同明确建立关联，客户端 UI state 和网页支付完成页面都不是支付真实性证明。对外 API 必须从当前服务合同核实，本文件不创造未存在的 verify/restore endpoint。

## 购买状态机

内部购买流程：unavailable / loading-products → ready → user-confirming → pending 或 purchased-unverified → verifying → entitlement-active；cancelled、rejected、retryable-error、account-mismatch、revoked 是可解释的状态。

依据 Play Billing 官方合同，pending 不授予权益；先核实有效购买且状态为 PURCHASED，才按业务规则授予对应权益。购买确认/消费和服务端授予需要幂等与可恢复的记录。支付界面返回成功但服务端暂时不可达时显示“正在确认”，不是永久成功，也不诱导用户立即重复购买。

用户在购买期间切换账户时，不能把旧交易随当前 UI 账户直接领取。认证事务、商店返回的用户关联和服务端账户归属共同决定领取对象；处理不明确时进入人工可理解的关联/恢复流程，不自动抢占另一账户的权益。

## 恢复与跨设备

应用恢复前台或重新连接商店时查询当前购买状态，再与服务端核对权益。恢复购买不仅是重新查询 UI 产品列表，而是重建用户应有的有效权益。消耗型商品的历史归属和发放记录不能只靠当前商店购买列表恢复。

Desktop、Android 和后端必须看到同一个 Fabushi account 的权益；商店账号不等于 Fabushi 登录账号。退款、撤销、过期、宽限、暂停等情形是否存在及具体含义，按实际产品和商店合同建模；不能用一个到期时间覆盖所有状态。旧缓存有清晰版本与新鲜度，在线敏感副作用由服务端再授权。

离线时只按明确定义的缓存/宽限政策开放已有能力，明确区分“不知道最新权益”和“没有权益”。客户端时钟不能决定支付真实性，也不能因为暂时断网删除已经购买的数据。

## 分发差异与安全

Play 和其他分发渠道的允许支付入口、数字商品规则和外链政策需要在发布时重新核实，不沿用桌面收款网页就宣称合规。购买和恢复的用户目标必须保留；渠道差异写入 platform-delta，并向用户说明实际付款主体和渠道。

生产密钥和服务端验证凭据不进入 Android 包。测试账号/沙箱支付只用于受保护验收，不能在公开 release 开启“直接发放权益”的测试路径。交易 token、个人订单与付款信息在日志和工件中脱敏。

## 必须通过的场景

Actions 单位/合同测试覆盖 pending→purchased、重复回调、服务端超时、已发放后客户端进程死亡、错误账户、过期回调、退款撤销、同一购买多次恢复和缓存版本倒退。

受保护设备验收从产品入口进入系统购买/恢复流程，检查到账后的真实功能授权；跨端核对同一测试账户。取消购买不能创建权益；重复恢复不能重复发放消耗品；验证失败不能通过修改本地 preference 绕过。付款相关人工确认和商店授权未就绪时登记 blocker，不删掉测试、不使用伪权益替代最终证明。

## 参考

[Play Billing 集成](https://developer.android.com/google/play/billing/integrate)、[购买安全与服务端验证](https://developer.android.com/google/play/billing/security)、[当前 Android 构建声明](https://github.com/fabu-android/fabushi-android/blob/5d049353e167d9b91b5c765e1686f1ac8670f2ff/mobile/android/app/build.gradle)。这些资料不能替代本产品真实后端合同和发布渠道批准。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现作为 Android 原生迁移实施规范入库，不代表商业能力已通过 Play 和产品验收。新设计字段、流程与 owner 不冒充已存在的后端 wire。需审计当前 Desktop 固定 SHA 的实际入口与受测效果，全部构建和测试在 GitHub Actions。

源锁：`bhrumom/fabushi-desktop@3bc92400826cc4ca7ac665b467708e22261edc61`，8,172 项初始清单不等于已验证完成。
