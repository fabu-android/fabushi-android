# 目标目录与既有工程接续

目录表达职责；不能以同名空文件冒充移植。下列为目标边界，部分目录已存在，部分为实施时按职责建立。新增前检查当前 tree，保留可用实现并更新 Gradle/Cargo 指向。

```text
frontend/                         # Compose screens, design system, projection consumers
source/android-main/              # platform composition/lifecycle/auth/media/files/notifications
source/android-preload/           # trusted typed app bridge, no domain truth
source/android-host-jni/           # sole Kotlin/Rust ABI facade
source/android-dev-controls/      # debug-only, absent from public release
source/mahayana-agent-coordinator/ # run lifecycle, transport, ordered projection, supervision
source/host/                      # domain execution and native messaging composition
source/local-exec-daemon/          # approved on-device execution, bounded resources
source/box-exec-daemon/            # authorized remote execution, not a required local daemon
source/shared/                    # Android-owned wire/domain contracts
source/internal/                  # platform-independent internal policies
source/packages/                  # Android-owned agent/transcript/inference/MCP support
source/packaging/                 # native library/model/license packaging manifests
mobile/android/                  # current Gradle packaging root and manifests
native/ ; third_party/            # only when licensed imported dependency closure requires
contracts/                       # versioned public/backend fixtures and journey definitions
manifests/ ; tests/ ; scripts/    # packaging contracts, cross-module tests, CI entrypoints
docs/android-port/               # this implementation and acceptance authority
```

当前 `mobile/android/app/build.gradle` 已将 main.kotlin.srcDirs 指向根目录 frontend/android-main/android-preload。不要因为旧 Spec 提到“移除 mobile/android”而删除有效包装入口；最终须移除的是重复业务实现与 stale bypass。Gradle 包装工程可保留在 mobile/android，若搬迁必须同步 wrapper、settings、sourceSets、manifest、JNI staging、CI 与测试。需要目录变更时以新 ADR 记录，不把搬目录当产品交付。

## 模块拆分的规则

UI 按 account/messaging/agent/marketplace/miniapp/media/remote/settings 划分 feature，domain API 不依赖 Compose。Android-main 按平台资源 owner 划分 auth/files/media/notification/lifecycle。Host 的消息、agent、市场、连接器和工作流可以独立 crate/module，但统一账户与持久化入口。共享 DTO 不导入 Host 具体实现。

每个模块 README 写明公开 API、状态 owner、依赖方向、取消/关闭规则、错误、focused tests。测试目录按 unit/contract/integration/instrumentation/packaged/performance 归档，路径对应具体职责，而非仅按某个历史源文件名。

文件级 mapping 保留 Desktop path/blob 与 Android target path/symbol；可一对多、多对一，但不能把 Coordinator/Host/Runner 合并成一个巨型 ViewModel。所有已被替代的 production paths 放入 removal ledger，先证明调用者迁移后删除。
