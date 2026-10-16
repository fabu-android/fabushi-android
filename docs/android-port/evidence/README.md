# 证据边界与保留

本目录只保存已脱敏的 provenance、摘要、审阅记录和可长期访问的证据定位，不保存生产密码、令牌、私钥或个人会话内容。模板、示例、生成的 unreviewed ledger 都不是测试证据。

每一条通过结论都绑定 Desktop commit、Android tested checkout SHA、workflow run/attempt/job/step、assertions、artifact ID 与 SHA-256。GitHub PR 工作流的 merge SHA 与 PR head SHA 必须区分；最终验收需要证明实际 checkout 与声明一致。来自不同 HEAD 的成功步骤不能拼成一个“全绿”结论。

Generated source inventory 证明 tracked 范围被枚举，不能证明 blob 已逐段理解、依赖可构建或功能适用。Gitlinks、LFS、符号链接和构建动态引用必须进一步闭包；报告需保留 unresolved 数，不能隐藏。

Actions docs workflow 只做文档/范围校验和归档，不产生 APK，不代表 packaged acceptance。对 missing 文档、失效链接、未关闭的数据合同保留 failed/pending，不把人工排除当成产品通过。

上线前保留 source manifest、SBOM、签名指纹、package digest、测试报告、基准、数据库迁移与安全检查。存储期限与访问权限按敏感度设定；需要生产账户的录像应使用受控测试账户并脱敏，不公开真实消息。
