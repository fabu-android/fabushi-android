# F04 工具、能力审批、执行与人工接管

## 授权合同

Host capability broker 校验权限，Runner 仅执行已授权工作，Coordinator 关联 request/run/account。工具元数据和 readOnlyHint 不等于用户授权。调用绑定 tool/version、参数摘要、账户 epoch、目标资源/设备、deadline、requestId 和 grant；schema 无效不得执行。

审批 requested → allowed-once/denied/expired/cancelled。允许一次原子消费并绑定原参数；更换参数、资源或目标必须重新审批。UI 显示允许按钮不代表授权已生效；重建页面从 canonical state 恢复，不再消费同一 grant。

## 执行和资源

本机执行只访问应用沙箱与用户授权资源。Android 不具备桌面任意 OS 路径和进程权限，按 [平台差异](../architecture/platform-deltas.md) 提供受支持本机实现或明确授权的远程替代。不能假定系统存在桌面 shell，也不能把插件内容当受信任原生代码。

任务具有并发、时间、输出、临时空间和内存预算。大结果使用附件引用。资源清理只作用于当前任务拥有的目录和句柄。副作用重试需要幂等键或状态查询；超时不确定时显示 outcome-unknown 并核对，不盲目重复动作。

取消先阻止新动作，再请求停止和释放资源，最后由 owner 结算；停止进程本身不能证明外部操作没有发生。每个执行结果和错误都带安全 correlation 信息而非敏感参数。

## 人工接管与验证

控制权状态 automation-owned → takeover-requested → human-owned → resume-pending → automation-owned。切换时撤销旧控制 token；人工控制期间自动输入必须停止。恢复必须是明确操作并重新验证目标，不自动夺回。

验证拒绝/过期无副作用、重复允许只执行一次、参数变化重新审批、旧账户 grant 无效、取消后子任务和临时资源收敛、非幂等超时不重复执行。证据同时包含审批记录、实际执行次数、资源收敛和最终 UI，而非只测按钮点击。
