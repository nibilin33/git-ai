# git-ai 编辑器集成快速开始

通过 git hooks 自动将代码审核结果注入到 VSCode/Cursor 编辑器中，无缝集成到开发工作流。

## 功能特性

✨ **实时反馈**：提交时自动审核，问题直接显示在 Problems 面板  
🎯 **精准定位**：在代码中高亮问题位置（未来支持行号）  
🔔 **桌面通知**：高优先级问题立即弹窗提醒  
📊 **个性化**：基于用户画像调整审核标准  

## 工作原理

```
你提交代码
    ↓
git commit 触发 pre-commit hook
    ↓
git-ai 调用 DashScope API 审核
    ↓
写入结果到 .git/ai/last_review.json
    ↓
VSCode 扩展监听文件变化
    ↓
在 Problems 面板显示问题 + 状态栏 + 通知
```

## 快速设置

### 1. 启用 Commit Review

编辑 `~/.git-ai/config.json`：

```json
{
  "commit_review": {
    "enabled": true,
    "qwen_api_key": "sk-your-dashscope-api-key",
    "timeout_secs": 30
  }
}
```

获取 API Key：https://dashscope.console.aliyun.com/apiKey

### 2. 安装/更新 VSCode 扩展

```bash
# 方法1: 从 marketplace 安装（推荐）
# 在 VSCode 中搜索 "git-ai"

# 方法2: 手动安装
cd agent-support/vscode
yarn install
yarn compile
vsce package
code --install-extension git-ai-vscode-*.vsix
```

重启 VSCode 后，扩展会自动激活审核集成功能。

### 3. 配置扩展设置（可选）

在 VSCode Settings 中搜索 "git-ai review"：

```json
{
  // 在 Problems 面板显示审核问题（默认启用）
  "gitai.review.enableDiagnostics": true,
  
  // 审核通过时显示成功通知（默认不显示）
  "gitai.review.showSuccessNotification": false
}
```

## 使用示例

### 场景1: 代码有问题 - 阻止提交

```bash
# 编辑文件，引入潜在问题
$ cat > test.rs <<'EOF'
fn main() {
    let x = Some(42);
    let value = x.unwrap();  // 审核会标记此处
    println!("{}", value);
}
EOF

$ git add test.rs
$ git commit -m "feat: add feature"

# 🔴 审核失败，提交被阻止
# VSCode 显示:
#   Problems (1)
#     ❌ test.rs [HIGH] 未处理的错误
#        unwrap() 可能导致 panic，建议使用 ? 或 match
#
#   状态栏: ⚠️ git-ai: 发现 1 个问题
#   通知: ❌ git-ai Review: 发现潜在问题，建议修复后重新提交
```

**Problems 面板截图（示意）：**
```
 PROBLEMS  TERMINAL  DEBUG CONSOLE

git-ai review (1)                                           清除 ✖
  ↓ test.rs                                           
    ❌ [8, 0] 未处理的错误: unwrap() 可能导致 panic，建议使用 ? 或 ...
```

点击问题会跳转到对应文件和行号。

### 场景2: 审核通过 - 静默提交

```bash
$ echo "// 添加注释" >> safe_code.rs
$ git add safe_code.rs
$ git commit -m "docs: add comment"

# ✅ 审核通过，提交成功
# VSCode 显示:
#   Problems (0)
#   状态栏: ✅ git-ai: 审核通过
#   （无弹窗，除非启用了 showSuccessNotification）
```

### 场景3: 个性化审核

系统会根据你的反馈历史自动调整审核严格度：

```bash
# 第一次使用：严格度 3/5（默认）
$ git commit -m "feat: xxx"
# 🟡 发现 5 个问题

# 你多次选择"不同意"并标记误报
# 系统自动降低严格度至 2/5

$ git commit -m "feat: yyy"
# 🟢 发现 1 个问题（过滤了常见误报）
```

**用户画像示例：**
```json
{
  "user_id": "developer@example.com",
  "preferences": {
    "strictness_level": 2,
    "suppressed_issue_patterns": [
      "未使用的变量",
      "代码重复"
    ]
  },
  "stats": {
    "total_reviews": 42,
    "agreement_rate": 0.65,
    "avg_helpfulness_score": 3.8
  }
}
```

## 高级功能

### 命令面板

在 VSCode 中按 `Cmd+Shift+P`（Mac）或 `Ctrl+Shift+P`（Windows/Linux）：

```
> Git AI: Toggle Show AI Code
# 切换显示 AI 生成的代码（已有功能）

> Git AI: Clear Review Diagnostics
# 清除审核问题标记（未来功能）

> Git AI: Show Review History
# 查看历史审核记录（未来功能）
```

### 手动清除审核结果

提交成功后，审核结果会保留在 `.git/ai/last_review.json`。如需手动清除：

```bash
# 清除审核结果文件
rm .git/ai/last_review.json

# VSCode 会自动清除 Problems 中的问题
```

也可以配置自动清除（在 `~/.git-ai/config.json`）：

```json
{
  "commit_review": {
    "editor_integration": {
      "clear_on_commit_success": true
    }
  }
}
```

### 查看原始审核数据

```bash
# 查看最近一次审核结果
cat .git/ai/last_review.json | jq

# 输出示例:
{
  "timestamp": "2026-03-31T10:30:00Z",
  "commit_sha": null,
  "summary": "发现 2 个潜在问题",
  "recommendation": "review",
  "decision": "cancelled",
  "findings": [
    {
      "severity": "high",
      "file": "src/main.rs",
      "title": "未处理的错误",
      "details": "unwrap() 可能导致 panic，建议使用 ? 或 match"
    }
  ],
  "user_profile": {
    "strictness_level": 3,
    "agreement_rate": 0.82,
    "avg_helpfulness_score": 4.2
  }
}
```

## 故障排查

### 问题1: 提交时没有触发审核

**检查项：**

```bash
# 1. 确认 git-ai 已安装
which git-ai

# 2. 检查 hooks 是否安装
ls .git/hooks/pre-commit
# 应显示 git-ai 的符号链接或脚本

# 3. 检查配置
cat ~/.git-ai/config.json | grep -A5 commit_review

# 4. 启用调试日志
GIT_AI_DEBUG=1 git commit -m "test"
```

### 问题2: VSCode 没有显示审核结果

**检查项：**

1. **扩展是否激活**：
   - 打开 Output 面板，选择 "git-ai" channel
   - 应该看到 `[git-ai] Review diagnostics manager activated`

2. **审核文件是否存在**：
   ```bash
   ls -la .git/ai/last_review.json
   ```

3. **设置是否启用**：
   - 打开 Settings，搜索 `gitai.review.enableDiagnostics`
   - 确保为 `true`

4. **强制重新加载**：
   ```bash
   touch .git/ai/last_review.json  # 触发 FileWatcher
   ```

### 问题3: 问题列表为空但审核失败了

原因：当前版本的 findings 中缺少行号信息。

**临时方案**：
- 查看终端输出的审核摘要
- 手动打开 `.git/ai/last_review.json` 查看详情

**未来改进**：
- 从 staged diff 中解析精确行号
- 支持多行范围标记

## API 参考

### ReviewResult 接口

```typescript
interface ReviewResult {
  timestamp: string;           // ISO 8601 格式
  commit_sha: string | null;   // pre-commit 时为 null
  summary: string;             // 审核摘要
  recommendation: "proceed" | "review" | "block";
  decision: "proceeded" | "cancelled" | "blocked";
  findings: ReviewFinding[];
  user_profile?: UserProfile;
}

interface ReviewFinding {
  severity: "high" | "medium" | "low";
  file: string;                // 相对于仓库根目录的路径
  line?: number;               // 行号（可选，未来支持）
  title: string;               // 问题标题
  details: string;             // 详细描述
}

interface UserProfile {
  strictness_level: number;    // 1-5，默认 3
  agreement_rate: number;      // 0.0-1.0
  avg_helpfulness_score: number; // 0.0-5.0
}
```

### VSCode 配置

```json
{
  "gitai.review.enableDiagnostics": boolean,
  "gitai.review.showSuccessNotification": boolean
}
```

### git-ai 配置

```json
{
  "commit_review": {
    "enabled": boolean,
    "qwen_api_key": string,
    "timeout_secs": number,
    "editor_integration": {
      "write_result_file": boolean,
      "clear_on_commit_success": boolean
    }
  }
}
```

## 性能影响

- **文件写入**：< 1ms（结果文件通常 < 10KB）
- **FileWatcher 开销**：可忽略（只监听单个文件）
- **审核延迟**：5-10s（取决于 DashScope API 响应速度）

总体对开发体验无明显影响。

## 路线图

🚀 **v0.2（已完成）**
- [x] 基础文件写入
- [x] VSCode 扩展集成
- [x] Problems 面板显示
- [x] 桌面通知

📋 **v0.3（计划中）**
- [ ] 精确行号定位
- [ ] 多行范围高亮
- [ ] Code Actions（快速修复）
- [ ] 历史审核查看

🔮 **未来**
- [ ] IntelliJ/WebStorm 插件
- [ ] LSP Server
- [ ] AI 归因可视化
- [ ] 实时 WebSocket 推送

## 反馈与贡献

- **问题反馈**：https://github.com/git-ai-project/git-ai/issues
- **功能建议**：欢迎在 Issues 中讨论
- **贡献代码**：查看 CONTRIBUTING.md

---

🎉 享受无缝的 AI 代码审核体验！
