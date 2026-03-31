#[macro_use]
mod repos;

use git_ai::commands::hooks::review_personalization::{
    ProfileStore, ReviewPreferences, ReviewStats, SeverityLevel, UserProfile,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a temporary profile store for testing
fn create_temp_profile_store() -> (TempDir, ProfileStore) {
    let temp_dir = TempDir::new().unwrap();
    let mut storage_path = temp_dir.path().to_path_buf();
    storage_path.push("review_profiles.json");
    
    let store = ProfileStore {
        storage_path,
    };
    
    (temp_dir, store)
}

#[test]
fn test_new_user_profile_defaults() {
    let profile = UserProfile::new("test@example.com".to_string());
    
    assert_eq!(profile.user_id, "test@example.com");
    assert_eq!(profile.preferences.strictness_level, 3);
    assert_eq!(profile.preferences.min_severity, SeverityLevel::Low);
    assert!(profile.preferences.suppressed_issue_patterns.is_empty());
    assert_eq!(profile.stats.total_reviews, 0);
    assert_eq!(profile.stats.feedback_count, 0);
}

#[test]
fn test_profile_save_and_load() {
    let (_temp_dir, store) = create_temp_profile_store();
    
    let mut profile = UserProfile::new("alice@example.com".to_string());
    profile.preferences.strictness_level = 2;
    profile.stats.total_reviews = 10;
    
    // Save profile
    store.save_profile(&profile).unwrap();
    
    // Load it back
    let loaded = store.load_profile("alice@example.com").unwrap();
    
    assert_eq!(loaded.user_id, "alice@example.com");
    assert_eq!(loaded.preferences.strictness_level, 2);
    assert_eq!(loaded.stats.total_reviews, 10);
}

#[test]
fn test_multiple_users_save_and_load() {
    let (_temp_dir, store) = create_temp_profile_store();
    
    let profile1 = UserProfile::new("alice@example.com".to_string());
    let mut profile2 = UserProfile::new("bob@example.com".to_string());
    profile2.preferences.strictness_level = 5;
    
    store.save_profile(&profile1).unwrap();
    store.save_profile(&profile2).unwrap();
    
    // Both profiles should exist
    let loaded1 = store.load_profile("alice@example.com").unwrap();
    let loaded2 = store.load_profile("bob@example.com").unwrap();
    
    assert_eq!(loaded1.preferences.strictness_level, 3);
    assert_eq!(loaded2.preferences.strictness_level, 5);
}

#[test]
fn test_profile_update_overwrites_existing() {
    let (_temp_dir, store) = create_temp_profile_store();
    
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 3;
    store.save_profile(&profile).unwrap();
    
    // Update and save again
    profile.preferences.strictness_level = 1;
    profile.stats.total_reviews = 5;
    store.save_profile(&profile).unwrap();
    
    let loaded = store.load_profile("test@example.com").unwrap();
    assert_eq!(loaded.preferences.strictness_level, 1);
    assert_eq!(loaded.stats.total_reviews, 5);
}

#[test]
fn test_feedback_increments_stats() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    profile.update_from_feedback(
        "block",
        "proceeded",
        Some(4),
        Some(false),
        vec![],
    );
    
    assert_eq!(profile.stats.total_reviews, 1);
    assert_eq!(profile.stats.feedback_count, 1);
    assert_eq!(profile.stats.total_helpfulness_score, 4);
    assert_eq!(profile.stats.disagreement_count, 1);
    assert_eq!(profile.stats.agreement_count, 0);
}

#[test]
fn test_agreement_tracking() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    // User agrees with AI
    profile.update_from_feedback(
        "proceed",
        "proceeded",
        Some(5),
        Some(true),
        vec![],
    );
    
    assert_eq!(profile.stats.agreement_count, 1);
    assert_eq!(profile.stats.disagreement_count, 0);
    assert_eq!(profile.agreement_rate(), 1.0);
    
    // User disagrees with AI
    profile.update_from_feedback(
        "block",
        "proceeded",
        Some(2),
        Some(false),
        vec![],
    );
    
    assert_eq!(profile.stats.agreement_count, 1);
    assert_eq!(profile.stats.disagreement_count, 1);
    assert_eq!(profile.agreement_rate(), 0.5);
}

#[test]
fn test_strictness_auto_decrease_on_high_disagreement() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 3;
    
    // Simulate 10 reviews where user disagrees 7 times (70% disagreement)
    for _ in 0..7 {
        profile.update_from_feedback(
            "block",
            "proceeded",
            None,
            Some(false),
            vec![],
        );
    }
    
    for _ in 0..3 {
        profile.update_from_feedback(
            "proceed",
            "proceeded",
            None,
            Some(true),
            vec![],
        );
    }
    
    // Strictness should decrease due to >60% disagreement rate
    assert!(profile.preferences.strictness_level < 3);
}

#[test]
fn test_strictness_does_not_decrease_below_one() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 1;
    
    // Even with 100% disagreement, shouldn't go below 1
    for _ in 0..10 {
        profile.update_from_feedback(
            "block",
            "proceeded",
            None,
            Some(false),
            vec![],
        );
    }
    
    assert_eq!(profile.preferences.strictness_level, 1);
}

#[test]
fn test_false_positive_tracking() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    let issue_title = "未使用的变量".to_string();
    
    // Mark as false positive once
    profile.update_from_feedback(
        "review",
        "proceeded",
        None,
        Some(false),
        vec![issue_title.clone()],
    );
    
    assert_eq!(profile.stats.false_positive_by_type.get(&issue_title), Some(&1));
    assert!(!profile.preferences.suppressed_issue_patterns.contains(&issue_title));
}

#[test]
fn test_issue_suppression_after_three_false_positives() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    let issue_title = "空值检查".to_string();
    
    // Mark as false positive 3 times
    for _ in 0..3 {
        profile.update_from_feedback(
            "review",
            "proceeded",
            None,
            Some(false),
            vec![issue_title.clone()],
        );
    }
    
    // Should be suppressed now
    assert_eq!(profile.stats.false_positive_by_type.get(&issue_title), Some(&3));
    assert!(profile.preferences.suppressed_issue_patterns.contains(&issue_title));
}

#[test]
fn test_multiple_false_positive_issues() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    let issue1 = "未使用的变量".to_string();
    let issue2 = "代码重复".to_string();
    
    // Mark both as false positives in one feedback
    profile.update_from_feedback(
        "review",
        "proceeded",
        None,
        Some(false),
        vec![issue1.clone(), issue2.clone()],
    );
    
    assert_eq!(profile.stats.false_positive_by_type.get(&issue1), Some(&1));
    assert_eq!(profile.stats.false_positive_by_type.get(&issue2), Some(&1));
}

#[test]
fn test_decision_pattern_tracking() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    profile.update_from_feedback("block", "proceeded", None, None, vec![]);
    profile.update_from_feedback("block", "proceeded", None, None, vec![]);
    profile.update_from_feedback("review", "cancelled", None, None, vec![]);
    
    assert_eq!(profile.stats.decision_patterns.get("block→proceeded"), Some(&2));
    assert_eq!(profile.stats.decision_patterns.get("review→cancelled"), Some(&1));
}

#[test]
fn test_average_helpfulness_score() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    
    assert_eq!(profile.avg_helpfulness_score(), 0.0);
    
    profile.update_from_feedback("proceed", "proceeded", Some(4), None, vec![]);
    profile.update_from_feedback("review", "proceeded", Some(5), None, vec![]);
    profile.update_from_feedback("block", "cancelled", Some(3), None, vec![]);
    
    // Average: (4 + 5 + 3) / 3 = 4.0
    assert_eq!(profile.avg_helpfulness_score(), 4.0);
}

#[test]
fn test_prompt_modifier_default_strictness() {
    let profile = UserProfile::new("test@example.com".to_string());
    
    // Default strictness (3) should have no modifier
    let modifier = profile.generate_prompt_modifier();
    assert_eq!(modifier, "");
}

#[test]
fn test_prompt_modifier_lenient_strictness() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 1;
    
    let modifier = profile.generate_prompt_modifier();
    assert!(modifier.contains("宽松标准"));
    assert!(modifier.contains("高风险"));
}

#[test]
fn test_prompt_modifier_strict_strictness() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 5;
    
    let modifier = profile.generate_prompt_modifier();
    assert!(modifier.contains("非常严格"));
    assert!(modifier.contains("可疑代码"));
}

#[test]
fn test_prompt_modifier_with_suppressed_patterns() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 3; // No strictness modifier
    profile.preferences.suppressed_issue_patterns = vec![
        "未使用的变量".to_string(),
        "代码重复".to_string(),
    ];
    
    let modifier = profile.generate_prompt_modifier();
    assert!(modifier.contains("个性化调整"));
    assert!(modifier.contains("未使用的变量"));
    assert!(modifier.contains("代码重复"));
}

#[test]
fn test_prompt_modifier_combined() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 2;
    profile.preferences.suppressed_issue_patterns = vec!["空指针".to_string()];
    
    let modifier = profile.generate_prompt_modifier();
    assert!(modifier.contains("较宽松"));
    assert!(modifier.contains("空指针"));
}

#[test]
fn test_profile_updated_at_timestamp() {
    let profile1 = UserProfile::new("test@example.com".to_string());
    std::thread::sleep(std::time::Duration::from_millis(10));
    
    let mut profile2 = UserProfile::new("test@example.com".to_string());
    profile2.update_from_feedback("proceed", "proceeded", None, None, vec![]);
    
    // updated_at should be more recent after feedback
    assert!(profile2.updated_at > profile1.updated_at);
}

#[test]
fn test_load_nonexistent_profile_returns_none() {
    let (_temp_dir, store) = create_temp_profile_store();
    
    let result = store.load_profile("nonexistent@example.com");
    assert!(result.is_none());
}

#[test]
fn test_empty_storage_loads_successfully() {
    let (_temp_dir, store) = create_temp_profile_store();
    
    // Should not panic when file doesn't exist
    let result = store.load_profile("any@example.com");
    assert!(result.is_none());
}

#[test]
fn test_severity_level_serde() {
    use serde_json;
    
    let low = SeverityLevel::Low;
    let json = serde_json::to_string(&low).unwrap();
    assert_eq!(json, "\"low\"");
    
    let deserialized: SeverityLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, SeverityLevel::Low);
}

#[test]
fn test_full_profile_serialization() {
    let mut profile = UserProfile::new("test@example.com".to_string());
    profile.preferences.strictness_level = 2;
    profile.preferences.suppressed_issue_patterns.push("issue1".to_string());
    profile.stats.total_reviews = 42;
    profile.stats.false_positive_by_type.insert("issue1".to_string(), 3);
    
    let json = serde_json::to_string(&profile).unwrap();
    let deserialized: UserProfile = serde_json::from_str(&json).unwrap();
    
    assert_eq!(deserialized.user_id, profile.user_id);
    assert_eq!(deserialized.preferences.strictness_level, 2);
    assert_eq!(deserialized.stats.total_reviews, 42);
    assert_eq!(
        deserialized.stats.false_positive_by_type.get("issue1"),
        Some(&3)
    );
}

#[test]
fn test_realistic_user_journey() {
    let mut profile = UserProfile::new("developer@example.com".to_string());
    
    // Week 1: New user, follows AI recommendations mostly
    for _ in 0..5 {
        profile.update_from_feedback(
            "review",
            "proceeded",
            Some(4),
            Some(true),
            vec![],
        );
    }
    
    assert_eq!(profile.stats.total_reviews, 5);
    assert_eq!(profile.preferences.strictness_level, 3); // Still default
    
    // Week 2: Start noticing false positives
    for _ in 0..3 {
        profile.update_from_feedback(
            "block",
            "proceeded",
            Some(3),
            Some(false),
            vec!["未使用的变量".to_string()],
        );
    }
    
    // After 3 false positives, issue should be suppressed
    assert!(profile.preferences.suppressed_issue_patterns.contains(&"未使用的变量".to_string()));
    
    // Week 3: Continue disagreeing, strictness should decrease
    for _ in 0..5 {
        profile.update_from_feedback(
            "block",
            "proceeded",
            Some(2),
            Some(false),
            vec![],
        );
    }
    
    // High disagreement rate should lower strictness
    assert!(profile.preferences.strictness_level < 3);
    
    // Check summary stats
    assert_eq!(profile.stats.total_reviews, 13);
    assert!(profile.avg_helpfulness_score() < 4.0);
    assert!(profile.agreement_rate() < 0.5);
}

#[test]
fn test_concurrent_save_does_not_corrupt() {
    let (_temp_dir, store) = create_temp_profile_store();
    
    let mut profile1 = UserProfile::new("user1@example.com".to_string());
    let mut profile2 = UserProfile::new("user2@example.com".to_string());
    
    profile1.stats.total_reviews = 10;
    profile2.stats.total_reviews = 20;
    
    // Save both profiles
    store.save_profile(&profile1).unwrap();
    store.save_profile(&profile2).unwrap();
    
    // Both should be retrievable with correct data
    let loaded1 = store.load_profile("user1@example.com").unwrap();
    let loaded2 = store.load_profile("user2@example.com").unwrap();
    
    assert_eq!(loaded1.stats.total_reviews, 10);
    assert_eq!(loaded2.stats.total_reviews, 20);
}
