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
  decision: "proceeded" | "cancelled" | "blocked";
  findings: ReviewFinding[];
  user_profile?: {
    strictness_level: number;
    agreement_rate: number;
    avg_helpfulness_score: number;
  };
}

/**
 * Manages integration with git-ai commit review results.
 * Watches .git/ai/last_review.json and displays findings in VS Code Problems panel.
 */
export class ReviewDiagnosticsManager {
  private diagnosticCollection: vscode.DiagnosticCollection;
  private fileWatcher: vscode.FileSystemWatcher | null = null;
  private statusBarItem: vscode.StatusBarItem;

  constructor(private context: vscode.ExtensionContext) {
    this.diagnosticCollection = vscode.languages.createDiagnosticCollection("git-ai-review");
    context.subscriptions.push(this.diagnosticCollection);

    this.statusBarItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
      100
    );
    this.statusBarItem.command = "workbench.action.problems.focus";
    context.subscriptions.push(this.statusBarItem);
  }

  activate() {
    // Check if review diagnostics are enabled
    const config = vscode.workspace.getConfiguration("gitai");
    if (!config.get("review.enableDiagnostics", true)) {
      console.log("[git-ai] Review diagnostics disabled in settings");
      return;
    }

    const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
    if (!workspaceFolder) {
      console.log("[git-ai] No workspace folder found, review diagnostics disabled");
      return;
    }

    const reviewFilePath = path.join(
      workspaceFolder.uri.fsPath,
      ".git",
      "ai",
      "last_review.json"
    );

    // Watch for review result file changes
    this.fileWatcher = vscode.workspace.createFileSystemWatcher(reviewFilePath);
    
    this.fileWatcher.onDidChange(() => this.loadAndDisplayReview(reviewFilePath));
    this.fileWatcher.onDidCreate(() => this.loadAndDisplayReview(reviewFilePath));
    this.fileWatcher.onDidDelete(() => this.clearReview());

    this.context.subscriptions.push(this.fileWatcher);

    // Load existing review on activation
    if (fs.existsSync(reviewFilePath)) {
      this.loadAndDisplayReview(reviewFilePath);
    }

    console.log("[git-ai] Review diagnostics manager activated");
  }

  private async loadAndDisplayReview(filePath: string) {
    try {
      const content = fs.readFileSync(filePath, "utf-8");
      const review: ReviewResult = JSON.parse(content);

      console.log(`[git-ai] Loaded review result: ${review.summary}`);

      this.displayReviewInProblems(review);
      this.updateStatusBar(review);
      this.showReviewNotification(review);
    } catch (error) {
      console.error("[git-ai] Failed to load review result:", error);
    }
  }

  private displayReviewInProblems(review: ReviewResult) {
    // Clear previous diagnostics
    this.diagnosticCollection.clear();

    if (review.findings.length === 0) {
      return;
    }

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
      // If no line number provided, mark the entire first line
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

    console.log(
      `[git-ai] Displayed ${review.findings.length} findings across ${diagnosticsByFile.size} files`
    );
  }

  private updateStatusBar(review: ReviewResult) {
    const icon =
      review.recommendation === "block"
        ? "$(error)"
        : review.recommendation === "review"
        ? "$(warning)"
        : "$(check)";

    const findingsCount = review.findings.length;
    const text = findingsCount > 0 
      ? `${icon} git-ai: ${findingsCount} 个问题`
      : `${icon} git-ai: 审核通过`;

    this.statusBarItem.text = text;
    this.statusBarItem.tooltip = review.summary;
    this.statusBarItem.show();
  }

  private showReviewNotification(review: ReviewResult) {
    // Only show notification for new reviews (within last 10 seconds)
    const reviewTime = new Date(review.timestamp);
    const now = new Date();
    const ageSeconds = (now.getTime() - reviewTime.getTime()) / 1000;
    
    if (ageSeconds > 10) {
      // This is an old review loaded on activation, don't spam
      return;
    }

    const icon =
      review.recommendation === "block"
        ? "❌"
        : review.recommendation === "review"
        ? "⚠️"
        : "✅";

    const message = `${icon} git-ai Review: ${review.summary}`;

    const showProblemsAction = "查看问题";

    if (review.recommendation === "block") {
      vscode.window.showErrorMessage(message, showProblemsAction).then((action) => {
        if (action === showProblemsAction) {
          vscode.commands.executeCommand("workbench.action.problems.focus");
        }
      });
    } else if (review.recommendation === "review" && review.findings.length > 0) {
      vscode.window.showWarningMessage(message, showProblemsAction).then((action) => {
        if (action === showProblemsAction) {
          vscode.commands.executeCommand("workbench.action.problems.focus");
        }
      });
    } else {
      // Only show success notification if enabled in settings
      const config = vscode.workspace.getConfiguration("gitai");
      if (config.get("review.showSuccessNotification", false)) {
        vscode.window.showInformationMessage(message);
      }
    }
  }

  private clearReview() {
    this.diagnosticCollection.clear();
    this.statusBarItem.hide();
    console.log("[git-ai] Cleared review diagnostics");
  }

  dispose() {
    this.diagnosticCollection.dispose();
    this.fileWatcher?.dispose();
    this.statusBarItem.dispose();
  }
}
