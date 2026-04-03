use crate::api::ApiContext;
use crate::config::Config;
use crate::error::GitAiError;
use crate::git::repository::Repository;
use crate::utils::is_interactive_terminal;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::{self, Write};

use super::review_personalization::{ProfileStore, UserProfile};

const REVIEW_SYSTEM_PROMPT: &str = "你是资深代码审核工程师。请仅基于给定的 staged diff 做严格审核，优先识别会影响正确性、稳定性、兼容性、安全性和测试覆盖的真实问题。只返回 JSON，不要输出 Markdown 代码块，不要附加解释。";
const DEFAULT_DASHSCOPE_GENERATION_URL: &str = "https://dashscope.aliyuncs.com/api/v1/services/aigc/text-generation/generation";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ReviewRecommendation {
    Proceed,
    Review,
    Block,
}

impl ReviewRecommendation {
    fn from_str(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "proceed" | "approve" | "approved" => Self::Proceed,
            "block" | "reject" | "rejected" => Self::Block,
            _ => Self::Review,
        }
    }

    fn label(&self) -> &'static str {
        match self {
            Self::Proceed => "可继续",
            Self::Review => "建议人工确认",
            Self::Block => "建议阻断",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct ReviewFinding {
    severity: String,
    file: String,
    title: String,
    details: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CommitReviewReport {
    model: String,
    summary: String,
    recommendation: ReviewRecommendation,
    findings: Vec<ReviewFinding>,
    raw_response: String,
}

#[derive(Debug, Serialize)]
struct DashScopeGenerationRequest {
    model: String,
    input: DashScopeInput,
    parameters: DashScopeParameters,
}

#[derive(Debug, Serialize)]
struct QwenMessage {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct DashScopeGenerationResponse {
    output: Option<DashScopeOutput>,
    code: Option<String>,
    message: Option<String>,
    request_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct DashScopeInput {
    messages: Vec<QwenMessage>,
}

#[derive(Debug, Serialize)]
struct DashScopeParameters {
    temperature: f32,
    result_format: String,
}

#[derive(Debug, Deserialize)]
struct DashScopeOutput {
    text: Option<String>,
    choices: Option<Vec<DashScopeChoice>>,
}

#[derive(Debug, Deserialize)]
struct DashScopeChoice {
    message: Option<DashScopeMessage>,
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DashScopeMessage {
    content: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ModelReviewPayload {
    summary: Option<String>,
    recommendation: Option<String>,
    findings: Option<Vec<ModelReviewFinding>>,
}

#[derive(Debug, Deserialize)]
struct ModelReviewFinding {
    severity: Option<String>,
    file: Option<String>,
    title: Option<String>,
    details: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
enum CommitReviewDecision {
    Proceeded,
    CancelledByUser,
    BlockedNonInteractive,
}

/// User feedback on review quality
#[derive(Debug, Clone, Serialize)]
struct ReviewFeedback {
    /// Overall helpfulness rating (1-5, 5 = very helpful)
    #[serde(skip_serializing_if = "Option::is_none")]
    helpfulness_score: Option<u8>,
    
    /// Whether user agrees with AI recommendation
    #[serde(skip_serializing_if = "Option::is_none")]
    agrees_with_recommendation: Option<bool>,
    
    /// Indices of false positive findings (user thinks these are not real issues)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    false_positive_indices: Vec<usize>,
    
    /// Free-form comment
    #[serde(skip_serializing_if = "Option::is_none")]
    comment: Option<String>,
}

#[derive(Debug, Serialize)]
struct CommitReviewUploadPayload {
    created_at: String,
    repository_path: String,
    head: Option<String>,
    remotes: Vec<(String, String)>,
    staged_files: Vec<String>,
    diff_truncated: bool,
    decision: CommitReviewDecision,
    review: CommitReviewReport,
    
    /// User feedback (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    feedback: Option<ReviewFeedback>,
}

pub fn run_commit_review(repo: &Repository) -> Result<(), GitAiError> {
    let config = Config::get();
    
    // Check if commit review is enabled
    if !config.commit_review_enabled() {
        crate::utils::debug_log("[CommitReview] Commit review is disabled in config");
        return Ok(());
    }

    // Check if repository is allowed (respects allow_repositories config)
    if !config.is_allowed_repository(&Some(repo.clone())) {
        crate::utils::debug_log("[CommitReview] Repository not in allowed list, skipping commit review");
        return Ok(());
    }

    let mut staged_files: Vec<String> = repo.get_staged_filenames()?.into_iter().collect();
    staged_files.sort();
    if staged_files.is_empty() {
        crate::utils::debug_log("[CommitReview] No staged files, skipping review");
        return Ok(());
    }

    let staged_patch = repo.staged_diff_patch()?;
    if staged_patch.trim().is_empty() {
        crate::utils::debug_log("[CommitReview] Staged patch is empty, skipping review");
        return Ok(());
    }

    crate::utils::debug_log(&format!(
        "[CommitReview] Starting review for {} files, patch size: {} bytes",
        staged_files.len(),
        staged_patch.len()
    ));

    // Load user profile for personalization
    let user_identity = repo.git_author_identity().formatted().unwrap_or_else(|| "unknown".to_string());
    let profile_store = ProfileStore::new();
    let mut user_profile = profile_store.load_profile(&user_identity)
        .unwrap_or_else(|| UserProfile::new(user_identity.clone()));
    
    crate::utils::debug_log(&format!(
        "[CommitReview] User profile loaded: strictness={}, avg_score={:.1}, agreement={:.1}%",
        user_profile.preferences.strictness_level,
        user_profile.avg_helpfulness_score(),
        user_profile.agreement_rate() * 100.0
    ));

    let (trimmed_patch, diff_truncated) = truncate_utf8(&staged_patch, config.commit_review_max_patch_bytes());
    let report = request_qwen_review(repo, &staged_files, &trimmed_patch, &user_profile)?;

    print_review(&report, &staged_files, diff_truncated, config.commit_review_max_patch_bytes());

    let decision = if is_interactive_terminal() {
        if prompt_continue_commit()? {
            CommitReviewDecision::Proceeded
        } else {
            CommitReviewDecision::CancelledByUser
        }
    } else {
        eprintln!("git-ai: 已启用提交审核，但当前终端不可交互，已阻止本次提交。");
        CommitReviewDecision::BlockedNonInteractive
    };

    crate::utils::debug_log(&format!("[CommitReview] Review decision: {:?}", decision));

    // Collect feedback in interactive mode
    let feedback = if is_interactive_terminal() {
        collect_review_feedback(&report, decision)
    } else {
        None
    };
    
    // Update user profile based on feedback
    if let Some(ref fb) = feedback {
        let ai_rec = match report.recommendation {
            ReviewRecommendation::Proceed => "proceed",
            ReviewRecommendation::Review => "review",
            ReviewRecommendation::Block => "block",
        };
        let user_dec = match decision {
            CommitReviewDecision::Proceeded => "proceeded",
            CommitReviewDecision::CancelledByUser => "cancelled",
            CommitReviewDecision::BlockedNonInteractive => "blocked",
        };
        
        let false_positive_titles: Vec<String> = fb.false_positive_indices.iter()
            .filter_map(|&idx| report.findings.get(idx).map(|f| f.title.clone()))
            .collect();
        
        user_profile.update_from_feedback(
            ai_rec,
            user_dec,
            fb.helpfulness_score,
            fb.agrees_with_recommendation,
            false_positive_titles,
        );
        
        // Save updated profile (best effort)
        let _ = profile_store.save_profile(&user_profile);
    }

    // Write review result to file for editor integration
    let _ = write_review_result_to_file(repo, &report, &user_profile, decision);
    
    upload_review_result(repo, &staged_files, &report, decision, diff_truncated, feedback)?;

    if matches!(decision, CommitReviewDecision::Proceeded) {
        return Ok(());
    }

    Err(GitAiError::Generic(
        "提交已在代码审核阶段取消，请处理问题后重新提交。".to_string(),
    ))
}

fn request_qwen_review(
    repo: &Repository,
    staged_files: &[String],
    staged_patch: &str,
    user_profile: &UserProfile,
) -> Result<CommitReviewReport, GitAiError> {
    let config = Config::get();
    let qwen_url = config
        .commit_review_qwen_url()
        .unwrap_or(DEFAULT_DASHSCOPE_GENERATION_URL);

    let prompt = build_review_prompt(repo, staged_files, staged_patch);
    let system_prompt = format!(
        "{}{}",
        REVIEW_SYSTEM_PROMPT,
        user_profile.generate_prompt_modifier()
    );

    let body = DashScopeGenerationRequest {
        model: config.commit_review_qwen_model().to_string(),
        input: DashScopeInput {
            messages: vec![
                QwenMessage {
                    role: "system".to_string(),
                    content: system_prompt,
                },
                QwenMessage {
                    role: "user".to_string(),
                    content: prompt,
                },
            ],
        },
        parameters: DashScopeParameters {
            temperature: 0.1,
            result_format: "message".to_string(),
        },
    };

    let body_json = serde_json::to_string(&body)?;
    let mut request = ApiContext::http_post(qwen_url)
        .with_header("Content-Type", "application/json")
        .with_body(body_json)
        .with_timeout(config.commit_review_timeout_secs());

    if let Some(api_key) = config.commit_review_qwen_api_key() {
        request = request.with_header("Authorization", format!("Bearer {}", api_key));
    }

    let response = request
        .send()
        .map_err(|e| GitAiError::Generic(format!("DashScope 审核请求失败: {}", e)))?;
    if !(200..300).contains(&response.status_code) {
        return Err(GitAiError::Generic(format!(
            "DashScope 审核请求返回异常状态 {}: {}",
            response.status_code,
            response.as_str().unwrap_or("unknown error")
        )));
    }

    let response_body = response.as_str().unwrap_or("");
    let parsed: DashScopeGenerationResponse = serde_json::from_str(response_body).map_err(|e| {
        GitAiError::Generic(format!("无法解析 DashScope 审核响应: {}", e))
    })?;

    if let Some(code) = parsed.code.as_deref() {
        return Err(GitAiError::Generic(format!(
            "DashScope 审核请求失败 {}: {}",
            code,
            parsed.message.as_deref().unwrap_or("unknown error")
        )));
    }

    let raw_content = extract_dashscope_content(&parsed)
        .ok_or_else(|| {
            let request_id = parsed.request_id.as_deref().unwrap_or("unknown");
            GitAiError::Generic(format!("DashScope 审核响应为空，request_id={}", request_id))
        })?;

    Ok(parse_review_report(
        config.commit_review_qwen_model(),
        &raw_content,
    ))
}

fn extract_dashscope_content(response: &DashScopeGenerationResponse) -> Option<String> {
    let output = response.output.as_ref()?;

    if let Some(choices) = output.choices.as_ref()
        && let Some(content) = choices
            .first()
            .and_then(|choice| choice.message.as_ref())
            .and_then(|message| message.content.as_ref())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    {
        return Some(content);
    }

    if let Some(text) = output
        .text
        .as_ref()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(text);
    }

    output
        .choices
        .as_ref()
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.text.as_ref())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn build_review_prompt(repo: &Repository, staged_files: &[String], staged_patch: &str) -> String {
    let head = repo
        .head()
        .ok()
        .and_then(|head| head.target().ok())
        .map(|oid| oid.to_string())
        .unwrap_or_else(|| "HEAD".to_string());
    let repo_path = repo
        .workdir()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| repo.path().display().to_string());

    format!(
        "请审查以下即将提交的 staged 代码变更。\n\n输出要求：\n1. 只输出 JSON。\n2. JSON 格式必须为 {{\"summary\": string, \"recommendation\": \"proceed|review|block\", \"findings\": [{{\"severity\": \"high|medium|low\", \"file\": string, \"title\": string, \"details\": string}}]}}。\n3. 仅保留真实且可执行的问题；如果没有问题，findings 返回空数组。\n\n仓库路径：{}\n基线提交：{}\n变更文件：{}\n\nstaged diff:\n{}",
        repo_path,
        head,
        staged_files.join(", "),
        staged_patch
    )
}

fn parse_review_report(model: &str, raw_content: &str) -> CommitReviewReport {
    if let Some(payload) = parse_model_review_payload(raw_content) {
        let findings = payload
            .findings
            .unwrap_or_default()
            .into_iter()
            .filter_map(|finding| {
                let title = finding.title.unwrap_or_default();
                let details = finding.details.unwrap_or_default();
                if title.trim().is_empty() && details.trim().is_empty() {
                    return None;
                }

                Some(ReviewFinding {
                    severity: finding
                        .severity
                        .unwrap_or_else(|| "medium".to_string())
                        .trim()
                        .to_string(),
                    file: finding.file.unwrap_or_default().trim().to_string(),
                    title: title.trim().to_string(),
                    details: details.trim().to_string(),
                })
            })
            .collect();

        return CommitReviewReport {
            model: model.to_string(),
            summary: payload
                .summary
                .unwrap_or_else(|| "Qwen 未给出摘要。".to_string())
                .trim()
                .to_string(),
            recommendation: payload
                .recommendation
                .as_deref()
                .map(ReviewRecommendation::from_str)
                .unwrap_or(ReviewRecommendation::Review),
            findings,
            raw_response: raw_content.to_string(),
        };
    }

    CommitReviewReport {
        model: model.to_string(),
        summary: raw_content.trim().to_string(),
        recommendation: ReviewRecommendation::Review,
        findings: Vec::new(),
        raw_response: raw_content.to_string(),
    }
}

fn parse_model_review_payload(raw_content: &str) -> Option<ModelReviewPayload> {
    let trimmed = raw_content.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Ok(payload) = serde_json::from_str::<ModelReviewPayload>(trimmed) {
        return Some(payload);
    }

    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if start >= end {
        return None;
    }

    serde_json::from_str::<ModelReviewPayload>(&trimmed[start..=end]).ok()
}

fn print_review(
    report: &CommitReviewReport,
    staged_files: &[String],
    diff_truncated: bool,
    max_patch_bytes: usize,
) {
    eprintln!("\n=== git-ai qwen 提交审核 ===");
    eprintln!("模型: {}", report.model);
    eprintln!("结论: {}", report.recommendation.label());
    eprintln!("文件: {}", staged_files.join(", "));
    if diff_truncated {
        eprintln!("提示: staged diff 已截断到 {} bytes 后送审。", max_patch_bytes);
    }
    eprintln!("摘要: {}", report.summary);

    if report.findings.is_empty() {
        eprintln!("发现: 未识别出明确问题。\n");
        return;
    }

    eprintln!("发现:");
    for (index, finding) in report.findings.iter().enumerate() {
        let file_display = if finding.file.trim().is_empty() {
            "未定位文件"
        } else {
            finding.file.as_str()
        };
        eprintln!(
            "{}. [{}] {} ({})",
            index + 1,
            finding.severity,
            finding.title,
            file_display
        );
        eprintln!("   {}", finding.details);
    }
    eprintln!();
}

fn prompt_continue_commit() -> Result<bool, GitAiError> {
    print!("继续提交? [y/N]: ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let normalized = input.trim().to_ascii_lowercase();
    Ok(matches!(normalized.as_str(), "y" | "yes"))
}

/// Collect optional user feedback on review quality
fn collect_review_feedback(
    report: &CommitReviewReport,
    decision: CommitReviewDecision,
) -> Option<ReviewFeedback> {
    // Quick feedback prompt - user can skip by pressing Enter
    print!("\n[可选] 审核结果反馈 (直接回车跳过)");
    print!("\n评分 (1-5, 5=非常有帮助): ");
    io::stdout().flush().ok()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input).ok()?;
    let score_str = input.trim();
    
    // If user just pressed Enter, skip feedback
    if score_str.is_empty() {
        return None;
    }

    let helpfulness_score = score_str.parse::<u8>().ok().filter(|&s| s >= 1 && s <= 5);
    
    // Ask about agreement with AI recommendation
    let ai_recommendation_matches_decision = match (&report.recommendation, decision) {
        (ReviewRecommendation::Proceed, CommitReviewDecision::Proceeded) => true,
        (ReviewRecommendation::Block, CommitReviewDecision::CancelledByUser) => true,
        (ReviewRecommendation::Review, CommitReviewDecision::Proceeded) 
        | (ReviewRecommendation::Review, CommitReviewDecision::CancelledByUser) => true,
        _ => false,
    };
    
    let agrees_with_recommendation = if !ai_recommendation_matches_decision {
        print!("是否认同 AI 的建议? [y/N]: ");
        io::stdout().flush().ok()?;
        let mut agree_input = String::new();
        io::stdin().read_line(&mut agree_input).ok()?;
        let normalized = agree_input.trim().to_ascii_lowercase();
        Some(matches!(normalized.as_str(), "y" | "yes"))
    } else {
        Some(true)
    };

    // Collect false positives if there are findings and user disagreed
    let mut false_positive_indices = Vec::new();
    if !report.findings.is_empty() 
        && agrees_with_recommendation == Some(false) 
    {
        print!("哪些问题是误报? (输入序号，用逗号分隔，如 1,3): ");
        io::stdout().flush().ok()?;
        let mut fp_input = String::new();
        io::stdin().read_line(&mut fp_input).ok()?;
        
        false_positive_indices = fp_input
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .filter(|&i| i > 0 && i <= report.findings.len())
            .map(|i| i - 1)  // Convert to 0-indexed
            .collect();
    }

    // Optional comment
    print!("补充说明 (可选): ");
    io::stdout().flush().ok()?;
    let mut comment_input = String::new();
    io::stdin().read_line(&mut comment_input).ok()?;
    let comment = if comment_input.trim().is_empty() {
        None
    } else {
        Some(comment_input.trim().to_string())
    };

    Some(ReviewFeedback {
        helpfulness_score,
        agrees_with_recommendation,
        false_positive_indices,
        comment,
    })
}

/// Write review result to .git/ai/last_review.json for editor integration
fn write_review_result_to_file(
    repo: &Repository,
    report: &CommitReviewReport,
    user_profile: &UserProfile,
    decision: CommitReviewDecision,
) -> Result<(), GitAiError> {
    let review_file_path = repo.path().join("ai").join("last_review.json");
    
    // Ensure directory exists
    if let Some(parent) = review_file_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    
    let review_data = serde_json::json!({
        "timestamp": Utc::now().to_rfc3339(),
        "commit_sha": null,  // pre-commit stage, no SHA yet
        "summary": report.summary,
        "recommendation": match report.recommendation {
            ReviewRecommendation::Proceed => "proceed",
            ReviewRecommendation::Review => "review",
            ReviewRecommendation::Block => "block",
        },
        "decision": match decision {
            CommitReviewDecision::Proceeded => "proceeded",
            CommitReviewDecision::CancelledByUser => "cancelled",
            CommitReviewDecision::BlockedNonInteractive => "blocked",
        },
        "findings": report.findings.iter().map(|f| {
            serde_json::json!({
                "severity": f.severity,
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
    
    crate::utils::debug_log(&format!(
        "[CommitReview] Wrote review result to {} for editor integration",
        review_file_path.display()
    ));
    
    Ok(())
}

fn upload_review_result(
    repo: &Repository,
    staged_files: &[String],
    report: &CommitReviewReport,
    decision: CommitReviewDecision,
    diff_truncated: bool,
    feedback: Option<ReviewFeedback>,
) -> Result<(), GitAiError> {
    let config = Config::get();
    let Some(upload_url) = config.commit_review_upload_url() else {
        crate::utils::debug_log("[CommitReview] No upload URL configured, skipping upload");
        return Ok(());
    };

    crate::utils::debug_log(&format!(
        "[CommitReview] Uploading review result to: {}",
        upload_url
    ));

    let api_context = ApiContext::new(None).with_timeout(config.commit_review_timeout_secs());
    let payload = CommitReviewUploadPayload {
        created_at: Utc::now().to_rfc3339(),
        repository_path: repo
            .workdir()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|_| repo.path().display().to_string()),
        head: repo
            .head()
            .ok()
            .and_then(|head| head.target().ok())
            .map(|oid| oid.to_string()),
        remotes: repo.remotes_with_urls().unwrap_or_default(),
        staged_files: staged_files.to_vec(),
        diff_truncated,
        decision,
        review: report.clone(),
        feedback,
    };

    let body_json = serde_json::to_string(&payload)?;
    crate::utils::debug_log(&format!(
        "[CommitReview] Upload payload size: {} bytes, remotes: {:?}",
        body_json.len(),
        payload.remotes
    ));

    let mut request = ApiContext::http_post(upload_url)
        .with_header("Content-Type", "application/json")
        .with_body(body_json)
        .with_timeout(config.commit_review_timeout_secs());

    if let Some(api_key) = api_context.api_key.as_deref() {
        request = request.with_header("X-API-Key", api_key);
        if let Some(identity) = api_context.author_identity.as_deref() {
            request = request.with_header("X-Author-Identity", identity);
        }
    }
    if let Some(token) = api_context.auth_token.as_deref() {
        request = request.with_header("Authorization", format!("Bearer {}", token));
    }

    let response = request.send().map_err(|e| {
        GitAiError::Generic(format!("提交审核结果上传失败: {}", e))
    })?;
    if !(200..300).contains(&response.status_code) {
        return Err(GitAiError::Generic(format!(
            "提交审核结果上传失败，服务返回 {}: {}",
            response.status_code,
            response.as_str().unwrap_or("unknown error")
        )));
    }

    crate::utils::debug_log(&format!(
        "[CommitReview] Upload successful, status: {}",
        response.status_code
    ));


    Ok(())
}

fn truncate_utf8(input: &str, max_bytes: usize) -> (String, bool) {
    if input.len() <= max_bytes {
        return (input.to_string(), false);
    }

    let mut end = max_bytes;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    (input[..end].to_string(), true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_review_report_from_json() {
        let raw = r#"{
            "summary": "发现一个高风险问题",
            "recommendation": "block",
            \"findings\": [
                {
                    "severity": "high",
                    "file": "src/main.rs",
                    "title": "空指针风险",
                    "details": "这里在无返回值时会 panic"
                }
            ]
        }"#;
        let report = parse_review_report("qwen-plus", raw);
        assert_eq!(report.recommendation, ReviewRecommendation::Block);
        assert_eq!(report.findings.len(), 1);
        assert_eq!(report.findings[0].file, "src/main.rs");
    }

    #[test]
    fn test_parse_review_report_from_fenced_json() {
        let raw = "```json\n{\"summary\":\"ok\",\"recommendation\":\"proceed\",\"findings\":[]}\n```";
        let report = parse_review_report("qwen-plus", raw);
        assert_eq!(report.recommendation, ReviewRecommendation::Proceed);
        assert!(report.findings.is_empty());
    }

    #[test]
    fn test_truncate_utf8_preserves_boundaries() {
        let value = "你好abc";
        let (truncated, did_truncate) = truncate_utf8(value, 5);
        assert!(did_truncate);
        assert_eq!(truncated, "你");
    }
}