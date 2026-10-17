# Android 生命周期、进程死亡与后台恢复

## Runtime 状态机

`uninitialized → opening-store → reconciling → ready → draining → closed`；打开/迁移失败进入 `unavailable(recoverable|fatal)`。只有 store migrations、account scope、Host negotiation、pending reconciliation 完成才 ready；UI 不用固定延时猜测 ready。

`Activity.onStop` 不等于业务 cancel。ViewModel/Compose 订阅关闭只撤销 UI observer，不能误杀用户已提交的 durable run。明确点击 Stop 才产生 runtime cancel 命令。权限 UI、旋转、分屏、折叠、主题变化、系统进程回收均走相同重连协议。

## 进程恢复事务

启动读取 active runs、pending requests、outbox、checkpoint、account epoch。对远程已接受任务按 stable ID 查询/订阅；对可幂等本地任务先查执行记录；对无法知道是否产生外部副作用的任务标 outcome-unknown，要求 reconciliation 或用户决策。不得把 pending 全部当未发送重试。若未接受 intent 在磁盘，仅重放该 idempotency key，不生成新 run ID。

主进程崩溃与 Host 子进程/actor 崩溃分开处理。Host heartbeat/connection 丢失时 Coordinator 先 fencing 再恢复；旧 Host 的迟到完成不得覆盖新 epoch。销毁 runtime handle 必须停止 callback、join owned work、释放 JNI/global refs，关闭 DB 前等待事务，超时有明确安全终态。

## 后台执行决策

持续用户可见且符合系统类别的工作才使用带正确 type/permission 的 foreground service；可推迟同步/下载/重试使用 WorkManager 并设置 unique work、约束、backoff 和取消。实时 UI streaming 不通过 WorkManager 模拟。精确自动化不能把 WorkManager 宣传成准点计时器。

前台服务从后台启动及 while-in-use 麦克风/摄像头权限受系统限制，参见 [官方来源](../sources.md)。失败显示原因并 checkpoint；不能无限唤醒、滥用闹钟/无障碍或要求用户永久关闭省电来绕过。后台被杀不影响服务器已授权运行，但本机/远程的差异必须明确展示。无 Google 服务设备仍可前台工作与恢复，不把 FCM 当唯一数据源。

## 验收注入点

在 durable accept 前后、首 token 前后、审批待决、tool side-effect 前后、附件落盘中、OAuth 回跳、权益恢复中强杀进程；重启验证同一账户/会话/run、唯一副作用、唯一终态。旋转 20 次检查 observer/handles 无增长。拒绝 FGS、撤销权限、Doze、断网、存储耗尽分别证明可恢复或明确失败。
