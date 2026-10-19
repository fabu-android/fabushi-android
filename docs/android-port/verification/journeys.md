# 端到端用户旅程与故障矩阵

所有旅程记录前置账户/设备/数据、动作、可见结果、canonical state、副作用、资源清理和 artifact。测试账号数据独立可清理，不使用真实用户敏感内容。

| Journey | 正向步骤 | 关键反例和断言 |
| --- | --- | --- |
| J01 身份 | 登录 → hydration → runtime ready → 冷启动恢复 | 取消/重复回跳、刷新中切账号、撤销；旧结果无效 |
| J02 跨端消息 | Desktop 发 → Android 读/回复 → Desktop 同步 | 断网/重复 ACK/乱序；稳定 ID、无重复消息 |
| J03 Agent | 选模型 → 附件 → 发问 → 流式 → 工具 → final | rate-limit/断流/Stop-final race；唯一终态 |
| J04 审批 | 请求工具 → 展示范围 → 允许一次 → 实际结果 | 拒绝/过期/改参数/重复允许；无越权副作用 |
| J05 市场 | 发现 → 安装 → 连接 → 工具发现 → 调用 | 版本不兼容、取消更新、卸载中调用、撤销 |
| J06 Mini App | 多入口打开同一 app → bridge → 工具/业务 → 关闭 | nonce/origin 错误、dispose pending、native fallback |
| J07 媒体 | 选文件 → 上传 → 引用发送 → 预览/导出 | URI 失效/磁盘满/断网；无孤儿成功态 |
| J08 语音 | 录音 → 本机转写 → 编辑草稿 → 明确发送 | 飞行模式、缺模型、来电、切会话/账号、草稿已改 |
| J09 远端 | 配对 → 选择目标 → 观测/控制 → 人工接管 → 撤销 | 失联/换目标/旧输入；停止后无自动输入 |
| J10 工作流 | 建实例 → 节点/审批 → checkpoint → 重启恢复 | 重复 trigger、未知副作用、DST/错过执行 |
| J11 权益 | 受控商品 → 支付/验证 → Host 更新 → restore | pending/cancel/重复 receipt/退款；不重复发放 |
| J12 原生恢复 | 深链冷启动 → 登录 → 正确目标 → 旋转/分屏 | 无权限/目标删除/错账户；不重复动作 |
| J13 升级 | 安装已发布旧包 → 数据 → 新签名同源包升级 | DB 迁移 kill/空间满/密钥失效；数据可恢复 |
| J14 完整使用 | Human → Agent → 插件 → 附件 → 工作流 → 重开 app | 同一 canonical 账户/运行时，没有页面专属假状态 |

## 故障注入

对 durable accept 前后、工具外部副作用前后、final commit 前后、UI event delivery 前后强制 process death。分别杀 app、Host、远程连接；不把三者等同。记录恢复选择、idempotency key、查询结果和终态。

每个领域补齐 loading/empty/error/offline/permission-denied/outcome-unknown。联网测试和 deterministic fixtures 分层，fixtures 能证明分支处理但不能证明真实后端可用。生产包路径使用相同业务入口；测试标识只定位 UI，不能改变业务行为。
