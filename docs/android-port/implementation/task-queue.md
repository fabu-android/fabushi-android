# 执行任务队列与交接输入

每次会话开始重新读 Desktop main、Android PR #3 HEAD、当前 CI。下表是实施顺序建议，不是已完成列表；实际最早可执行项由依赖和最新证据决定。

| 工作包 | 唯一输出 | 依赖 | 验证 |
| --- | --- | --- | --- |
| A01 全仓库文件库存 | 固定 tree、所有 leaf、模式/哈希、来源摘要 | 仓库只读访问 | API 未截断、根 SHA、路径集合相等 |
| A02 逐职责映射 | source symbols/callers、owner、targets、差异、测试 | 对应源内容完整可读 | 人工审阅，禁止路径名自动标 mapped |
| A03 工具链锁定 | Gradle/JDK/SDK/NDK/Rust/ABI/依赖闭包 | 已有 build files | Actions clean checkout 与依赖解析 |
| A04 wire/ABI/storage | DTO 映射、golden fixtures、生命周期与迁移 | A02 对应责任 | 协议、进程恢复、内存/句柄边界 tests |
| B01 账户与 readiness | 单一账号 scope、hydrate、退出与恢复 | A04 | 受控真实登录、切账号反例 |
| B02 Human/Bot 消息 | canonical messaging 与原生入口 | B01、message contract | 跨端消息/搜索/草稿/未读 |
| B03 Agent 执行 | 模型、流式、终态、取消、工具/审批 | B01、run contract | 真实 provider、race/副作用核对 |
| C01 市场/MCP/Mini App | 安装到实际调用与业务状态 | B01、broker、bridge | 实际 connector 与完整 surface journey |
| C02 媒体与离线语音 | 文件、录音、转写、播放器 | platform resources、附件合同 | 权限/空间/断网/旧结果反例 |
| C03 远端与自动化 | 配对、控制权、workflow/checkpoint/schedule | broker、run、storage | 真实设备、恢复和去重 |
| C04 业务权益 | 订单/验证/恢复/撤销 | B01、C01、当前业务合同 | 受控交易/恢复，不真实用户购买 |
| D01 原生质量 | 全导航、平板、a11y、i18n、通知 | 对应生产路径 | Compose/仪器/可访问性 tests |
| D02 legacy removal | 移除替代 owner 与失效依赖 | 对应当前实现 verified | 依赖图、运行 trace、升级/回滚 |
| D03 release acceptance | 签名 APK/AAB、安装升级与 evidence | 所有 applicable 责任闭合 | 完成定义逐项 evidence |

每项至少拆为单一可描述用户效果/状态机，不能一次把整个 source/host 标已完成。提交信息包含责任 ID；PR 说明记录 source SHA、变更、未解决条件、run/job/artifact 和下一项。生成文件库存不自动领取/实现任何业务责任。

外部 blocker 用 operations/blockers.md 登记，不将 unknown 当 not-applicable。主实现只维护一个队列；独立验收可以退回某一项并给出可验证差异，不直接代替执行者改代码。
