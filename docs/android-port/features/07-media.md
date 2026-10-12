# 07 — 文件、附件、媒体、通话与离线语音

## 覆盖与不变量

分别处理文件选择、拍摄、图库、附件上传/下载、语音消息、语音输入、离线转写、音视频播放，以及经 Desktop shipping 审计确认存在的通话/屏幕媒体职责。不能把“能选择文件”当作全部附件 parity，也不能把麦克风用途声明当作已经实现音视频通话。

媒体基础设施的核心不变量是：唯一麦克风占用者；权限来自用户和系统；原始文件与临时文件身份可追溯；取消后资源释放；后台或进程死亡不重复发送；转写默认写入可编辑草稿，不自动发送。

## 来源与包装

已读 Desktop `desktop/package.json` 包含 `resources/asr` 与 `resources/computer-control` 的打包声明及麦克风/摄像头用途文本。它证明包装合同存在，不足以证明每种媒体能力的完整运行路径。实现阶段继续审计实际 ASR manifest、capture owner、附件 projection、发送/上传协议、录音 UI、播放器和通话生产入口。

Android 将采集、权限、ContentResolver、AudioFocus 和系统媒体 API 适配放在 `source/android-main`；JNI 负责受类型约束的音频/文件句柄传递；Host 负责附件注册、上传调用和业务状态；Compose 只呈现附件和录音投影。

## 附件模型与提交顺序

内部记录至少包含 accountId、attachmentId、来源 URI 或私有文件引用、文件类型、字节长度、摘要、所有者会话、上传操作身份、remoteReference、状态与错误原因。名称和扩展名不能替代内容类型检查。对外字段从 Desktop 实际 wire 提取。

状态设计：selected → validating → staged → uploading → available → attached；失败、取消和权限撤销单独记录。上传完成不等于消息发送完成。先持久化上传结果与发送意图，再发送带幂等身份的消息；重启时查询已提交结果。内容相同可能允许复用存储，但不得跨账户复用私密远端引用。

Photo Picker/Storage Access Framework 获得最小 URI 权限；只有确有长期访问需要且提供者允许时才持久化 grant。不能把所有 URI 当文件路径。临时复制前检查空间和配额，复制过程中使用有界流，失败删除未完成临时文件；不能一次把巨型视频载入 Kotlin/JNI 内存。

## 录音与离线转写方案

设计选择：统一 AudioCapture owner 对语音消息、语音输入和通话申请做互斥/切换；默认的“录音后转写”走可验证的本地 PCM 到离线识别引擎接口，避免同时开启第二个麦克风捕获。引擎和模型须固定来源、许可、摘要、架构、内存与准确度基准，纳入 native packaging 与 16 KB 验证。模型选型尚未经项目设备基准，不能直接把某引擎版本写成已决定、已可发布。

Android 的 `SpeechRecognizer.createOnDeviceSpeechRecognizer` / `isOnDeviceRecognitionAvailable` 可在受支持的 API/设备上作为原生本地识别适配。必须实际检查服务和语言能力；不能依赖 `EXTRA_PREFER_OFFLINE` 来承诺离线，也不能假定任意版本都能消费已有录音。采用它时由统一 capture broker 把麦克风所有权显式移交，不同时运行另一录音器。

没有离线服务或模型时显示“离线转写不可用/模型未就绪”。不得悄悄回退云识别。若产品提供联网转写，应是独立、清楚标识且用户授权的功能。飞行模式验收必须证明目标语言的真实音频能完成转写，而非返回 fixture。

录音状态：idle → requesting-permission → recording → stopping → transcribing → editable-draft；cancelled / denied / interrupted / unsupported / failed 是明确终态。captureSessionId、accountEpoch、conversationId 和 draftRevision 共同防止晚到识别结果覆盖另一会话或用户已经修改的草稿。取消转写不能删除已成功发送的语音消息。

## 播放、下载与通话

播放器复用受控播放 owner，处理 AudioFocus、蓝牙/耳机切换、拔出耳机、电话打断、速度和位置恢复。下载必须有大小上限、可取消状态、摘要与来源校验；向外分享经系统授权 URI，不暴露应用私有目录。

经源审计确认的通话责任必须独立迁移呼叫/接听/拒绝/挂断、信令、音视频轨、静音、摄像头切换、音频路由、失联恢复与远端结束。媒体采集权限和 Android 前台服务资格需依 API 实际检查。不能保证系统杀进程后本地通话无限继续；恢复必须显示真实远端会话状态。

## Actions 验收矩阵

至少覆盖：零字节/损坏/超限文件；URI 被撤销；空间不足；上传完成但消息未确认时进程死亡；重试不重复消息；切换账户拒绝旧附件；录音权限永久拒绝；录音中来电/蓝牙变更；取消与完成竞争；转写返回前修改草稿；离线服务不支持；没有网络的真音频识别；release R8 下 native engine 加载；各发布 ABI 的 native 库和 16 KB 环境运行。

与 Desktop 比较功能效果而非桌面快捷键外观，给出语音时长、语言、设备、模型摘要、峰值内存、耗时、结果准确度定义与失败样本。音频样本使用有授权的测试素材，不上传用户私密录音到公开工件。

## 参考

[SpeechRecognizer](https://developer.android.com/reference/android/speech/SpeechRecognizer)、[Photo Picker](https://developer.android.com/training/data-storage/shared/photopicker)、[SAF](https://developer.android.com/guide/topics/providers/document-provider)、[FGS 限制](https://developer.android.com/develop/background-work/services/fgs/restrictions-bg-start)、[16 KB](https://developer.android.com/guide/practices/page-sizes)、[Desktop 包装](https://github.com/bhrumom/fabushi-desktop/blob/3bc92400826cc4ca7ac665b467708e22261edc61/desktop/package.json)。API 细节和模型选型必须在实现所锁定的版本再次核对。

## 状态、追溯与使用规则

本文件原写于 2026-10-09，现为 Android 实施规范；不构成产品实现和测试通过的声明。内部模型与状态为设计约束，真实后端 wire 需重新审计固定 Desktop SHA，按实际符号、调用与测试锚点补齐。构建与验收只使用 GitHub Actions，禁止在 bhrum2 或 Mac 本地执行。

统一源锁：`bhrumom/fabushi-desktop@3bc92400826cc4ca7ac665b467708e22261edc61`。Actions 已登记 8,172 个源码文件条目，不代表逐职责 verified。
