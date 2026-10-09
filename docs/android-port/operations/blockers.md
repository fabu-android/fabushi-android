# 卡点与决策记录

状态只允许 open / resolved / obsolete；每项有 owner、受影响范围、解除条件和下一次检查点。

| ID | 状态 | 影响 | 解除条件 | 可并行内容 |
| --- | --- | --- | --- | --- |
| DOC-WRITE-001 | resolved | 历史部分批量写入受安全检查拦截；2026-10-10 已通过原仓库授权的常规 GitHub 内容写入，补齐 05–12 八份正文 | 已在 PR #3 当前分支确认全部 12 份手册存在，等待 exact-HEAD docs CI 再验证 | 下一步继续 source-to-target 职责审计、产品实现和验收 |
| PRODUCT-BASELINE-001 | open | 旧 Grok 级 ledger 不能证明当前 Desktop 全量 parity | 完整 inventory 与逐职责复核、production wiring + exact HEAD evidence | 已确定域的源审计与实现规划 |
| RELEASE-001 | open | 真机、签名、商店、授权账户证据本轮未取得 | 受保护 Actions 环境及云设备完成真实验收 | 不依赖生产凭据的合同和静态检查 |

DOC-WRITE-001 曾向用户指定邮箱通知，threadId `1a1215529c220244`；目前正常 GitHub 写入已完成，已核实 PR #3 的 `docs/android-port/features/01–12` 全部存在。后续不能把历史 safety-block / CI 失败错误地当作仍未入库；但只有新 exact-HEAD Actions 全绿，才能将文档 CI 标为通过。无需对此主题再次发送同样的邮件。

本记录不声称上述项目是全项目仅有卡点。主实现必须持续登记新发现，并把 `blocked` 具体关联到责任和依赖，不得以一个全局 blocked 停止全部工作。