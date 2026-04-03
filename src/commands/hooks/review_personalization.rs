use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// User profile for personalized code review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    /// User identifier (email or git identity)
    pub user_id: String,
    
    /// Review preferences learned from feedback
    pub preferences: ReviewPreferences,
    
    /// Statistics used for adaptive behavior
    pub stats: ReviewStats,
    
    /// Last updated timestamp
    pub updated_at: i64,
}

/// User preferences for code review behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewPreferences {
    /// Strictness level: 1 (lenient) to 5 (strict)
    /// Learned from feedback - if user often proceeds despite AI warnings, decrease
    pub strictness_level: u8,
    
    /// Issue types to de-emphasize (based on false positive feedback)
    #[serde(default)]
    pub suppressed_issue_patterns: Vec<String>,
    
    /// Preferred severity threshold (ignore issues below this)
    #[serde(default)]
    pub min_severity: SeverityLevel,
    
    /// File patterns to skip review
    #[serde(default)]
    pub skip_file_patterns: Vec<String>,
    
    /// Whether to auto-proceed when no high severity issues found
    #[serde(default)]
    pub auto_proceed_on_low_risk: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeverityLevel {
    Low,
    Medium,
    High,
}

impl Default for SeverityLevel {
    fn default() -> Self {
        SeverityLevel::Low
    }
}

/// Statistics for learning user behavior
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewStats {
    /// Total reviews conducted
    pub total_reviews: u32,
    
    /// Reviews with feedback provided
    pub feedback_count: u32,
    
    /// Sum of helpfulness scores (for averaging)
    pub total_helpfulness_score: u32,
    
    /// Times user agreed with AI recommendation
    pub agreement_count: u32,
    
    /// Times user disagreed with AI recommendation
    pub disagreement_count: u32,
    
    /// Count by issue type that were marked as false positives
    #[serde(default)]
    pub false_positive_by_type: HashMap<String, u32>,
    
    /// Decision pattern: (AI_recommendation, user_decision) -> count
    #[serde(default)]
    pub decision_patterns: HashMap<String, u32>,
}

impl Default for ReviewPreferences {
    fn default() -> Self {
        Self {
            strictness_level: 3, // Medium strictness
            suppressed_issue_patterns: Vec::new(),
            min_severity: SeverityLevel::Low,
            skip_file_patterns: Vec::new(),
            auto_proceed_on_low_risk: false,
        }
    }
}

impl Default for ReviewStats {
    fn default() -> Self {
        Self {
            total_reviews: 0,
            feedback_count: 0,
            total_helpfulness_score: 0,
            agreement_count: 0,
            disagreement_count: 0,
            false_positive_by_type: HashMap::new(),
            decision_patterns: HashMap::new(),
        }
    }
}

impl UserProfile {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            preferences: ReviewPreferences::default(),
            stats: ReviewStats::default(),
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64,
        }
    }
    
    /// Update profile based on review feedback
    pub fn update_from_feedback(
        &mut self,
        ai_recommendation: &str,
        user_decision: &str,
        helpfulness_score: Option<u8>,
        agrees_with_recommendation: Option<bool>,
        false_positive_titles: Vec<String>,
    ) {
        self.stats.total_reviews += 1;
        
        if helpfulness_score.is_some() || agrees_with_recommendation.is_some() {
            self.stats.feedback_count += 1;
        }
        
        if let Some(score) = helpfulness_score {
            self.stats.total_helpfulness_score += score as u32;
        }
        
        // Track agreement
        if let Some(agrees) = agrees_with_recommendation {
            if agrees {
                self.stats.agreement_count += 1;
            } else {
                self.stats.disagreement_count += 1;
                
                // Adjust strictness if user frequently disagrees
                let disagreement_rate = self.stats.disagreement_count as f64 
                    / self.stats.feedback_count.max(1) as f64;
                if disagreement_rate > 0.6 && self.preferences.strictness_level > 1 {
                    self.preferences.strictness_level -= 1;
                    crate::utils::debug_log(&format!(
                        "[Personalization] Decreased strictness to {} due to high disagreement rate",
                        self.preferences.strictness_level
                    ));
                }
            }
        }
        
        // Track false positives
        for title in false_positive_titles {
            *self.stats.false_positive_by_type.entry(title.clone()).or_insert(0) += 1;
            
            // Suppress issue types with 3+ false positives
            let count = self.stats.false_positive_by_type.get(&title).unwrap();
            if *count >= 3 && !self.preferences.suppressed_issue_patterns.contains(&title) {
                self.preferences.suppressed_issue_patterns.push(title.clone());
                crate::utils::debug_log(&format!(
                    "[Personalization] Suppressed issue pattern: {}",
                    title
                ));
            }
        }
        
        // Track decision patterns
        let pattern_key = format!("{}→{}", ai_recommendation, user_decision);
        *self.stats.decision_patterns.entry(pattern_key).or_insert(0) += 1;
        
        self.updated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
    }
    
    /// Get average helpfulness score
    pub fn avg_helpfulness_score(&self) -> f64 {
        if self.stats.feedback_count == 0 {
            0.0
        } else {
            self.stats.total_helpfulness_score as f64 / self.stats.feedback_count as f64
        }
    }
    
    /// Get agreement rate with AI recommendations
    pub fn agreement_rate(&self) -> f64 {
        let total = self.stats.agreement_count + self.stats.disagreement_count;
        if total == 0 {
            0.0
        } else {
            self.stats.agreement_count as f64 / total as f64
        }
    }
    
    /// Generate personalized system prompt modifier
    pub fn generate_prompt_modifier(&self) -> String {
        let mut modifiers: Vec<String> = Vec::new();
        
        match self.preferences.strictness_level {
            1 => modifiers.push("采用宽松标准，仅报告明确的高风险问题".to_string()),
            2 => modifiers.push("采用较宽松标准，关注确定性高的问题".to_string()),
            3 => {} // Default, no modifier
            4 => modifiers.push("采用严格标准，包含潜在风险点".to_string()),
            5 => modifiers.push("采用非常严格标准，报告所有可疑代码".to_string()),
            _ => {}
        }
        
        if !self.preferences.suppressed_issue_patterns.is_empty() {
            let patterns = self.preferences.suppressed_issue_patterns.join("、");
            let modifier_msg = format!("用户历史反馈显示以下问题类型误报率较高，请特别谨慎评估：{}", patterns);
            modifiers.push(modifier_msg);
        }
        
        if modifiers.is_empty() {
            String::new()
        } else {
            format!("\n\n个性化调整：{}", modifiers.join("；"))
        }
    }
}

/// Persistent storage for user profiles
pub struct ProfileStore {
    storage_path: PathBuf,
}

impl ProfileStore {
    pub fn new() -> Self {
        let mut storage_path = crate::mdm::utils::home_dir();
        storage_path.push(".git-ai");
        storage_path.push("review_profiles.json");
        
        Self { storage_path }
    }
    
    pub fn load_profile(&self, user_id: &str) -> Option<UserProfile> {
        let profiles = self.load_all_profiles().ok()?;
        profiles.get(user_id).cloned()
    }
    
    pub fn save_profile(&self, profile: &UserProfile) -> Result<(), std::io::Error> {
        let mut profiles = self.load_all_profiles().unwrap_or_default();
        profiles.insert(profile.user_id.clone(), profile.clone());
        
        // Ensure directory exists
        if let Some(parent) = self.storage_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        let json = serde_json::to_string_pretty(&profiles)?;
        std::fs::write(&self.storage_path, json)?;
        
        crate::utils::debug_log(&format!(
            "[Personalization] Saved profile for user: {}",
            profile.user_id
        ));
        
        Ok(())
    }
    
    fn load_all_profiles(&self) -> Result<HashMap<String, UserProfile>, std::io::Error> {
        if !self.storage_path.exists() {
            return Ok(HashMap::new());
        }
        
        let content = std::fs::read_to_string(&self.storage_path)?;
        let profiles: HashMap<String, UserProfile> = serde_json::from_str(&content)
            .unwrap_or_default();
        
        Ok(profiles)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_profile_creation() {
        let profile = UserProfile::new("test@example.com".to_string());
        assert_eq!(profile.preferences.strictness_level, 3);
        assert_eq!(profile.stats.total_reviews, 0);
    }
    
    #[test]
    fn test_feedback_updates_stats() {
        let mut profile = UserProfile::new("test@example.com".to_string());
        
        profile.update_from_feedback(
            "block",
            "proceeded",
            Some(4),
            Some(false),
            vec!["空值检查".to_string()],
        );
        
        assert_eq!(profile.stats.total_reviews, 1);
        assert_eq!(profile.stats.feedback_count, 1);
        assert_eq!(profile.stats.disagreement_count, 1);
        assert_eq!(profile.stats.false_positive_by_type.get("空值检查"), Some(&1));
    }
    
    #[test]
    fn test_strictness_adjustment() {
        let mut profile = UserProfile::new("test@example.com".to_string());
        profile.preferences.strictness_level = 3;
        
        // Simulate multiple disagreements
        for _ in 0..10 {
            profile.update_from_feedback(
                "block",
                "proceeded",
                None,
                Some(false),
                vec![],
            );
        }
        
        // Strictness should decrease due to high disagreement rate
        assert!(profile.preferences.strictness_level < 3);
    }
    
    #[test]
    fn test_issue_suppression() {
        let mut profile = UserProfile::new("test@example.com".to_string());
        
        // Mark same issue as false positive 3 times
        for _ in 0..3 {
            profile.update_from_feedback(
                "review",
                "proceeded",
                None,
                Some(false),
                vec!["未使用的变量".to_string()],
            );
        }
        
        // Issue should be suppressed
        assert!(profile.preferences.suppressed_issue_patterns.contains(&"未使用的变量".to_string()));
    }
    
    #[test]
    fn test_prompt_modifier_generation() {
        let mut profile = UserProfile::new("test@example.com".to_string());
        profile.preferences.strictness_level = 1;
        profile.preferences.suppressed_issue_patterns.push("空指针".to_string());
        
        let modifier = profile.generate_prompt_modifier();
        assert!(modifier.contains("宽松标准"));
        assert!(modifier.contains("空指针"));
    }
}
