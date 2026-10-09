# 单项产品职责记录模板

复制到持久 ledger 对应的职责文件，字段必须由源码与证据填写；本模板本身不计入实现覆盖率。

## 标识与来源

唯一 responsibility_id；领域；用户目标；Desktop repository/commit/path/blob SHA/符号/行范围；caller、callee、实际入口及关联 tests；源码是否 shipping；Android 基线 SHA；审阅者与日期。对一个源文件中的多个职责分别记录，不能用文件总数代替职责数。

## 责任与合同

指定唯一 owner 与 allowed callers；输入 schema、输出 schema、事件、error codes；账户/会话/run/请求身份；授权条件；顺序、并发、幂等、重试和取消；数据库事务及外部副作用提交点；超时和 outcome-unknown 的处理。协议名称和字段先从 Desktop 当前实现提取，设计新增字段明确标为 Android 内部合同，不冒充既有 wire schema。

## Android 实现

选择 direct-port / android-adapted / not-applicable-with-replacement；列全部 target paths、语言、实际 Gradle/Cargo 模块、入口接线、平台差异与用户可见替代效果、旧路径移除计划。不存在的目标路径只能标 planned，不能标已接线。

## 验证与状态

列正常流程、错误路径、跨账户、重复请求、进程死亡、离线、取消竞争、版本升级等具体 Given/When/Then；每个测试关联真实 assertion 与 artifact。记录 implementation_status：unreviewed / mapped / implemented / verified。implemented 不等于 verified。verified 必须填 Android SHA、Desktop SHA、run/attempt/job、artifact digest、设备/API、结果与独立 reviewer。

## 失效条件

source blob 改动、caller/contract 改动、toolchain/schema 改动、target wiring 改动或证据 artifact 失效时列受影响范围和重验任务，保留历史而不静默覆盖。