# Git-AI Code Review 个性化系统

## 概述

基于用户反馈的智能化代码审核个性化系统，能够学习每个开发者的代码风格和偏好，自动调整审核严格度。

## 核心功能

### 1. 用户画像 (UserProfile)

每个用户（基于 git identity）维护独立的画像，包含：

- **偏好设置** (ReviewPreferences)
  - `strictness_level`: 严格度等级 (1-5)
  - `suppressed_issue_patterns`: 被抑制的问题类型（误报多次后自动添加）
  - `min_severity`: 最小严重级别阈值
  - `skip_file_patterns`: 跳过审核的文件模式

- **统计数据** (ReviewStats)
  - 总审核次数、反馈次数
  - 同意/不同意 AI 建议的次数
  - 各类问题的误报计数
  - 决策模式统计

### 2. 自适应学习

#### a. 严格度自动调整

```
不同意率 > 60% → 降低严格度 (strictness_level - 1)
```

**示例**：
- 初始严格度：3（中等）
- 用户连续 10 次审核中，7 次选择 "proceed" 而 AI 建议 "block"
- 系统自动调整：严格度降为 2（较宽松）

#### b. 问题类型抑制

```
同一问题被标记误报 ≥ 3 次 → 自动加入抑制列表
```

**示例**：
```json
{
  "false_positive_by_type": {
    "未使用的变量": 5,
    "空值检查": 3
  },
  "suppressed_issue_patterns": [
    "未使用的变量",
    "空值检查"
  ]
}
```

#### c. 个性化 Prompt 修饰

根据用户画像动态调整 AI 审核 prompt：

**严格度调整**：
- Level 1: "采用宽松标准，仅报告明确的高风险问题"
- Level 2: "采用较宽松标准，关注确定性高的问题"
- Level 3: （默认，无调整）
- Level 4: "采用严格标准，包含潜在风险点"
- Level 5: "采用非常严格标准，报告所有可疑代码"

**误报提示**：
```
用户历史反馈显示以下问题类型误报率较高，请特别谨慎评估：未使用的变量、空值检查
```

### 3. 数据持久化

用户画像存储在 `~/.git-ai/review_profiles.json`：

```json
{
  "user@example.com": {
    "user_id": "user@example.com",
    "preferences": {
      "strictness_level": 2,
      "suppressed_issue_patterns": ["未使用的变量"],
      "min_severity": "low",
      "skip_file_patterns": [],
      "auto_proceed_on_low_risk": false
    },
    "stats": {
      "total_reviews": 45,
      "feedback_count": 12,
      "total_helpfulness_score": 48,
      "agreement_count": 8,
      "disagreement_count": 4,
      "false_positive_by_type": {
        "未使用的变量": 5
      },
      "decision_patterns": {
        "block→cancelled": 3,
        "block→proceeded": 4,
        "review→proceeded": 5
      }
    },
    "updated_at": 1711929600
  }
}
```

## 工作流程

```
┌─────────────────┐
│  git commit     │
└────────┬────────┘
         │
         ▼
┌─────────────────────┐
│ 加载用户画像         │
│ strictness=2        │
│ suppressed=[...]    │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ 生成个性化 Prompt    │
│ "采用较宽松标准..."  │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ AI 审核              │
│ findings=[...]      │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ 显示结果，收集反馈    │
│ 评分？误报？         │
└────────┬────────────┘
         │
         ▼
┌─────────────────────┐
│ 更新用户画像         │
│ • disagreement++    │
│ • false_positive++  │
│ • 调整严格度         │
└─────────────────────┘
```

## 使用示例

### 场景 1：新用户首次使用

```bash
$ git commit

=== git-ai qwen 提交审核 ===
模型: qwen-plus
结论: 建议人工确认
摘要: 发现 1 个潜在问题
发现:
1. [medium] 未使用的变量 (src/main.rs)
   变量 `temp` 已声明但未使用

继续提交? [y/N]: y

[可选] 审核结果反馈 (直接回车跳过)
评分 (1-5, 5=非常有帮助): 2
是否认同 AI 的建议? [y/N]: n
哪些问题是误报? (输入序号，用逗号分隔，如 1,3): 1
补充说明 (可选): 这是临时变量，后续会用到

[Personalization] Saved profile for user: user@example.com
```

### 场景 2：经验丰富用户（严格度已调低）

```bash
$ git commit

[CommitReview] User profile loaded: strictness=2, avg_score=3.5, agreement=75.0%

=== git-ai qwen 提交审核 ===
模型: qwen-plus
结论: 可继续
摘要: 未发现明确问题  # ← 因为严格度低，没报告 "未使用的变量"
...
```

### 场景 3：问题已被抑制

用户 3 次标记 "未使用的变量" 为误报后：

```bash
$ git commit

# AI 收到的 system prompt:
"""
你是资深代码审核工程师...

个性化调整：采用较宽松标准，关注确定性高的问题；
用户历史反馈显示以下问题类型误报率较高，请特别谨慎评估：未使用的变量
"""

=== git-ai qwen 提交审核 ===
结论: 可继续
摘要: 未发现需要关注的问题  # ← 即使有未使用的变量，也不报告
```

## API 分析建议

### 后端统计查询

```sql
-- 个性化效果分析
SELECT 
    JSON_EXTRACT(feedback, '$.helpfulness_score') as score,
    COUNT(*) as count,
    AVG(CASE WHEN JSON_EXTRACT(feedback, '$.agrees_with_recommendation')='true' THEN 1.0 ELSE 0.0 END) as agreement_rate
FROM commit_reviews
WHERE feedback IS NOT NULL
GROUP BY score
ORDER BY score;

-- 最常见的抑制问题类型
SELECT 
    JSON_EXTRACT(review, '$.findings[*].title') as issue_title,
    COUNT(*) as occurrences,
    SUM(
        CASE WHEN JSON_ARRAY_CONTAINS(
            JSON_EXTRACT(feedback, '$.false_positive_indices'),
            JSON_INDEX
        ) THEN 1 ELSE 0 END
    ) as false_positives
FROM commit_reviews
WHERE feedback IS NOT NULL
GROUP BY issue_title
HAVING false_positives > 2;
```

### 个性化效果指标

| 指标 | 公式 | 目标值 |
|------|------|--------|
| **反馈采纳率** | `用户 proceed 且 AI 建议 proceed` / 总数 | > 70% |
| **误报改善率** | `(初期误报率 - 当前误报率) / 初期误报率` | > 30% |
| **平均评分提升** | `最近 10 次平均分 - 最早 10 次平均分` | > 0.5 |
| **严格度分布** | Level 1-5 的用户占比 | 正态分布 |

## 配置选项

用户可以手动编辑 `~/.git-ai/review_profiles.json` 来：

1. **调整严格度**：
   ```json
   "strictness_level": 1  // 1=最宽松, 5=最严格
   ```

2. **手动抑制问题类型**：
   ```json
   "suppressed_issue_patterns": [
     "未使用的变量",
     "代码重复",
     "复杂度过高"
   ]
   ```

3. **跳过特定文件**：
   ```json
   "skip_file_patterns": [
     "**/*.test.ts",
     "**/migrations/*.sql"
   ]
   ```

4. **重置画像**：删除对应的用户 key

## 隐私和数据控制

- **本地存储**：用户画像仅存储在本地 `~/.git-ai/review_profiles.json`
- **上传数据**：只上传匿名化的反馈统计，不包含代码内容
- **用户控制**：可随时删除或编辑个人画像文件

## 未来增强方向

1. **团队画像**：共享团队级别的误报规则
2. **A/B 测试**：对比不同 prompt 策略的效果
3. **主动学习**：自动识别用户修改模式
4. **语言偏好**：识别用户使用的编程语言习惯
5. **时间模式**：工作日 vs 周末的严格度自动调整
