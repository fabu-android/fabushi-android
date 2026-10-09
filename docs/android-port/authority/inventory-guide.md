# 完整源码范围清单的取得与使用

`.github/workflows/android-port-docs.yml` 在 GitHub Actions 运行 `scripts/android-port-docs.py`。本轮不在本地执行测试/构建/文档校验。

## 自动输出

`desktop-root-tree.json` 保存固定 root tree；`desktop-recursive-api.json` 保存原始递归响应；`desktop-complete-tree.json` 保存无路径白名单的完整主仓树；`initial-parity-ledger.jsonl` 为每个 blob/symlink/gitlink 生成一行，全部从 unreviewed 开始；`anchor-integrity.json` 校验指定入口的实际 blob 内容摘要；`source-manifest.json` 记录 commit/tree、完整数量、根目录分布、截断回退、特殊对象、Actions run/attempt 与 actual checkout。

如果 GitHub recursive 响应 truncated，脚本改用每个非递归子树遍历补齐；非递归仍被截断则失败。源码分支在开始与结束都要等于 pin，否则保留快照但 gate 失败。不得将取不到的路径当作不存在。

## 不得混淆的范围

完整主仓 tree 不是完整运行依赖审计。Gitlink 指向的子仓、Git LFS 对象、锁文件解析出的依赖和构建时动态加载内容需要额外审查。脚本明确将这些语义闭包设为 pending，不会自动认定 N/A。每个关键 source anchor 只证明内容已取得且 hash 一致，不表示每条行为已理解。

生成的 ledger 字段遵循 `templates/parity-ledger.schema.json`；人工职责分解、目标映射及真实测试证据需要主实现提交到持久 ledger。机器初始行的 null disposition 和空 responsibilities 是明确未审状态，不是漏项已关闭。

## 文档门

独立 documentation-contract job 校验 Markdown 本地链接、JSON 语法及十二个必需功能手册。当前存在 DOC-WRITE-001 时，受阻缺失手册应使该 gate 失败；不准通过删掉要求或忽略链接变绿。source-inventory 独立继续输出可用清单。

输出 artifact 是快照。后续变更必须重跑，并按 source-manifest 和 digest 确认来源。artifact 到期后需从固定 SHA 再生，不把仅剩的聊天摘要作为源范围证明。
