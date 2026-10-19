# 来源、许可证与依赖闭包

本文件是工程发布控制，不是法律意见。拥有 GitHub 读取权限不自动等于拥有复制、改作或再分发权。

## 来源登记

每次导入 source、assets、字体、模型权重、原生库、协议生成器或第三方代码时，登记 repository、commit/tag、path、blob/digest、上游许可证、版权通知、修改范围、再分发方式和审查结论。软件许可证与模型/数据许可证分开登记；不能用顶层 LICENSE 代替逐目录检查。

Desktop 是产品语义权威，不自动授权其全部 third_party 代码。现有 Android workspace 标记 UNLICENSED，不能被当作已解决对外分发许可。历史 Grok reconstruction 仅供经过权利审查的行为/架构参考；保留既有 docs/reviews 中的权利记录，新的导入单独复核。

## 完整闭包

从 APK/AAB 中实际包含的 native libraries、资源及依赖反向核对 Cargo/Gradle 锁文件与源清单。Gitlink/submodule 必须记录固定子仓库 SHA、其递归闭包及许可证；Git LFS 指针必须核对实际对象摘要和可取得性；指针存在不是内容已审计。

Android runtime source 必须由本仓库所有，不依赖兄弟 Fabushi checkout。第三方公开包可作为锁定且可复现的依赖，不应误把“不跨 Fabushi 源仓库”理解成“不能使用任何第三方依赖”。服务端 API 属于产品运行依赖，必须单独记录，不伪称完全离线。

## 发布门

产出 machine-readable SBOM、NOTICE、license inventory、native-symbol provenance 和模型许可证清单。所有 shipped 组件均应可追溯；未知来源资产及许可冲突阻断发布。Android 包中的字体不得通过本次聊天单独分发，产品使用也须独立确认许可。

验收记录应包含审查者、日期、对应 exact HEAD 与 artifact digest。依赖升级、模型更换和资源替换都会使相关审查失效，不能沿用旧审批。