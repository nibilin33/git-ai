Git hooks (commit、push、receive）时自动运行的脚本。
Claude Code 的 hooks：这是 Claude Code 自己的 hook 事件（hook_event_name == PreToolUse/PostToolUse）
新版vscode 有 VS Code Chat hooks
VS Code/Copilot 在“准备编辑文件/编辑完成”时调用这些 hooks
解析 Copilot/VS Code 扩展产生的 payload（会有 tool_name、transcript_path 指向 copilot session）
编辑器/AI 工具（Cursor、Claude、Codex 等）需要在“AI 把改动写入工作区之后”调用一次 git-ai checkpoint <tool>（并附带会话/模型/转录路径等 hook-input 元数据）
Codex（这里指 codex CLI，不是 VS Code 插件）怎么触发
触发器来自 Codex CLI 的配置：~/.codex/config.toml 的 notify。
会把 notify 配成运行 git-ai checkpoint codex --hook-input ...（Codex 会把 hook payload 作为参数传进来，而不是 stdin）


WorkingLogEntry：一个文件在该 checkpoint 的归属快照，见 src/authorship/working_log.rs:11

file：repo 相对路径
blob_sha：当时文件内容 hash
attributions: Vec<Attribution>：字符级归属
line_attributions: Vec<LineAttribution>：行级归属（用于 blame/统计/展示更快）

归属的基本单位：

Attribution { start, end, author_id, ts }（字符区间，支持重叠），见 src/authorship/attribution_tracker.rs:24
LineAttribution { start_line, end_line, author_id, overrode }（行区间，overrode 表示人改了 AI 行），见 src/authorship/attribution_tracker.rs:38