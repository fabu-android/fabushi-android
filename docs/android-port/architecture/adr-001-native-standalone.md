# ADR-001：原生 Kotlin UI + Android 自有 Rust 运行时

状态：本迁移采纳。决策依据：已有 Compose/JNI/Rust workspace，以及 PR #3 的独立平台所有权要求。

选定 Compose/Kotlin 实现原生交互、无障碍、生命周期、媒体和系统 API；Rust 承担账户范围内的 Coordinator/Host/Runner、持久化与可移植业务核心；Kotlin/JNI 使用窄、类型化、异步桥接。已有 Rust 模块优先修复/补齐，而非再次起一个共享核心。任何从 Desktop 移植的代码在本仓库有明确 provenance 和许可记录，构建不 checkout 其他 Fabushi 产品源码。

没有选 WebView 整站套壳：它不能自然解决平台生命周期、原生状态、后台与权限所有权，且容易形成另一条运行时。没有选 React Native/Flutter 全量重写：当前已有原生 Compose，新增运行时会增加迁移面；除非后续有可复核的性能/维护收益和用户确认，不切换。没有为语言统一强制 Kotlin-only 或 Rust-only：平台 API 与状态机采用各自更合适的边界。

逻辑 actor 边界不是 OS 沙箱。Coordinator/Host 在同一进程时仍必须隔离状态与职责；不受信任的插件不能因此获得 app 进程任意执行。需要安全隔离的 runner 使用明确沙箱/隔离服务或授权远程 runner；普通 Service、独立线程、Mutex 都不构成权限隔离。

当前现有 JNI 继续作为切入点，不同时引入第二套桥。若改为生成式绑定，先证明 ABI、取消、回调寿命、线程、旧安装兼容和工具链闭包，再以独立 ADR 原子切换。

代价：Android 需承担自有 runtime 升级和上游差异审计；收益：可独立构建发布、可按移动端安全与性能优化、责任和故障边界清楚。验证：clean checkout 构建；依赖闭包扫描；UI 无 runtime 直写；生命周期重建不重启业务 run；不同 account 的任务与凭据不可串用。
