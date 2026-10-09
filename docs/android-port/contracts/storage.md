# 数据模型、持久化、迁移与一致性

## 单一 canonical storage

保留并迁移 Desktop 实际持久化语义到 Android 自有 Rust repository。若当前 Android 的 DB 格式不同，用显式 importer 和版本迁移，不要求 SQLite 表名与 Desktop 相同。Room/DataStore 仅用于不冲突的 UI 偏好与可重建缓存；不再存另一份 canonical conversations/messages/runs/grants。所有敏感记录带 account scope，查询和索引必须包含该 scope。

逻辑实体：AccountSessionMetadata、Conversation、Participant、Message（稳定 server/local IDs）、Draft（revision）、Run、RunEvent/Checkpoint、OutboxCommand、ToolCall、ApprovalGrant、Attachment、InstalledPlugin、ConnectorConnection、WorkflowSchedule、EntitlementProjection、DeviceBinding、TelemetryCursor。每个实体必须在职责记录映射到源字段与原生 DB migration，不能照此清单凭空创建不兼容后端数据。

## 原子写入

业务变更、outbox、idempotency record、event cursor 在同一事务内；提交后才发布 projection。intent 落盘但未发送可重试，发送但未 ACK 的命令查重/核对；已完成副作用不因 UI crash 再执行。附件 content 写临时文件、校验 size/hash 后原子 rename，再提交 metadata；orphan cleanup 有保留窗口与引用检查。

排序使用服务端 cursor/sequence 或明确冲突规则，不依赖手机墙钟。草稿使用 revision/CAS，语音迟到结果不得覆盖新编辑；已删除会话不能被迟到消息盲目重建。数据库 busy/损坏/磁盘满分别有错误态，不通过清库让测试通过。

## 密钥、备份与退出

Keystore 管理包装密钥；refresh token、provider secret、device credential 按存储威胁模型加密。token 不进入通用 JSON dump、backup、截图和崩溃日志。备份规则明确哪些数据可迁移、哪些凭据必须重新登录。用户退出先 fencing，撤销/清理账户凭据与订阅，关闭旧任务回调，再切 UI；不能仅清屏。

## 升级和回滚

schema version 独立于 app version。每个 migration 说明 preconditions、transaction、postconditions、失败恢复和降级策略。发布前用真实旧版 DB fixture 验证中途 kill、空间不足、密钥失效。不可逆迁移不能承诺旧 APK 直接降级读取；采用兼容窗口或安全导出/恢复路径。删除 legacy store 要在新 owner 验证后、可恢复备份策略内执行。

## 验收

一条消息/工具命令在每个 crash 点最多产生一个可确认副作用；数据库与 UI 最终一致；A/B 账户不能通过搜索、附件 URI、日志、通知串数据；断网重启恢复草稿、未读与 run；升级不丢消息、不重新授权已拒绝能力、不恢复已撤销设备。
