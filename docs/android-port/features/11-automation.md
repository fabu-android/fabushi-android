# 11 — 工作流、自动化、记忆与多 Agent 编排

## 完整责任，而不是定时器 Demo

迁移工作流定义、触发条件、步骤依赖、模型/工具执行、权限审批、人工参与、状态持久化、重试、取消、恢复、日志和结果入口；同时核对 Desktop 真实存在的记忆与多 Agent 协作责任。只有一个“定时发送消息”按钮不能代表完整自动化 parity。

Desktop source inventory 中检索 workflow、automation、scheduler、memory、collaboration 与实际生产调用者，提取 definition schema、版本、节点类型、事件、错误及终态。不要把历史前端文件或尚未 shipping 的 planner 直接列为已存在功能；Android 要实现已确认适用的语义，并对未 shipping 来源单独标识。

## 编排 owner 与不可变定义

Coordinator 是 workflow/run 的唯一执行 owner；Host 执行能力和工具；Runner 完成本机/远程任务；Android WorkManager/系统服务只是触发与执行资格适配，不建立另一份流程引擎。UI 编辑器提交命令，不自行运行节点或恢复任务。

内部工作流记录包括 definitionId、definitionVersion、触发器、步骤图、参数引用、权限要求、资源限制、执行位置和停止策略。发布定义前校验引用、依赖、循环与兼容性；允不允许循环以 Desktop 真实合同为准，不能无条件把所有流程当 DAG。每次 run 固定使用启动时的定义版本，编辑新版本不悄悄改变进行中的步骤。

## 执行状态与持久化

run 状态需能表达 queued、running、waiting-input、waiting-approval、waiting-platform、waiting-network、suspended、cancelling、succeeded、failed、cancelled；这些是设计语义类别，实际序列化枚举应与现有版本迁移合同相符。

节点执行先持久化尝试身份和输入版本，再调用 Host。结果和后继就绪状态以可恢复的事务保存；同一个触发 occurrence 和同一节点 attempt 具有稳定去重身份。远程副作用没有幂等保证时，失败恢复必须确认执行结果或进入 outcome-unknown，不能把流程整体从头再跑。

取消是向 owner 提交一次性请求，先禁止新步骤，再取消可取消的任务，保留不可回滚副作用的已完成事实。人工审批与取消竞争由 owner 原子裁决；审批 UI 被打开两次不能让步骤执行两次。

## Android 调度策略

即时且用户正在操作的流程在允许的前台运行环境执行；可延期、需重启后恢复的工作交给 WorkManager 触发。WorkManager 受约束、电源和系统调度影响，不能把设定时间描述为实际精确运行时间。

需要精确时刻的产品责任必须先证明符合系统 API 和权限/政策适用范围，否则明确给出可接受窗口、延迟提示或用户选择的远程执行位置。不能用不断重启服务、滥用精确闹钟或保持唤醒锁模拟桌面守护进程。

定时定义保存时区、当地时间语义、重复规则、修改版本和已消费 occurrence。夏令时、时区变更、设备离线、关机和应用恢复后，明确选择跳过/合并/补跑策略；补跑不得对已执行副作用再次操作。连续运行不支持的设备状态应显示 waiting-platform，而不是假装进行中。

## 记忆与上下文

把用户授权的长期记忆、会话摘要、检索索引、workflow 变量和临时模型 context 分开。Host 内相应 owner 维护版本与删除语义；记忆写入必须能追溯来源/账户，不把模型推测自动保存为用户事实。用户删除原文或退出账户时，对关联索引、缓存和远程副本按实际产品合同处理。

记忆导出与检索不可跨账户串数据。外部工具内容只能作为数据参与上下文，不得成为新的高优先级指令。token/附件预算、截断与摘要过程要保留用户可见的失败或降级状态，不无声丢掉重要输入。

## 多 Agent 与人工参与

每个子 Agent 有独立身份、任务与授权范围，并由共同 Coordinator 管理父子关联、资源配额和终结传播。子任务结束不能意外终结全部父流程；父取消后不能让未授权子任务继续产生新副作用。协作消息和结果写回通过 canonical transcript，避免多个 UI store 各写一份状态。

human takeover / waiting-input 必须显示请求内容、涉及资源、截止或过期行为；用户离开页面不自动等于批准或拒绝。恢复后仍能看见同一请求与原 run，不创建新待办。

## Actions 场景与证据

覆盖节点失败、重复触发、相同事件重送、两个并发 run、定义更新、进程在节点提交前/后被杀死、取消与批准竞争、权限撤销、远端失联、跨账户恢复拒绝、时区和夏令时、系统延期、断网后仅补跑未消费 occurrence、记忆删除及索引清理、父子任务关闭。

测试应通过生产 Coordinator/Host 执行链，不以测试专用 loop 代替。记录触发身份、定义摘要、attempt、commit point、唯一终态和副作用次数；以 exact HEAD 的日志与断言证明没有重复执行。

## 参考

[WorkManager PeriodicWorkRequest 的实际执行时机](https://developer.android.com/reference/androidx/work/PeriodicWorkRequest.Builder)、[Android 前台服务限制](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start)、[Desktop 完整源锁](https://github.com/bhrumom/fabushi-desktop/tree/3bc92400826cc4ca7ac665b467708e22261edc61)。具体调度周期与 API 取值由真实产品需求和发布所锁定版本验证，不凭本文新增后台无限运行保证。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现作为仓库 Android 实施规范。设计流程与字段不保证已经由 Android 生产实现，也不是产品测试通过报告；须以源合同、真实入口、状态机和 CI 正反向结果逐项验收。只在 GitHub Actions 构建/测试。

固定 Desktop main：`3bc92400826cc4ca7ac665b467708e22261edc61`。8,172 项文件清单尚需人工职责映射。
