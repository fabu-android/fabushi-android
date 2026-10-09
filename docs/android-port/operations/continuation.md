# 实现会话与独立验收会话接力

## 主实现会话入口

读取根 AGENTS、ANDROID_PORT、总规范、baseline、task-queue 和最新 GitHub PR #3。重新读取 Desktop main 与 Android PR exact HEAD，禁止只用聊天记忆中的 SHA。发现 authority 移动时先列变更影响并更新来源记录，未受影响的独立职责可继续审计；不能继承已过期证据。

每轮选择最早的依赖已满足职责：读取完整源实现、直接调用者、状态/协议与 tests；按模板写事实、目标 owner、Android 差异、输入输出和正反例；实现生产代码和 focused tests；提交同一 PR；只在 Actions 执行验证。出现失败要读到失败 step/log，不把 pending 当 success，不靠取消工作流、skip 或放宽断言过关。

一个提交必须能回答：用户能做什么；哪个 owner 负责；状态何时持久化；重入/重试/取消如何处理；调用接到哪个实际 shipping 入口；哪些 Android 测试可证明。功能只存在于未接线类、假的服务或测试 fixture 时不能标 implemented-complete。

## 卡点处理

只阻断依赖它的任务；把依赖图中已经就绪的下一项继续推进。需要用户提供权限、账号、真机或确认时，先查询 Gmail 同主题线程；尚未解决的同一问题不重复通知。邮件不包含秘密。再次检查若已解决就恢复该任务，否则推进其他可执行项。不得用其他账号/设备绕过工具安全拦截。

## 独立验收入口

只读调查，不代替 Work 修改生产代码。读取原始目标、本规范、当前代码、Actions 与 artifacts；把 Work 的叙述视为待验证主张。逐条检查 source authority、真实 shipping wiring、所有合同与状态机、production account 同一性、进程死亡恢复、release 包、安装升级、证据来源。

最终报告给出 passed / blocked / failed / not-applicable，列 exact SHA、workflow/run/job/step、artifact ID/digest、设备与系统、未解决职责。未读日志、缺少真机、测试跳过、仅文档通过都不构成完成。

## 每轮交接格式

记录 observedAt、Desktop SHA、Android SHA/branch/PR、已提交变更、已读 source anchors、当前 run 状态、确定性失败、可独立推进项、阻塞邮件 threadId，以及下一项最小具体动作。不携带 cookie、token、私钥、账户密码或个人消息正文。