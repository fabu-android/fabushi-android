# 阶段、依赖与完成门

阶段是依赖图，不是只按编号等待。一个外部条件阻塞时，登记责任和证据，然后推进依赖已满足的任务。不能绕过安全门，也不能为等待整个源清单停止无依赖的工具链/原生布局研究。

| 阶段 | 输入和输出 | 出口条件 |
| --- | --- | --- |
| P0 权威与范围 | 当前 Desktop main、Android PR；生成完整树/文件表、旧文档差异 | 每个 tracked leaf 有登记；gitlink/LFS 状态明确；不宣称语义审计完成 |
| P1 合同与底座 | 按文件/调用链分解 responsibility；建立 wire fixtures、owner 图、ABI、DB 迁移 | 每个开工职责有完整 Spec 与源锚点；clean checkout 工具链在 Actions 可运行 |
| P2 Native runtime | 接续既有 Rust/JNI/Compose 组合；实现唯一账户、事件、状态、恢复 | 真正从 app 入口经过 Coordinator/Host/Runner；进程死亡无重复副作用 |
| P3 核心产品 | Human/Bot 消息、Agent、模型、附件、审批和搜索 | 各职责正反例和真实服务闭环；不是 mock demo |
| P4 扩展产品 | 市场/连接器/Mini App、远程设备、媒体/ASR、工作流与商业合同 | 与核心共享同一账户/运行时；依赖与平台差异明确 |
| P5 质量与切换 | 全导航、a11y、国际化、性能、隐私、legacy removal | 不存在双生产 owner；升级/迁移/失败恢复通过 |
| P6 包和发布 | 可重现 APK/AAB、签名、真实设备、受保护账户、渠道审核 | exact HEAD 完整验收与独立 review；发布另需明确授权 |

P0 源库存是全局范围门，P1 审计按责任逐个闭合。P3/P4 中无依赖的 UI、状态机、合同 fixture 可并行；依赖真实权限/后端的验收可以 blocked，但不能先标 verified。每个领域都要回到当前源复核，不能因为排进阶段表就视为功能已覆盖。

每轮只对可审计范围提交一个聚焦变更：source → Spec → production → focused tests → Actions evidence → ledger → compliance。合并旧迁移架构时优先保持可运行纵向路径，但最终不以纵向切片为完成定义。
