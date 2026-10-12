# Runtime 命令、事件、取消与恢复合同

本文件规定 Android 内部合同设计；不是宣称 Desktop 已使用下列字段名。实现前从 fixed SHA 的 actual dispatch、types、Host handlers 和 tests 提取现有 wire method/field/error，写入兼容映射；不得自行发明后端 endpoint。

## Envelope（Android 内部规范）

Command 至少表达：protocolVersion、requestId、idempotencyKey（有副作用时）、accountId、accountEpoch、conversationId、runId（若已创建）、method、payload、deadline、capability grant reference。Reply 表达原 requestId、accepted/completed/error 与 typed result。Event 表达 account/epoch、streamId、sequence、eventId、runId、type、payload、durable checkpoint。ID 使用稳定不透明字符串；禁止截断、用显示名作 ID、将 64-bit sequence 经过 JS Number 丢精度。

错误族：invalid-input、unauthenticated、permission-denied、unsupported、rate-limited、unavailable、cancelled、deadline-exceeded、conflict、storage-full、outcome-unknown、protocol-mismatch。错误包含 safeMessage、retryability、sideEffectState、correlationId，不含 token。UI 有意保留未知枚举显示，不把未知错误当成功。

## Run 状态机

`created → accepted → running ↔ awaiting-approval/awaiting-user/suspended → succeeded|failed|cancelled|interrupted|outcome-unknown`。suspended 与 awaiting 状态不是终态。cancel-requested 是意图不是执行停止证明；Coordinator 在原子结算点决定唯一终态。完成先于有效 cancel 时保留成功；cancel 已生效后迟到 token/tool result 不得复活 run。外部副作用不确定必须 reconciliation 后才允许重试。

## 事件一致性

仅在对应账户 epoch 上投影。sequence 连续时应用，重复 eventId 忽略，gap 暂停增量并拉 snapshot+cursor，过旧 epoch 丢弃。snapshot 与后续事件在一致边界切换。通过 bounded fan-out 保证慢消费者不会耗尽内存，终态/审批事件不能简单 drop。renderer 订阅重建只 resync，绝不 resend。

## 传输语义

重连用 request registry + durable outbox；发送和 ACK 之间断开可能重复，故必须服务端幂等或 outcome 查询。没有服务端能力就不能保证 exactly-once。副作用 key 必须绑定账户/操作/参数 digest；不同参数不能复用同 key。stream EOF、HTTP 200、空 token、UI Stop 消失不等于最终成功。

## 必测反例

同 intent 连点、两设备重复提交、响应乱序、snapshot 期间新增事件、cancel 与 final 竞争、cancel 后审批返回、超时后第三方实际成功、账户切换后的旧 callback、Host restart 与旧消息并发。断言 durable state、真实副作用数量和 UI 一致，而非只检查字符串。
