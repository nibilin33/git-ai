# 通过 Git Hooks 向编辑器注入上下文

## 概述

git-ai 可以通过 git hooks 将代码审核结果、AI 归因数据等上下文信息注入到代码编辑器中，实现以下功能：

- **实时显示审核问题**：在 Problems 面板显示 commit review 发现的问题
- **代码内联标记**：在代码中标注问题位置（Diagnostics）
- **桌面通知**：弹出审核摘要通知
- **个性化建议**：基于用户画像提供定制化提示

## 架构设计

### 数据流

```
┌─────────────────┐
│  git commit     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐      ┌──────────────────┐
│ pre-commit hook │─────▶│ Commit Review    │
│ (git-ai)        │      │ (DashScope API)  │
└─────────┬───────┘      └────────┬─────────┘
          │                       │
          │                       ▼
          │              ┌──────────────────┐
          │              │ Review Report    │
          │              │ (JSON)           │
          │              └────────┬─────────┘
          │                       │
          │                       ▼
          │              ┌──────────────────┐
          │              │ Write to file:   │
          │              │ .git/ai/         │
          │              │  last_review.json│
          │              └────────┬─────────┘
          │                       │
          ▼                       ▼
┌─────────────────────────────────────────┐
│  FileSystemWatcher (VS Code Extension)  │
└────────────────┬────────────────────────┘
                 │
                 ▼
        ┌────────────────┐
        │ Parse JSON     │
        └────────┬───────┘
                 │
        ┌────────┴────────┐
        │                 │
        ▼                 ▼
┌──────────────┐  ┌──────────────────┐
│ Diagnostics  │  │ Notification     │
│ (Problems)   │  │ (Toast Message)  │
└──────────────┘  └──────────────────┘
```

### 存储格式

**文件路径：** `.git/ai/last_review.json`

**JSON Schema：**
```json
{
  "timestamp": "2026-03-31T10:30:00Z",
  "commit_sha": null,  // pre-commit 时为 null
  "summary": "发现 2 个潜在问题",
  "recommendation": "review",
  "findings": [
    {
      "severity": "high",
      "file": "src/main.rs",
      "line": 42,  // 可选，需要从 diff 中解析
      "title": "未处理的错误",
      "details": "unwrap() 可能导致 panic，建议使用 ? 或 match"
    },
    {
      "severity": "medium",
      "file": "src/utils.rs",
      "line": 15,
      "title": "空指针风险",
      "details": "缺少 null 检查"
    }
  ],
  "user_profile": {
    "strictness_level": 3,
    "agreement_rate": 0.82,
    "avg_helpfulness_score": 4.2
  }
}
```

## 实现步骤

### 第一步：修改 commit_review.rs 写入文件

在 `src/commands/hooks/commit_review.rs` 中，审核完成后写入结果文件：

```rust
fn write_review_result_to_file(
    repo: &Repository,
    report: &CommitReviewReport,
    user_profile: &UserProfile,
) -> Result<(), GitAiError> {
    let review_file_path = repo.path().join("ai").join("last_review.json");
    
    // Ensure directory exists
    if let Some(parent) = review_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let review_data = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(),
        "commit_sha": null,
        "summary": report.summary,
        "recommendation": format!("{:?}", report.recommendation).to_lowercase(),
        "findings": report.findings.iter().map(|f| {
            serde_json::json!({
                "severity": format!("{:?}", f.severity).to_lowercase(),
                "file": f.file,
                "title": f.title,
                "details": f.details,
            })
        }).collect::<Vec<_>>(),
        "user_profile": {
            "strictness_level": user_profile.preferences.strictness_level,
            "agreement_rate": user_profile.agreement_rate(),
            "avg_helpfulness_score": user_profile.avg_helpfulness_score(),
        }
    });
    
    std::fs::write(&review_file_path, serde_json::to_string_pretty(&review_data)?)?;
    
    debug_log(&format!(
        "[CommitReview] Wrote review result to {}",
        review_file_path.display()
    ));
    
    Ok(())
}
```

在 `run_commit_review()` 中调用：
```rust
// 在 print_review() 之后
write_review_result_to_file(repo, &report, &user_profile)?;
```

### 第二步：扩展 VSCode 扩展

在 `agent-support/vscode/src/` 中添加新模块 `review-diagnostics.ts`：

```typescript
import * as vscode from "vscode";
import * as fs from "node:fs";
import * as path from "node:path";

interface ReviewFinding {
  severity: "high" | "medium" | "low";
  file: string;
  line?: number;
  title: string;
  details: string;
}

interface ReviewResult {
  timestamp: string;
  commit_sha: string | null;
  summary: string;
  recommendation: "proceed" | "review" | "block";
  findings: ReviewFinding[];
  user_profile?: {
    strictness_level: number;
    agreement_rate: number;
    avg_helpfulness_score: number;
  };
}

export class ReviewDiagnosticsManager {
  private diagnosticCollection: vscode.DiagnosticCollection;
  private fileWatcher: vscode.FileSystemWatcher | null = null;

  constructor(private context: vscode.ExtensionContext) {
    this.diagnosticCollection = vscode.languages.createDiagnosticCollection("git-ai-review");
    context.subscriptions.push(this.diagnosticCollection);
  }

  activate() {
    // Watch for review result file changes
    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) return;

    const reviewFilePath = path.join(
      workspaceFolder.uri.fsPath,
      ".git",
      "ai",
      "last_review.json"
    );

    this.fileWatcher = vscode.workspace.createFileSystemWatcher(reviewFilePath);
    
    this.fileWatcher.onDidChange(() => this.loadAndDisplayReview(reviewFilePath));
    this.fileWatcher.onDidCreate(() => this.loadAndDisplayReview(reviewFilePath));

    this.context.subscriptions.push(this.fileWatcher);

    // Load existing review on activation
    if (fs.existsSync(reviewFilePath)) {
      this.loadAndDisplayReview(reviewFilePath);
    }
  }

  private async loadAndDisplayReview(filePath: string) {
    try {
      const content = fs.readFileSync(filePath, "utf-8");
      const review: ReviewResult = JSON.parse(content);

      this.displayReviewInProblems(review);
      this.showReviewNotification(review);
    } catch (error) {
      console.error("[git-ai] Failed to load review result:", error);
    }
  }

  private displayReviewInProblems(review: ReviewResult) {
    // Clear previous diagnostics
    this.diagnosticCollection.clear();

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) return;

    const diagnosticsByFile = new Map<string, vscode.Diagnostic[]>();

    review.findings.forEach((finding) => {
      const filePath = path.join(workspaceFolder.uri.fsPath, finding.file);
      const fileUri = vscode.Uri.file(filePath);

      const severity =
        finding.severity === "high"
          ? vscode.DiagnosticSeverity.Error
          : finding.severity === "medium"
          ? vscode.DiagnosticSeverity.Warning
          : vscode.DiagnosticSeverity.Information;

      // Line number from finding (0-indexed in VSCode)
      const line = finding.line ? finding.line - 1 : 0;
      const range = new vscode.Range(line, 0, line, Number.MAX_SAFE_INTEGER);

      const diagnostic = new vscode.Diagnostic(
        range,
        `${finding.title}: ${finding.details}`,
        severity
      );
      diagnostic.source = "git-ai review";
      diagnostic.code = finding.title;

      if (!diagnosticsByFile.has(filePath)) {
        diagnosticsByFile.set(filePath, []);
      }
      diagnosticsByFile.get(filePath)!.push(diagnostic);
    });

    // Apply all diagnostics
    diagnosticsByFile.forEach((diagnostics, filePath) => {
      this.diagnosticCollection.set(vscode.Uri.file(filePath), diagnostics);
    });
  }

  private showReviewNotification(review: ReviewResult) {
    const icon =
      review.recommendation === "block"
        ? "❌"
        : review.recommendation === "review"
        ? "⚠️"
        : "✅";

    const message = `${icon} git-ai Review: ${review.summary}`;

    if (review.recommendation === "block") {
      vscode.window.showErrorMessage(message, "查看问题").then((action) => {
        if (action === "查看问题") {
          vscode.commands.executeCommand("workbench.action.problems.focus");
        }
      });
    } else if (review.recommendation === "review") {
      vscode.window.showWarningMessage(message, "查看问题").then((action) => {
        if (action === "查看问题") {
          vscode.commands.executeCommand("workbench.action.problems.focus");
        }
      });
    } else {
      vscode.window.showInformationMessage(message);
    }
  }

  dispose() {
    this.diagnosticCollection.dispose();
    this.fileWatcher?.dispose();
  }
}
```

在 `extension.ts` 中激活：

```typescript
import { ReviewDiagnosticsManager } from "./review-diagnostics";

export function activate(context: vscode.ExtensionContext) {
  // ... 现有代码 ...

  // Initialize review diagnostics
  const reviewDiagnosticsManager = new ReviewDiagnosticsManager(context);
  reviewDiagnosticsManager.activate();
  context.subscriptions.push({
    dispose: () => reviewDiagnosticsManager.dispose()
  });
}
```

### 第三步：增强功能 - 行号解析

由于 commit review 的 findings 目前只有文件名，需要从 staged diff 中解析行号：

```rust
fn parse_line_number_from_diff(
    staged_patch: &str,
    file: &str,
    issue_context: &str,  // 问题描述中的代码片段
) -> Option<u32> {
    // 解析 unified diff 格式，找到匹配的行号
    // TODO: 实现 diff 解析逻辑
    None
}
```

### 第四步：IntelliJ 插件集成

对于 IntelliJ IDEA，可以通过类似方式实现：

1. **FileWatcher**：监听 `.git/ai/last_review.json`
2. **ExternalAnnotator**：在编辑器中标记问题
3. **Notification**：显示审核摘要

```kotlin
class GitAiReviewExternalAnnotator : ExternalAnnotator<ReviewResult, ReviewResult>() {
    override fun collectInformation(file: PsiFile): ReviewResult? {
        val reviewFile = File(file.project.basePath, ".git/ai/last_review.json")
        if (!reviewFile.exists()) return null
        
        return Gson().fromJson(reviewFile.readText(), ReviewResult::class.java)
    }
    
    override fun doAnnotate(collectedInfo: ReviewResult?): ReviewResult? {
        return collectedInfo
    }
    
    override fun apply(file: PsiFile, annotationResult: ReviewResult?, holder: AnnotationHolder) {
        annotationResult?.findings?.forEach { finding ->
            if (finding.file == file.virtualFile.path) {
                // 在代码中标记问题
                val range = TextRange(0, 0)  // TODO: 精确定位
                holder.newAnnotation(
                    if (finding.severity == "high") HighlightSeverity.ERROR else HighlightSeverity.WARNING,
                    "${finding.title}: ${finding.details}"
                ).range(range).create()
            }
        }
    }
}
```

## 配置选项

在 `~/.git-ai/config.json` 中添加：

```json
{
  "commit_review": {
    "editor_integration": {
      "enabled": true,
      "write_result_file": true,
      "clear_on_commit_success": false,
      "include_line_numbers": true
    }
  }
}
```

## 高级功能

### 1. 清除审核结果

提交成功后自动清除问题标记：

```rust
// 在 commit_post_command_hook 中
fn clear_review_diagnostics(repo: &Repository) {
    let review_file = repo.path().join("ai").join("last_review.json");
    let _ = std::fs::remove_file(&review_file);
}
```

### 2. 历史审核结果

保存审核历史：

```rust
// .git/ai/review_history/2026-03-31_10-30-00.json
fn archive_review_result(repo: &Repository, report: &CommitReviewReport) {
    let history_dir = repo.path().join("ai").join("review_history");
    std::fs::create_dir_all(&history_dir);
    
    let filename = format!("{}.json", Utc::now().format("%Y-%m-%d_%H-%M-%S"));
    // ... 写入历史文件
}
```

VSCode 命令查看历史：
```typescript
vscode.commands.registerCommand("git-ai.showReviewHistory", async () => {
  // 读取 .git/ai/review_history/*.json
  // 在 QuickPick 中显示
});
```

### 3. 个性化提示

基于用户画像在编辑器中显示个性化建议：

```typescript
if (review.user_profile) {
  const { strictness_level, agreement_rate } = review.user_profile;
  
  if (agreement_rate < 0.5) {
    vscode.window.showWarningMessage(
      `git-ai: 您的认同率较低 (${(agreement_rate * 100).toFixed(0)}%)，` +
      `已自动降低审核严格度至 ${strictness_level}/5`
    );
  }
}
```

### 4. 代码操作集成

在 Problems 面板提供快速修复：

```typescript
const codeActionProvider = vscode.languages.registerCodeActionsProvider(
  { scheme: "file" },
  {
    provideCodeActions(document, range, context) {
      const diagnostics = context.diagnostics.filter(
        (diag) => diag.source === "git-ai review"
      );

      return diagnostics.map((diag) => {
        const action = new vscode.CodeAction(
          "标记为误报",
          vscode.CodeActionKind.QuickFix
        );
        action.command = {
          title: "标记为误报",
          command: "git-ai.markFalsePositive",
          arguments: [diag.code],
        };
        return action;
      });
    },
  }
);
```

## 测试

### 单元测试
```bash
# 测试文件写入
cargo test --test review_personalization test_write_review_file

# 测试 VSCode 扩展
cd agent-support/vscode
yarn test
```

### 集成测试
```bash
# 1. 触发审核
echo "test" >> test.txt
git add test.txt
git commit -m "test"  # 会触发 pre-commit hook

# 2. 检查文件生成
cat .git/ai/last_review.json

# 3. VSCode 应该自动显示问题（如果扩展已安装）
```

## 性能考虑

- **异步写入**：审核结果写入文件是同步的，但文件很小（通常 < 10KB），影响可忽略
- **FileWatcher 开销**：只监听单个文件，性能影响极小
- **清理策略**：commit 成功后可选择清除文件，避免磁盘累积

## 未来增强

1. **LSP Server**：创建 git-ai LSP，提供更丰富的编辑器集成（代码补全、hover 提示等）
2. **WebSocket 实时通信**：实时推送审核进度和结果
3. **跨编辑器同步**：多个编辑器打开同一仓库时同步审核状态
4. **AI 归因可视化**：在编辑器中高亮显示 AI 生成的代码行（类似 git blame）

## 总结

通过文件系统作为桥梁，git-ai 可以将审核结果和其他上下文信息无缝注入到各种代码编辑器中，实现：

✅ **实时反馈**：pre-commit 时立即看到问题  
✅ **无侵入式**：不影响 git 工作流  
✅ **跨编辑器兼容**：VSCode、Cursor、IntelliJ 均可支持  
✅ **个性化体验**：基于用户画像提供定制建议  

这种架构简单、可靠、易于扩展，是连接 git hooks 和编辑器生态的最佳实践。
