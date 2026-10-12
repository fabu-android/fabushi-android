# F03 Agent/Bot、推理、流式与协作

## 来源和边界

源入口：`desktop/e2e/mahayana-agent-workbench.spec.ts`、source/host、source/node-agent-coordinator、当前 Rust 运行时与 agent packages。Coordinator 管 run 生命周期；Host 管推理/工具/业务；provider adapter 管模型协议；UI 仅投影。

保留主线 Agent/Bot 创建、配置、选择、身份、模型/provider/推理参数、指令、上下文、附件、工具过程、审批、记忆、子任务、多 Agent 协作、产物和用量责任。具体支持范围必须经实际入口和调用链确认，不能把旧规划当已实现功能。

## 推理和流式合同

模型能力显式描述工具、图片、推理等级等支持范围。不支持的输入返回明确错误。accept 时固定本次 run 的配置版本；用户后续改模型不得改写已派发的动作，也不能重复提交当前 turn。

UI 区分 waiting/thinking/streaming/tool-running/awaiting-user/suspended/final。增量按 event sequence 组合，断流执行 resync，不将部分文本直接结算。Stop 发出一个取消意图，保持 stopping 直至 canonical 终态。stream EOF、无 token、页面按钮变化不代表成功。

父子 run 保留 parentId、依赖与取消传播规则；子任务完成是否结束父任务必须遵循源状态机。并发会话使用有界调度和公平预算。人工接管暂停对应自动控制，恢复前重新确认权限和目标。

## 记忆、恢复与验证

会话、Agent、账户记忆按源边界隔离，保留删除/搜索/导出等实际动作。上下文压缩保存 lineage 与版本；恢复不能悄悄新开无历史会话。模型预算、附件限制来自能力和运行时策略。

使用真实受控 provider 验证文本、工具与附件路径，覆盖 rate limit、断流、cancel/final 竞争、Host 崩溃、切账号、长上下文、父子任务中止和公平性。比较 canonical 最终消息与工具结果，而非 canned response；记录成本上限，测试不读取或展示真实用户的 provider secret。
