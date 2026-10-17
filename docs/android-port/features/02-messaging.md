# F02 Human/Bot 消息、联系人、群组与搜索

## 范围与 owner

源入口：Desktop messenger E2E、shipping messenger renderer、canonical native messaging 与 Host projection。Android frontend 管消息交互，Host messaging repository 管 canonical 数据，Coordinator 管事件与恢复；不能只移植 Agent 输入框而丢掉 Human 消息。

从固定源码逐项确认会话列表/分类/排序、联系人和群组、参与者权限、草稿、未读/已读、搜索、分页、发送/回复/转发/编辑/删除/复制/重试、提及、富文本与附件。反应、投票、话题、收藏、定时发送等额外动作仅在主线有实际责任时纳入确定清单；发现后不得遗漏，未读源码不能标完成。

## 数据与状态

消息状态：draft → queued → sending → accepted；delivered/read 仅在协议提供证据时显示。failed 与 outcome-unknown 分开。localId/serverId/clientMutationId 可追溯，重试保留原 mutation identity；ACK 丢失不再新建一条消息。乐观 UI 可显示 queued，但最终值来自 durable projection。

草稿绑定 account/conversation/revision；切会话不丢，迟到转写不覆盖新内容。未读使用 canonical cursor，不按页面打开次数直接清零。搜索绑定 query generation 与分页 cursor，旧结果不能覆盖新 query。历史列表用稳定键与滚动锚点，追加旧消息不跳到底部。错误、离线缓存、无结果和仍在加载必须区分。

## 原生体验与验证

手机采用列表到会话的导航，平板可双栏；长按动作同时有可访问菜单。富文本和附件预览不执行任意内容。验证 Desktop 发消息 → Android 读取并回复 → Desktop 同一会话同步；Bot 回复来自真实 runtime 而非占位文本。

反例包括重复/乱序消息、删除与迟到更新竞争、取消搜索、离线发送后重启、跨账户通知、群权限变化、长历史、大字体和 RTL。每个动作核对 durable 结果及服务端副作用；只有按钮、截图或 mock 通过不算功能闭合。
