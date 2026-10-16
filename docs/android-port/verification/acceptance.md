# 完成定义：文档工程与产品工程分开验收

## 文档工程

D01 固定 Desktop/Android 来源；D02 完整 tracked-file 库存且无截断；D03 每个领域有 owner/输入输出/状态/失败/验证设计；D04 原生架构、数据与 ABI 合同；D05 平台差异不隐藏；D06 分阶段依赖和可执行队列；D07 安全/发布/迁移与证据格式；D08 内部链接、JSON、来源锚点和脚本在 Actions 校验；D09 云端和本次交付清单准确。D02 只证明范围枚举，不证明逐函数设计完成。

## 产品最终门

| ID | 必须证明的结果 |
| --- | --- |
| P01 | Desktop main 与验收 authority 一致，所有变化完成 rebaseline |
| P02 | 所有 tracked source/资产/依赖已有 reviewed disposition；gitlink/LFS 已处理 |
| P03 | 所有 applicable responsibility 有 Android target、真实 wiring 与 evidence |
| P04 | Kotlin 原生 UI 完整，无整站 WebView 替代 |
| P05 | Coordinator/Host/Runner/bridge/store 唯一 owner 和依赖边界成立 |
| P06 | 独立 clean checkout 无另一 Fabushi 源码依赖即可构建 |
| P07 | 账户、登录、刷新、退出、凭据和跨账号隔离 |
| P08 | Human/Bot 消息、联系人/群组、草稿/未读/搜索及主线消息动作 |
| P09 | Agent/provider/model、流式/终态/取消/记忆/协作 |
| P10 | 工具/审批、幂等与未知副作用核对、人工接管 |
| P11 | 插件安装/更新/卸载与真实 MCP/连接器认证调用 |
| P12 | Mini App/App Surface/WebMCP 的会话、授权、fallback 与关闭 |
| P13 | 文件/媒体/录音和真正离线 ASR，适用设备范围明确 |
| P14 | 远程设备、撤销、权限和适用的 Computer Use |
| P15 | 工作流、checkpoint、调度时区与重启恢复 |
| P16 | 主线存在的业务/订单/权益/恢复全链路 |
| P17 | 后台、process death、Activity recreation、断网和重连 |
| P18 | 数据库/文件迁移、空间不足、崩溃一致性与旧版升级 |
| P19 | 通知/深链/设置、TalkBack、大字体、RTL、平板/折叠布局 |
| P20 | Rust/Kotlin/合同/集成/Compose/仪器 tests 同 HEAD 通过 |
| P21 | 性能、内存、能耗、JNI 资源、原生 ABI/16KB 页达标 |
| P22 | 威胁模型、依赖/许可、隐私、debug/测试入口隔离 |
| P23 | 同源签名 APK/AAB、minified 包、真机安装升级和渠道合规 |
| P24 | exact source/run/job/step/artifact/digest、独立 review、合并后复验 |

每项最终状态只允许 passed/blocked/not-applicable；not-applicable 必须理由、替代和审核。缺失证据、skipped job、mock success、历史 artifact、仅编译成功均不能填 passed。所有阻塞清楚保留，禁止分母删项、降级为 warning 或以目录对应率宣称完成。

独立验收从仓库/Actions/包重新读取事实；Work 自述只是线索。用户本轮要求文档，不意味着授权自动发布、购买或真实账户破坏性测试。
