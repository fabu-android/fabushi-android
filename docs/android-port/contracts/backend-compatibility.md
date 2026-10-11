# 后端、协议与跨端兼容

Android 是 Desktop 同一产品的原生端，不建立平行用户、消息、市场或权益后端。本任务不授权修改服务端。先从 Desktop current main 的实际 clients/serializers/handlers/contracts/tests 取得 base URL 配置、method、版本、字段、编码、认证、分页、幂等、错误与时间语义，产出 Android-owned contract fixture；不得把本手册的示意字段直接当外网 API。

## 提取记录格式

每个端点/方法记录：Desktop path+blob+symbol；transport 与 verb/method；request/response schema；nullable/missing/default；ID 精度；timeout/cancel；auth audience/scope；error mapping；pagination cursor；side effects/idempotency；retry；兼容版本；真实调用入口。golden fixtures 去除 token/用户内容；测试“可解析”之外还要验证发送值与作用域正确。

## 必须一致的交叉语义

账户身份、Bot/Agent/Conversation/Message 的 stable ID；设备注册与撤销；附件引用与上传完成；未读/已读 cursor；插件 installed/version/connection 状态；provider/model 支持能力；审批 scope；run/thread lineage；订单/权益/恢复状态。显示文本不得代替机器 ID。客户端不能信任其他端发来的未经服务端授权的 entitlement/grant。

## 兼容演进

新增 optional field 能向后读；未知必需 variant/version 失败并提示升级，不默认执行危险动作。保留 wire names，Android 内部可以采用 idiomatic names，但 serializer mapping 有 tests。重试只限已知安全或带 backend 幂等的操作；401 单次协调刷新，多个请求共享刷新 owner，失败回到明确登录态，不 refresh storm。

## 契约测试与生产证据

mock fixtures 用于确定性测试，staging/受控账户用于真实序列：Desktop 创建会话 → Android 读取/回复 → Desktop 同步；插件安装/授权状态跨端核对；退出/撤销后旧 token 被拒绝。生产账号由 protected environment 注入最小权限短期会话，不写仓库与 artifact。无法访问服务端时其他模块继续实现，但对应 compatibility 不标 verified。
