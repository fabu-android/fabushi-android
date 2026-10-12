# 生产切换、旧实现清理与回滚

## 切换单位

以 responsibility/owner 为单位，不以重命名目录为单位。对每项旧 UI/runtime/持久化路径记录调用者、数据、消费者、新目标、验证证据及删除条件。禁止永久保留两份 Coordinator、transcript、账户或工具执行真相。既有 mobile/android 包装工程可以保留；必须删除的是被替代的业务/状态所有权。

## 安全过程

先让新 owner 在测试 fixture 和受控环境读取相同状态，建立映射和一致性断言；准备数据迁移和恢复；将生产调用者一次性切到新 owner；通过真实路径与旧版升级场景；确认无剩余调用/存储写入后删除旧代码。临时兼容 adapter 有到期条件、单向职责和测试，不允许自身持有第二真相。

可以用受控 feature switch 灰度 UI 路由，但不能让两个 runtime 同时产生副作用。回滚 UI 前确认新 schema 是否允许旧读者。不可逆迁移需提供明确恢复/修复方式，不能承诺安装旧 APK 就能读取所有新数据。

## 切换检查

检查 manifest/component、Gradle sourceSets、Cargo members/path deps、JNI staging、路由表、WorkManager unique names、notification channels、deep links、database migrations、backup rules、CI paths、release smoke。旧路径从编译图消失不等于旧账号数据安全迁移，必须用真实旧格式 fixture 验证。

验收在迁移开始、事务提交前后、附件拷贝、token rewrap、进程退出时注入 kill/空间不足，确认旧数据可恢复或明确进入 repair 状态。清理不删除非本任务拥有的用户文件。文档/ledger 保留历史 provenance，不能删掉旧记录让覆盖率看起来更高。
