# 桌面机制的 Android 替代合同

| Desktop 机制 | Android 责任实现 | 必须保留的效果与限制 |
| --- | --- | --- |
| Electron main/window/tray | Application + Activity + navigation + notification | 单一 runtime；系统返回/多窗口/通知能回到同一会话 |
| preload/IPC | typed bridge + JNI + capability validation | account/run/session correlation；不把 JSON 字符串变成任意调用入口 |
| 常驻后台进程 | bounded service/WorkManager/checkpoint 或明确远程执行 | 进程可能死亡；恢复和用户可见状态，而非宣称永远在线 |
| 桌面本地 shell/tool | app sandbox 的已审核可执行能力 | 不承诺宿主 OS 全权；不私自 root 或下载执行代码 |
| 桌面任意路径 | SAF/Photo Picker/ContentResolver + app storage | 用户授权范围、URI lifetime、撤销与空间错误；不假造文件绝对路径 |
| 系统密钥库 | Android Keystore 包装凭据存储 | 用户锁定/设备换机/密钥失效/退出清理；不明文 SharedPreferences |
| 浏览器 OAuth/passkey | Browser/Custom Tabs + App Links + Credential Manager | state/PKCE/nonce、回跳唯一消费、scope 与账户绑定 |
| 桌面 offline ASR | 真正 on-device adapter 或有许可的本地模型引擎 | 断网可用才称离线；不把 prefer-offline hint 当保证 |
| 插件 iframe/private port | 限定 origin 的 WebView message channel + load session | exact instance/nonce/grants、dispose/reject pending；不导出秘密 |
| 全桌面 Computer Use | 远程电脑控制或受系统允许的本机屏幕共享/交互 | 显式授权、可见目标、撤销、人工接管；手机不是远程电脑的隐式替身 |
| 自更新/DMG 签名 | Play AAB 与独立签名 APK 各自渠道 | provenance、签名 lineage、升级、数据迁移；禁止跨渠道静默换签名 |

所有替代都保留用户目的，不默认把原本本地功能外包到付费服务器。远程路径要公开网络、隐私、费用、在线设备依赖与离线失败；若没有可接受等价物，明确记为未闭合差异，不宣称“完美相同”。

原生 UI 对手机/平板重排信息而不是缩小桌面。浮窗、悬停、右键改为明确按钮/长按/上下文菜单且可被无障碍触达。桌面快捷键在外接键盘存在时支持；触屏始终有入口。动效遵循减少动画设置。

平台差异记录使用 [模板](../templates/platform-delta.md)，包括 OS/API 范围、真机证据、用户影响和 reviewer。`not-applicable` 不得只写“mobile 不需要”。
