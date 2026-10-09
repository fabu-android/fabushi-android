# 卡点与决策记录

状态只允许 open / resolved / obsolete；每项有 owner、受影响范围、解除条件和下一次检查点。

| ID | 状态 | 影响 | 解除条件 | 可并行内容 |
| --- | --- | --- | --- | --- |
| DOC-WRITE-001 | open | 本轮若干功能手册 GitHub 写入被工具安全检查拦截，不能宣称已写入 | 工具确认可以执行原写入；不得切账号或设备绕过 | 实施、验证、来源登记和非受阻文档 |
| PRODUCT-BASELINE-001 | open | 旧 Grok 级 ledger 不能证明当前 Desktop 全量 parity | 完整 inventory 与逐职责复核、production wiring + exact HEAD evidence | 已确定域的源审计与实现规划 |
| RELEASE-001 | open | 真机、签名、商店、授权账户证据本轮未取得 | 受保护 Actions 环境及云设备完成真实验收 | 不依赖生产凭据的合同和静态检查 |

DOC-WRITE-001 已向用户指定邮箱通知，发送前查重；threadId `1a1215529c220244`。这是工具层拦截，不是已确认的 GitHub 权限不足。对受阻项只保留状态，不在未解除前反复重试。

本记录不声称上述项目是全项目仅有卡点。主实现必须持续登记新发现，并把 `blocked` 具体关联到责任和依赖，不得以一个全局 blocked 停止全部工作。