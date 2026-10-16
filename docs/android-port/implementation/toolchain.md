# 工具链、构建入口与依赖闭包

## 当前事实与决策

当前包装入口是 `mobile/android/app/build.gradle`（Groovy），minSdk 26、compileSdk/targetSdk 37、applicationId `com.ombhrum.fabushi`。根 Cargo workspace 已有 Android 自有 Host/JNI/Coordinator 等成员。此文不改变这些配置，也不把当前版本写成已证实最佳或可成功构建。

先读取 wrapper/校验和、settings、根 plugins、Cargo.lock、rust-toolchain、CI 的真实文件；缺 lock 或 pin 作为待修复项。以可复现和当前官方兼容矩阵决定 JDK/AGP/Gradle/Kotlin/Compose/NDK/Rust 组合。必须固定完整版本、下载校验和、依赖验证与原生目标；不能写 latest 或每次运行升级。

## Actions 中的构建图

检查 sourceSets → 编译 Rust workspace → 为 arm64-v8a 与 x86_64 等正式支持 ABI 交叉编译 JNI/native libs → 受控 staging → Kotlin/Compose 编译 → unit tests → instrumentation → minified release APK/AAB → 包检查 → 安装/升级证据。明确架构只支持哪些 ABI，未支持的 ABI 不能生成假空库。测试模拟器 ABI 不代表真机 arm64 构建。

Gradle tasks 由当前工程真实列举后写入工作流；常规入口为在 mobile/android 中使用版本库 gradlew，测试与 release 选择对应现有 variant。文档不假定不存在的 task 名可直接运行。Rust 使用 Cargo.lock 的 locked 模式，Android link/NDK target 指向受控工具链。生产 build 必须从本仓库源闭包完成，不 clone 另一 Fabushi 产品补缺。

## Native 兼容

每个 so 记录 ABI、来源、许可证、构建 revision、依赖、ELF/ZIP 对齐。16KB 页兼容需要检查并在相应环境运行，不能仅升级 NDK 就宣布全部第三方库兼容。JNI 经 R8 后仍能注册/调用；minified release 要做实际 smoke。ASR/媒体/加密等附加库全部包含在清单。

## 执行边界

所有解析验证、测试、构建、打包仅 GitHub Actions；不在 bhrum2、Mac 或会话沙箱跑产品 build/test。构建缓存有尺寸与保留策略，不能把未验证缓存当二进制来源。CI logs/artifacts 不含 keystore、凭据或用户数据。签名只在受保护 release job，PR 文档校验不读取 secrets。
