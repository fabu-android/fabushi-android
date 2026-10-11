# 系统边界、唯一 owner 与数据流

## 生产组成

```text
Compose screens / native navigation
        ↓ intents                 ↑ immutable projections
Presentation adapters (ViewModel, lifecycle collection)
        ↓
Typed Android bridge / android-preload
        ↓
Android platform runtime / android-main
        ↓ typed JNI (android-host-jni)
Mahayana Coordinator → Host → capability broker
                         ├→ local runner / platform capability
                         └→ authorized remote/box runner
```

此图表达所有权，不强制每条消息串行拷贝经过每一层。编译依赖必须为有向无环；上层不能绕开审批、账户 fencing 和 request registry。

| owner | 唯一写权 | 禁止职责 |
| --- | --- | --- |
| Compose/Presentation | 临时编辑态、焦点、滚动、渲染选择 | canonical transcript、推理调度、网络副作用重试 |
| Platform runtime | Activity/Service/Intent、系统权限、资源句柄、Keystore 访问 | Agent 规划、另一份 run 终态 |
| Coordinator | request registry、账户 epoch、run lifecycle、事件顺序、reconnect、Host supervision | 直接执行未授权工具 |
| Host | 业务执行、工具图、provider adapter、工作流、canonical repository 接口 | UI 生命周期、隐式权限升级 |
| Capability broker | scope/参数/用户授权校验、执行边界选择 | 仅依赖 UI 是否显示了审批卡 |
| Runner | 一次已授权执行的资源/进程/远程任务、取消与结果 | 自行换账户、修改授权范围 |
| Canonical repository | 同事务业务状态/outbox/event cursor/dedup | 与 Room/SharedPreferences 双写同一真相 |

## 生命周期原则

Application composition root 创建唯一 runtime handle。Activity recreation 仅 detach/attach projection；账号切换提升 epoch 并撤销旧能力；Host crash 由 Coordinator 结算，而不是 renderer 猜测“完成”。本机进程死亡后从 durable state 恢复，不从 ViewModel 内存重播用户请求。

所有异步工作具有 owner、scope、deadline、取消句柄与资源上限。事件 fan-out 使用 bounded queue，慢 UI 可合并增量并 snapshot resync，但不能丢失审批、终态、消息持久化事件。跨 ABI 传递大附件用受控文件句柄/引用，不复制 base64 全量数据。

## 状态流实例

用户发送 → 保存 draft revision 与 idempotency intent → Coordinator 校验 account/conversation → Host durable accept → projection 显示 accepted → run events 按 sequence 投影 → 终态与结果入库 → UI 显示 final。网络 response 或 token 停止本身不是 durable commit 证明。

审批 → broker 绑定参数摘要与 run → UI 展示风险和作用域 → 用户允许一次 → broker 原子消费一次 grant → runner 执行。用户修改参数后必须重新审批。来自工具/网页内容的文字不构成授权。

## 架构验收

检查 Gradle/Cargo dependency graph；检索禁止跨层 import 不足以证明运行时 wiring，还需 tracing 展示真实入口到 Host/Runner 路径。注入账户切换、重复事件、runner crash，验证唯一 owner 及副作用去重。禁止只测 mock transport 然后宣布生产闭环。
