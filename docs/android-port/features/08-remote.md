# 08 — 远程设备、Computer Use 与执行位置

## 三种场景必须分开

一是 Android 作为客户端查看/控制用户已经授权的远程电脑；二是 Android 为 Agent 提供应用自身的可访问操作面；三是控制 Android 设备上其他应用。这三者权限、能力、分发合规和技术限制不同，不能用一个“Computer Use 可用”结论覆盖。

产品目标是保留 Desktop 已承载的可适用任务和人类接管流程。优先在本机应用内提供 typed App Surface 操作；桌面 OS 专属能力通过明确配对的远程电脑实现，清楚显示执行位置。不得承诺普通 Android 应用具备无限制跨应用控制能力，不把 Accessibility 或其他敏感权限作为隐蔽的通用自动化绕过途径。

## 实际源码与 Android owner

Desktop 包装声明了 `resources/computer-control`；构建合同会生成 `source/box-exec-daemon` 的产物。继续从完整 inventory 定位设备身份、网关协议、配对流程、截图/帧传输、pointer/keyboard、命令生命周期及 human takeover 的实际调用链。历史 `desktop-peer.ts` 路径只做移动追踪线索，引用前必须确认仍存在。

Android platform adapter 管理网络状态、系统授权与前台展示；RemoteDeviceGateway 作为逻辑 owner 管理配对和连接；Host 绑定权限与工具调用；Coordinator 决定 run 所用执行位置；原生 UI 管理输入手势和帧呈现但不直接执行远端命令。可复用既有 Rust box/local Runner 的职责，不能同时新增互相绕开的第二套命令执行服务。

## 配对、授权与连接状态

设备身份至少绑定稳定 deviceId、账户、可信公钥/认证会话、用户给定标签、能力集及最后连接状态。配对码或邀请须短期、单次、与预期账户绑定，并要求用户确认目标；显示名不是安全身份。不得凭局域网发现结果直接授权。

状态：unpaired → pairing → paired-offline → connecting → ready；revoked、incompatible、reauth-required、unavailable 是显式分支。连接成功只说明传输可用，不能自动授予所有控制权限。能力批准应有设备、资源、用途、有效期和可撤销范围。

撤销先让本地授权失效并阻止新命令，再向远端传播撤销、结束会话与清理缓存；网络离线时也不能继续使用已撤销的本地 grant。仅关闭页面与解除配对不同；产品必须让用户看懂正在发生哪一个动作。

## 命令与结果生命周期

内部关联字段：accountEpoch、deviceId、connectionGeneration、runId、commandId、deadline、permissionDecision、viewportRevision。外部 wire 字段必须从当前 Desktop/网关协议抽取，不强行改名。

发送之前检验授权与实际 capability；持久化有必要恢复的命令身份，标记 issued 后接收 acknowledged/progress/result。网络断开不意味着远端没执行。对输入、点击、文件写入等副作用，不允许自动重新播放一整组命令；查询确认或暴露 outcome-unknown，必要时交给用户核对。

截图/视频帧与输入坐标必须携带一致的显示器、缩放、旋转、裁剪区域与 viewportRevision。旧帧上的点击不得作用到布局已变化的新页面。手势需转换为远端坐标，区分拖动、滚轮与本地缩放；键盘输入遵守明确支持范围，不猜测 IME 文本。

## 人类接管与运行恢复

run 进入等待人工接管时，暂停 Agent 新控制命令，释放或转移控制租约并显示目标设备与任务。用户结束接管后，先获取新的 viewport/应用状态并确认续跑条件，再恢复 Agent。接管超时、设备掉线、账号切换和权限撤销均须有可见结果，不能默默恢复自动控制。

Android 进程死亡后只恢复持久任务和连接意图，不恢复旧网络句柄或旧页面 nonce。foreground 通知可以说明仍有用户主动发起的任务，但前台服务使用必须符合系统条件；不能承诺通过保持通知就永久运行任何任务。

## 端到端验证

两台受控测试端：Android 与已授权桌面/测试 Runner。验证正确目标的配对、拒绝错误账户/过期配对码、重连与撤销、变更分辨率后的坐标正确性、远端已执行但响应丢失时不盲重放、重复 commandId 行为、human takeover 停止 Agent 输入、Android 重启后结果核对、远端版本不兼容时 fail closed。

必须从产品入口完成配对和任务，而不是直接注入 trusted-device fixture；真实最终产物的 App Surface 与网关路径需覆盖。日志保存关联标识与结果摘要，不默认收集私人桌面截图、键盘输入或凭据。

## 参考

[Desktop 构建与资源声明](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/desktop/package.json)；[Android 后台/前台服务限制](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start)。Android 跨应用能力是否可分发，须独立取得官方 API/商店政策和受支持设备证据，未取得前不声明该平台能力已等价交付。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现入库为 Android 实施规范；不构成产品实现和测试通过报告。设计状态、owner 和字段不冒充后端 wire。实现必须补上 Desktop 源码合同、唯一 shipping owner 与 exact-HEAD 的反例测试，只通过 GitHub Actions 构建和验证。

源锁：`bhrumom/fabushi-desktop@3bc92400826cc4ca7ac665b467708e22261edc61`，8,172 个 tracked 文件的初始 ledger 均非 verified。
