# 10 — 原生导航、聊天呈现、交互与可访问性

## 用户体验目标

Android 是完整原生产品，不是把桌面 sidebar 缩成一张手机截图。用户必须能完成 Desktop 实际可达的账户、会话、联系人、Agent/Bot、插件、Mini App、设备、工作流和设置操作；信息架构可适配触屏，但不得悄悄丢失低频入口。

首先制作 Desktop shipping-route → user-goal → Android entry → back behavior → state owner 的 route register。以真实桌面 renderer 和 e2e 为证据，不把目录中未接线页面视为已 shipping。共享 journey 类型中的八个入口只是种子场景，不是整个产品导航清单。

## 页面与状态边界

Compose 页面在现有 `frontend/src/main/kotlin` 内实现，跟随既有 Gradle sourceSets。Presentation 层将 Host/Coordinator 状态映射为不可变 UI projection；ViewModel 不建立第二份业务数据库，不根据屏幕是否可见重启或重复提交任务。

路由内部至少区分账户、workspace、conversation/agent、目标资源与导航来源。路由参数只携带身份引用，不能放凭据或完整大对象。恢复导航栈前确认账户已恢复、目标仍存在且有权限；失效链接回到明确错误/选择页，而非错误账户下的相似会话。

手机采用一至两层主要导航，平板/展开屏可显示列表与详情双栏。布局模式切换只改变呈现，不创建新的业务会话。具体视觉 token 与组件要从 Fabushi 既有设计资产和真实产品截取建立对应，不在本文件虚构像素或色彩已经确定。

## 聊天与编辑器

输入草稿由 accountId + conversationId 管理；区分编辑已有消息、回复、转发、附件队列和 Agent 新 turn。发送动作只有一个入口和稳定 operationId，连续点击不能多发。流式 token、tool call、approval、附件进度和最终回复都显示为同一 canonical transcript 的投影，不能因刷新视图拼出重复 assistant 消息。

进入其他会话、旋转、折叠屏变化或外部分享后返回，草稿与滚动位置应可恢复。用户已编辑的草稿不能被晚到的语音识别或深链数据无条件覆盖。自动滚动只在用户停留在最新消息区域时触发；用户回看历史时提供“新消息”提示。

文字选择、复制、代码块、链接、附件、消息操作菜单与键盘动作按真实 Desktop 功能逐项实现。桌面右键菜单映射到可发现的长按或显式菜单；hover-only 操作必须有触屏入口。软件键盘、外接键盘、中文组合输入和换行/发送策略须各自验收。

## Back、深链与生命周期

Back 的优先级由界面状态决定，例如关闭临时弹层/选中模式，再关闭 surface，再退出会话详情；不能每次都终止后台业务 run。支持系统返回语义的实现需跟当前 AndroidX/API 验证，取消预测返回不得提前提交破坏性动作。

深链来自通知、分享、OAuth、插件或外部网页时，先验证 scheme/host/path/参数和账户归属，再进入目标。OAuth callback 与普通导航必须分开解析。冷启动过程暂存一次性导航意图，登录/恢复完成后最多消费一次；重复 Intent 不得重复执行工具或发送消息。

Activity recreation 恢复显示状态；process death 恢复 durable owner 状态；两者测试不能混为一谈。保存 UI state 不是保存 Rust 运行句柄，不把失效 JNI 指针持久化。

## 可访问性与国际化

设计要求：关键可点控件具明确名称、角色与状态；语义读取顺序与视觉任务顺序相符；图标按钮不能只读出文件名；错误既有可见文字又可被辅助功能获知。动态字体、对比度、横竖屏、触控目标、外接键盘焦点和屏幕阅读器作为发布验收项。

流式文本不应每个 token 都强制播报；按稳定片段/最终消息和重要审批状态发出可理解的更新。无障碍动作与触摸动作通向同一批准/发送合同，不能有隐藏的越权捷径。

文本从资源系统读取，处理复数、占位符、日期、时区和长文本；至少对产品支持语言建立布局与端到端矩阵。需要 RTL 的语言按实际支持声明验收，不声称只翻译字符串就完成国际化。

## 测试与完成

Compose 测试覆盖投影、空/加载/错误/部分数据、真实导航和交互；设备测试覆盖旋转、Activity 重建、独立进程杀死、冷启动深链、权限拒绝、字体放大、键盘组合输入、屏幕阅读器、平板和展开屏。

每项受支持功能都要有可达入口证据。截图只能证明某帧外观，不能代替按操作顺序执行的断言；最终 release 包必须跑关键流程，避免 R8、资源缩减或 manifest 差异只在发布时暴露。

## 参考

[Android 既有 Compose 构建入口](https://github.com/fabu-android/fabushi-android/blob/5d049353e167d9b91b5c765e1686f1ac8670f2ff/mobile/android/app/build.gradle)、[Desktop 种子旅程定义](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/frontend/packages/shared/src/mahayana-host-features.ts)。视觉细节与完整导航表仍须通过 pinned Desktop 的实际页面和源码逐项填证。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现入库为 Android 原生导航实施规范；不代表已完成全部页面或产品测试。内部状态是设计约束，真实 shipping 页面与后端合同仍需定位并按当前 Desktop 精确对照。只在 GitHub Actions 构建和验收。

固定 Desktop main：`3bc92400826cc4ca7ac665b467708e22261edc61`。登记 8,172 个文件不构成逐文件语义验收。
