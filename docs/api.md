# git-ai Server API 文档

## 飞书接口

### GET /api/feishu/get_config_parameters

获取前端调用飞书 config 接口所需的签名参数。

请求参数：

- url: 前端当前页面 URL，必填

响应示例：

```json
{
  "appid": "cli_xxx",
  "signature": "c2f7f2f2f7f2f2f7f2f2f7f2f2f7f2f2f7f2f2f7",
  "noncestr": "haimati-feishu",
  "timestamp": 1712131200000
}
```

环境变量：

- FEISHU_APP_ID: 飞书应用 App ID
- FEISHU_APP_SECRET: 飞书应用 App Secret
- FEISHU_NONCE_STR: 签名 nonce，可选，默认 haimati-feishu
- FEISHU_JSAPI_TICKET: 可选，若已由外部服务维护 ticket，可直接注入该值跳过实时拉取

Base URL: `https://vue-fabric-editor.run.hzmantu.com`（可通过配置 `api_base_url` 替换为自定义域名）

---

## 公共请求头

所有请求均携带以下请求头：

| Header | 说明 | 必填 |
|--------|------|------|
| `User-Agent` | `git-ai/{version}` | 是 |
| `X-Distinct-ID` | 匿名用户唯一 ID（本地生成，存于 `~/.git-ai/internal/distinct_id`） | 是 |
| `Content-Type` | `application/json`（POST 请求） | POST 必填 |
| `Authorization` | `Bearer {token}`（OAuth 登录后携带） | 否 |
| `X-API-Key` | API Key（配置文件中设置） | 否 |
| `X-Author-Identity` | git 作者身份（仅在 `X-API-Key` 存在时携带，格式：`Name <email> timestamp +0000`） | 否 |

---

## 1. 上报 Metrics

### `POST /worker/metrics/upload`

每次 git commit 后，将 AI 代码统计数据批量上报。

#### Request Body

```json
{
  "v": 1,
  "events": [
    {
      "t": 1710000000,
      "e": 1,
      "v": { ... },
      "a": { ... }
    }
  ]
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `v` | number | API 版本，固定为 `1` |
| `events` | array | MetricEvent 数组，单次最多 250 条 |

#### MetricEvent 结构

| 字段 | 类型 | 说明 |
|------|------|------|
| `t` | u32 | Unix 时间戳（秒） |
| `e` | u16 | 事件类型 ID（见下表） |
| `v` | object | 事件值（sparse array，key 为位置字符串） |
| `a` | object | 公共属性（sparse array，key 为位置字符串） |

#### 事件类型 `e`

| ID | 名称 | 触发时机 |
|----|------|----------|
| `1` | Committed | git commit 时 |
| `2` | AgentUsage | 每次 AI checkpoint 时 |
| `3` | InstallHooks | 执行 `git-ai install-hooks` 时 |
| `4` | Checkpoint | 每个文件 checkpoint 时 |

#### 公共属性 `a`（所有事件共用）

| key | 字段名 | 类型 | 说明 |
|-----|--------|------|------|
| `"0"` | git_ai_version | string | 客户端版本号，如 `"1.1.16"` |
| `"1"` | repo_url | string | 仓库远端 URL |
| `"2"` | author | string | git 作者邮箱 |
| `"3"` | commit_sha | string | 当前 commit SHA |
| `"4"` | base_commit_sha | string | 父 commit SHA |
| `"5"` | branch | string | 当前分支名 |
| `"20"` | tool | string | AI 工具名，如 `"claude-code"`, `"cursor"` |
| `"21"` | model | string | 模型名，如 `"claude-sonnet-4-5"` |
| `"22"` | prompt_id | string | prompt 短哈希 |
| `"23"` | external_prompt_id | string | 外部 prompt ID |
| `"30"` | custom_attributes | string | 自定义属性（JSON 字符串） |

> Sparse array 规则：key 缺失 = 未设置；key 存在但值为 `null` = 显式 null。

#### 推荐维度映射（保持现有上报结构不变）

为了支持按项目、作者、仓库、分支、工具、模型做归因统计，推荐统一使用以下字段映射：

| 统计维度 | 来源 | 说明 |
|------|------|------|
| `project_id` | `a[30].project_id` | 建议写入 `custom_attributes`，值通常为 CI 项目 ID |
| `project_name` | `a[30].project_name` | 建议写入 `custom_attributes`，用于展示 |
| `author` | `a[2]` | git 作者邮箱 |
| `repo_url` | `a[1]` | 仓库远端 URL |
| `branch` | `a[5]` | 当前分支名 |
| `tool` | `a[20]` | AI 工具名 |
| `model` | `a[21]` | 模型名 |

其中 `a[30]` 为 JSON 字符串，建议约定为：

```json
{
  "project_id": "123456",
  "project_name": "my-mini-program"
}
```

这样可以在不修改当前 Metrics 上报结构的情况下，补齐项目维度统计所需的信息。

服务端当前会在处理 Metrics 上报时，优先保留客户端已传入的 `a[30].project_id` 和 `a[30].project_name`；若缺失，则会根据 `a[1].repo_url` 调用项目映射接口自动补齐：

- 接口地址：`https://gitlabhectbyurl-feishu-rverless-hrieqoephq.cn-hangzhou.fcapp.run`
- 请求方式：`GET`
- 请求参数：`repoUrl`
- 响应示例：

```json
{
  "id": "project_id",
  "name": "project_name",
  "path": "path_with_namespace"
}
```

因此推荐客户端在上报时直接传入 `a[30]`，服务端自动补齐仅作为兜底策略。
#### 事件值 `v`（按事件类型）

**e=1 Committed**

| key | 字段名 | 类型 | 说明 |
|-----|--------|------|------|
| `"0"` | human_additions | u32 | 纯人类写的新增行数 |
| `"1"` | git_diff_deleted_lines | u32 | git diff 总删除行数 |
| `"2"` | git_diff_added_lines | u32 | git diff 总新增行数 |
| `"3"` | tool_model_pairs | string[] | 工具/模型对列表，index 0 固定为 `"all"` 表示汇总，后续为各工具，如 `["all", "claude-code:claude-sonnet-4-5"]` |
| `"4"` | mixed_additions | u32[] | AI 写的、被人类编辑过的行数（与 tool_model_pairs 平行） |
| `"5"` | ai_additions | u32[] | AI 贡献的总新增行数（= ai_accepted + mixed） |
| `"6"` | ai_accepted | u32[] | AI 写的、未经修改直接提交的行数 |
| `"7"` | total_ai_additions | u32[] | AI 生成的所有新增行（含最终被删除的） |
| `"8"` | total_ai_deletions | u32[] | AI 生成的所有删除行 |
| `"9"` | time_waiting_for_ai | u64[] | 等待 AI 响应的时间（秒） |
| `"10"` | first_checkpoint_ts | u64 | 第一个 checkpoint 的时间戳 |
| `"11"` | commit_subject | string | commit 标题 |
| `"12"` | commit_body | string | commit 正文（可为 null） |

**e=2 AgentUsage**

每次 AI checkpoint 时记录 token 使用量，用于统计模型调用成本。

| key | 字段名 | 类型 | 说明 |
|-----|--------|------|------|
| `"0"` | input_tokens | u32 | 输入 token 数量（prompt/上下文） |
| `"1"` | output_tokens | u32 | 输出 token 数量（AI 生成的内容） |
| `"2"` | total_tokens | u32 | 总 token 数量（可选，可由前两者计算） |

> 注：agent 信息（tool、model、prompt_id）位于公共属性 `a` 中。

**e=3 InstallHooks**

| key | 字段名 | 类型 | 说明 |
|-----|--------|------|------|
| `"0"` | tool_id | string | 工具名，如 `"cursor"`, `"claude-code"` |
| `"1"` | status | string | `"not_found"` / `"installed"` / `"already_installed"` / `"failed"` |
| `"2"` | message | string | 错误信息或警告（可为 null） |

**e=4 Checkpoint**

| key | 字段名 | 类型 | 说明 |
|-----|--------|------|------|
| `"0"` | checkpoint_ts | u64 | checkpoint 时间戳 |
| `"1"` | kind | string | `"human"` / `"ai_agent"` / `"ai_tab"` |
| `"2"` | file_path | string | 文件相对路径 |
| `"3"` | lines_added | u32 | 新增行数 |
| `"4"` | lines_deleted | u32 | 删除行数 |
| `"5"` | lines_added_sloc | u32 | 新增有效代码行数（排除空行/注释） |
| `"6"` | lines_deleted_sloc | u32 | 删除有效代码行数 |

#### 完整请求示例

```json
{
  "v": 1,
  "events": [
    {
      "t": 1710000000,
      "e": 1,
      "v": {
        "0": 20,
        "1": 5,
        "2": 80,
        "3": ["all", "claude-code:claude-sonnet-4-5"],
        "4": [10, 10],
        "5": [60, 60],
        "6": [50, 50],
        "7": [70, 70],
        "8": [8, 8],
        "9": [30, 30],
        "10": 1709999900,
        "11": "feat: add login page",
        "12": null
      },
      "a": {
        "0": "1.1.16",
        "1": "https://github.com/org/repo",
        "2": "dev@example.com",
        "3": "abc123def456",
        "5": "main",
        "20": "claude-code",
        "21": "claude-sonnet-4-5",
        "30": "{\"project_id\":\"123456\",\"project_name\":\"my-mini-program\"}"
      }
    },
    {
      "t": 1709999950,
      "e": 2,
      "v": {
        "0": 1250,
        "1": 850,
        "2": 2100
      },
      "a": {
        "0": "1.1.16",
        "20": "claude-code",
        "21": "claude-sonnet-4-5",
        "22": "d9978a87",
        "23": "session-uuid-xxx"
      }
    }
  ]
}
```

#### Response

**200 OK**
```json
{
  "errors": []
}
```

部分失败时 `errors` 非空：
```json
{
  "errors": [
    { "index": 2, "error": "invalid event_id" }
  ]
}
```

**400 Bad Request**
```json
{ "error": "Invalid request body", "details": "..." }
```

**401 Unauthorized**

**500 Internal Server Error**
```json
{ "error": "Internal server error" }
```

---

## 2. CAS 上传

### `POST /worker/cas/upload`

上传 prompt/transcript 内容到内容寻址存储（Content Addressable Storage）。

#### Request Body

```json
{
  "objects": [
    {
      "hash": "sha256hex...",
      "content": { "messages": [ ... ] },
      "metadata": { "key": "value" }
    }
  ]
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `objects` | array | 待上传对象列表 |
| `objects[].hash` | string | 内容的 SHA256 哈希（hex） |
| `objects[].content` | object | 任意 JSON 内容 |
| `objects[].metadata` | object | 可选的键值对元数据（为空时省略） |

`content` 通常为 `CasMessagesObject` 格式：
```json
{
  "messages": [
    { "type": "user", "text": "帮我写一个登录页", "timestamp": "2024-01-01T00:00:00Z" },
    { "type": "assistant", "text": "好的，以下是登录页代码...", "timestamp": "2024-01-01T00:00:01Z" },
    { "type": "thinking", "text": "..." },
    { "type": "plan", "text": "..." },
    { "type": "tool_use", "name": "write_file", "input": { "path": "login.tsx" } }
  ]
}
```

Message 类型：

| type | 字段 | 说明 |
|------|------|------|
| `user` | `text`, `timestamp?` | 用户输入 |
| `assistant` | `text`, `timestamp?` | AI 回复 |
| `thinking` | `text`, `timestamp?` | AI 思考过程 |
| `plan` | `text`, `timestamp?` | AI 计划 |
| `tool_use` | `name`, `input`, `timestamp?` | 工具调用 |

#### Response

**200 OK**
```json
{
  "results": [
    { "hash": "abc123...", "status": "ok" },
    { "hash": "def456...", "status": "error", "error": "hash mismatch" }
  ],
  "success_count": 1,
  "failure_count": 1
}
```

**400 Bad Request** / **500 Internal Server Error**
```json
{ "error": "...", "details": "..." }
```

---

## 3. CAS 读取

### `GET /worker/cas/?hashes={hash1},{hash2},...`

批量读取已存储的 CAS 内容，最多 100 个 hash。

#### Query Parameters

| 参数 | 类型 | 说明 |
|------|------|------|
| `hashes` | string | 逗号分隔的 SHA256 哈希列表 |

#### 示例

```
GET /worker/cas/?hashes=abc123def456,789xyz...
```

#### Response

**200 OK**
```json
{
  "results": [
    {
      "hash": "abc123...",
      "status": "ok",
      "content": { "messages": [ ... ] }
    },
    {
      "hash": "def456...",
      "status": "error",
      "error": "not found"
    }
  ],
  "success_count": 1,
  "failure_count": 1
}
```

**404 Not Found**：所有 hash 均不存在时返回，客户端视为空结果处理。

---

## 4. 提交审核结果上传

### `POST /worker/commit-review/upload`

当启用提交前代码审核且配置了 `COMMIT_REVIEW_UPLOAD_URL` 时，客户端会在研发确认是否继续提交后，将本次审核结果上传到服务端。

说明：

- 服务端默认实现路径为 `/worker/commit-review/upload`，客户端应将 `COMMIT_REVIEW_UPLOAD_URL` 配置为该完整地址。
- 当前实现不会上传完整 diff，只上传仓库信息、暂存文件列表、审核结论和模型原始返回文本。

#### Request Body

```json
{
  "created_at": "2026-03-18T10:20:30+00:00",
  "repository_path": "/path/to/repo",
  "head": "abc123def4567890",
  "remotes": [
    ["origin", "git@github.com:org/repo.git"]
  ],
  "staged_files": [
    "src/foo.rs",
    "src/bar.rs"
  ],
  "diff_truncated": false,
  "decision": "proceeded",
  "review": {
    "model": "qwen-plus",
    "summary": "整体风险较低，但有一个边界条件需要确认。",
    "recommendation": "review",
    "findings": [
      {
        "severity": "high",
        "file": "src/foo.rs",
        "title": "空值分支可能 panic",
        "details": "当返回值为空时这里直接 unwrap，会导致运行时崩溃。"
      }
    ],
    "raw_response": "{\"summary\":\"...\"}"
  },
  "feedback": {
    "helpfulness_score": 4,
    "agrees_with_recommendation": true,
    "false_positive_indices": [],
    "comment": "问题识别准确"
  }
}
```

#### 顶层字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `created_at` | string | 是 | 上传时间，RFC3339 格式 |
| `repository_path` | string | 是 | 本地仓库路径 |
| `head` | string/null | 否 | 当前 HEAD commit SHA；空仓库场景可为 `null` |
| `remotes` | array | 是 | 远端仓库列表，每项格式为 `[remote_name, remote_url]` |
| `staged_files` | string[] | 是 | 参与审核的暂存文件路径列表 |
| `diff_truncated` | boolean | 是 | 送审 diff 是否因大小限制被截断 |
| `decision` | string | 是 | 研发最终决策：`proceeded` / `cancelled_by_user` / `blocked_non_interactive` |
| `review` | object | 是 | 审核结果对象 |
| `feedback` | object | 否 | 用户反馈信息（可选） |

#### `review` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `model` | string | 是 | 大模型名称 |
| `summary` | string | 是 | 审核摘要 |
| `recommendation` | string | 是 | 模型建议：`proceed` / `review` / `block` |
| `findings` | array | 是 | 问题列表；无问题时为空数组 |
| `raw_response` | string | 是 | 模型原始文本响应，便于服务端留档与排查 |

#### `review.findings[]` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `severity` | string | 是 | 问题严重级别，当前约定为 `high` / `medium` / `low` |
| `file` | string | 否 | 问题对应文件路径；模型未定位到文件时可为空字符串 |
| `title` | string | 是 | 问题标题 |
| `details` | string | 是 | 问题详情 |

#### `feedback` 字段

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `helpfulness_score` | integer | 是 | 用户对本次审核结果的帮助度评分 |
| `agrees_with_recommendation` | boolean | 是 | 用户是否认可模型给出的 recommendation |
| `false_positive_indices` | integer[] | 是 | 用户认为误报的问题索引列表，对应 `review.findings` 的下标 |
| `comment` | string | 否 | 用户补充说明 |

#### Response

推荐服务端返回如下成功结果：

**200 OK**
```json
{
  "success": true,
  "id": 1
}
```

如果服务端校验失败，建议返回：

**400 Bad Request**
```json
{
  "error": "Invalid request body",
  "details": "review.summary is required"
}
```

服务端异常时：

**500 Internal Server Error**
```json
{
  "error": "Internal server error"
}
```

---

## 5. 创建 Bundle

### `POST /api/bundles`

创建一个可分享的代码归因链接，包含 prompt 和文件 diff 信息。

#### Request Body

```json
{
  "title": "feat: add login page",
  "data": {
    "prompts": {
      "d9978a8723e02b52": {
        "agent_id": {
          "tool": "claude-code",
          "id": "session-uuid-xxx",
          "model": "claude-sonnet-4-5"
        },
        "human_author": "dev@example.com",
        "total_additions": 10,
        "total_deletions": 2,
        "accepted_lines": 8,
        "overriden_lines": 2,
        "messages_url": "https://vue-fabric-editor.run.hzmantu.com/cas/abc123...",
        "messages": [
          { "type": "user", "text": "帮我写登录页" },
          { "type": "assistant", "text": "好的..." }
        ],
        "custom_attributes": { "ticket": "JIRA-123" }
      }
    },
    "files": {
      "src/login.tsx": {
        "annotations": {
          "d9978a8723e02b52": [1, [5, 10], 20]
        },
        "diff": "--- a/src/login.tsx\n+++ b/src/login.tsx\n...",
        "base_content": "// original file content"
      }
    }
  }
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 是 | Bundle 标题，最少 1 个字符 |
| `data.prompts` | object | 是 | prompt 记录，key 为 prompt 短哈希 |
| `data.files` | object | 否 | 文件 diff 和归因，key 为文件路径 |

**PromptRecord 字段：**

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `agent_id.tool` | string | 是 | AI 工具名 |
| `agent_id.id` | string | 是 | 工具内的 session ID |
| `agent_id.model` | string | 是 | 模型名 |
| `human_author` | string | 否 | 人类作者邮箱 |
| `total_additions` | u32 | 否 | AI 生成的总新增行 |
| `total_deletions` | u32 | 否 | AI 生成的总删除行 |
| `accepted_lines` | u32 | 否 | 未修改直接采用的行数 |
| `overriden_lines` | u32 | 否 | 被人类修改过的行数 |
| `messages` | array | 否 | 对话记录（与 messages_url 二选一） |
| `messages_url` | string | 否 | CAS 存储的对话记录 URL |
| `custom_attributes` | object | 否 | 自定义键值对 |

**ApiFileRecord 字段：**

| 字段 | 类型 | 说明 |
|------|------|------|
| `annotations` | object | key 为 prompt 短哈希，value 为行号/范围数组。单行用数字，范围用 `[start, end]` |
| `diff` | string | git diff 输出 |
| `base_content` | string | 修改前的文件内容 |

#### Response

**200 OK**
```json
{
  "success": true,
  "id": "bundle_abc123",
  "url": "https://vue-fabric-editor.run.hzmantu.com/b/bundle_abc123"
}
```

**400 Bad Request**
```json
{ "error": "title is required", "details": { "field": "title" } }
```

**500 Internal Server Error**
```json
{ "error": "Internal server error" }
```

---

## 6. Git-AI 管理接口（Admin）

以下接口统一前缀：`/api/admin/git-ai`，用于后台查看 Metrics/CAS/Bundle 数据。

### 6.1 总览

#### `GET /api/admin/git-ai/overview`

返回 Git-AI 各类数据总览统计。

#### Query Parameters（可选）

| 参数 | 类型 | 说明 |
|------|------|------|
| `low_health_top_n` | int | 返回低健康度项目数量，默认 `5`，范围 `1-50` |
| `min_total_commits` | int | 项目最小提交样本数，默认 `1` |
| `include_unknown` | bool | 是否包含未识别项目的数据，默认 `false` |

#### Response

**200 OK**
```json
{
  "metrics": {
    "total_events": 123,
    "distinct_distinct_ids": 10,
    "event_type_counts": [
      { "event_type": 1, "event_name": "Committed", "count": 80 }
    ],
    "top_tools": [
      { "tool": "claude-code", "count": 70 }
    ],
    "top_models": [
      { "model": "claude-sonnet-4-5", "count": 65 }
    ],
    "low_health_projects": {
      "top_n": 5,
      "min_total_commits": 1,
      "include_unknown": false,
      "skipped_without_project": 3,
      "items": [
        {
          "project_id": "123456",
          "project_name": "my-mini-program",
          "health_score": 61.75,
          "total_commits": 12,
          "ai_commit_count": 9,
          "ai_coverage_rate": 0.75,
          "acceptance_rate": 0.64,
          "rewrite_rate": 0.42,
          "direct_acceptance_rate": 0.58,
          "ai_contribution_rate": 0.39,
          "waiting_seconds_per_ai_line": 1.82,
          "totals": {
            "git_diff_added_lines": 820,
            "human_additions": 310,
            "ai_additions": 320,
            "ai_accepted": 185,
            "mixed_additions": 135,
            "total_ai_additions": 500,
            "total_ai_deletions": 42,
            "time_waiting_for_ai": 582
          },
          "authors": ["dev@example.com"],
          "repo_urls": ["https://github.com/org/repo"],
          "branches": ["main"],
          "tools": ["claude-code"],
          "models": ["claude-sonnet-4-5"],
          "latest_created_at": "2026-03-27T09:30:00"
        }
      ]
    },
    "latest_created_at": "2026-03-27T09:30:00"
  },
  "cas": {
    "total_objects": 20,
    "latest_created_at": "2026-03-27T09:20:00"
  },
  "bundles": {
    "total_bundles": 8,
    "latest_created_at": "2026-03-27T09:10:00"
  },
  "commit_reviews": {
    "total_reviews": 6,
    "latest_created_at": "2026-03-27T09:00:00"
  }
}
```

---

### 6.2 Metrics 汇总

#### `GET /api/admin/git-ai/metrics/summary`

按条件筛选后返回 Metrics 汇总信息。

#### Query Parameters（可选）

| 参数 | 类型 | 说明 |
|------|------|------|
| `event_type` | int | 事件类型（1/2/3/4） |
| `distinct_id` | string | 匿名用户 ID |
| `author` | string | git 作者邮箱（来自属性 `a[2]`） |
| `project_id` | string | 项目 ID（来自 `a[30].project_id`） |
| `project_name` | string | 项目名称（来自 `a[30].project_name`） |
| `tool` | string | 工具名（来自属性 `a[20]`） |
| `model` | string | 模型名（来自属性 `a[21]`） |
| `repo_url` | string | 仓库 URL（来自属性 `a[1]`） |
| `branch` | string | 分支名（来自属性 `a[5]`） |

> 说明：项目维度字段仍沿用现有上报结构，不新增顶层上报字段；服务端从 `custom_attributes` 中解析 `project_id` 和 `project_name` 参与筛选。

#### Response

**200 OK**
```json
{
  "filters": {
    "event_type": 1,
    "distinct_id": null,
    "author": "dev@example.com",
    "project_id": "123456",
    "project_name": null,
    "tool": "claude-code",
    "model": null,
    "repo_url": null,
    "branch": "main"
  },
  "total_events": 80,
  "distinct_distinct_ids": 7,
  "latest_created_at": "2026-03-27T09:30:00",
  "event_type_counts": [
    { "event_type": 1, "event_name": "Committed", "count": 80 }
  ],
  "tool_counts": [
    { "tool": "claude-code", "count": 70 }
  ],
  "model_counts": [
    { "model": "claude-sonnet-4-5", "count": 65 }
  ]
}
```

---

### 6.3 Metrics 事件列表

#### `GET /api/admin/git-ai/metrics/events`

分页查询 Metrics 事件明细，支持与 `metrics/summary` 相同的筛选参数。

#### Query Parameters

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | int | 否 | 页码，默认 `1`，最小 `1` |
| `page_size` | int | 否 | 每页数量，默认 `20`，范围 `1-100` |
| `event_type` | int | 否 | 同上 |
| `distinct_id` | string | 否 | 同上 |
| `author` | string | 否 | 同上 |
| `project_id` | string | 否 | 同上 |
| `project_name` | string | 否 | 同上 |
| `tool` | string | 否 | 同上 |
| `model` | string | 否 | 同上 |
| `repo_url` | string | 否 | 同上 |
| `branch` | string | 否 | 同上 |

#### Response

**200 OK**
```json
{
  "total": 123,
  "page": 1,
  "page_size": 20,
  "items": [
    {
      "id": 10,
      "request_version": 1,
      "t": 1710000000,
      "e": 1,
      "event_name": "Committed",
      "v": { "0": 20 },
      "a": { "20": "claude-code" },
      "distinct_id": "xxxx",
      "authorization": null,
      "api_key": null,
      "author_identity": null,
      "tool": "claude-code",
      "model": "claude-sonnet-4-5",
      "prompt_id": null,
      "external_prompt_id": null,
      "repo_url": "https://github.com/org/repo",
      "author": "dev@example.com",
      "project_id": "123456",
      "project_name": "my-mini-program",
      "commit_sha": "abc123",
      "base_commit_sha": null,
      "branch": "main",
      "custom_attributes": "{\"project_id\":\"123456\",\"project_name\":\"my-mini-program\"}",
      "request_headers": {
        "User-Agent": "git-ai/1.1.16"
      },
      "created_at": "2026-03-27T09:30:00"
    }
  ]
}
```

**400 Bad Request**
```json
{
  "error": "Invalid request",
  "details": "Query parameter 'page_size' must be a positive integer and no greater than 100"
}
```

---

### 6.4 项目健康度

#### `GET /api/admin/git-ai/projects/health`

按项目聚合 Committed 事件，直接返回 AI 健康度指标，便于识别 AI 指标偏低的项目。

#### Query Parameters

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | int | 否 | 页码，默认 `1`，最小 `1` |
| `page_size` | int | 否 | 每页数量，默认 `20`，范围 `1-100` |
| `author` | string | 否 | 按作者筛选 |
| `project_id` | string | 否 | 按项目 ID 筛选 |
| `project_name` | string | 否 | 按项目名称筛选 |
| `tool` | string | 否 | 按工具筛选 |
| `model` | string | 否 | 按模型筛选 |
| `repo_url` | string | 否 | 按仓库筛选 |
| `branch` | string | 否 | 按分支筛选 |
| `include_unknown` | bool | 否 | 是否包含未识别项目的数据，默认 `false` |
| `min_total_commits` | int | 否 | 最小提交样本数，默认 `1` |
| `sort_by` | string | 否 | 排序字段，默认 `health_score` |
| `sort_order` | string | 否 | 排序方向，`asc` 或 `desc`，默认 `asc` |

#### 指标口径

| 字段 | 公式 | 说明 |
|------|------|------|
| `ai_coverage_rate` | `ai_commit_count / total_commits` | 有 AI 落地贡献的提交占比 |
| `acceptance_rate` | `ai_additions / total_ai_additions` | AI 生成新增行最终落地到提交的占比 |
| `rewrite_rate` | `mixed_additions / ai_additions` | AI 落地行中被人类改写的占比 |
| `direct_acceptance_rate` | `ai_accepted / ai_additions` | AI 落地行中未经修改直接提交的占比 |
| `ai_contribution_rate` | `ai_additions / git_diff_added_lines` | 总新增行中由 AI 贡献的占比 |
| `waiting_seconds_per_ai_line` | `time_waiting_for_ai / ai_additions` | 每行 AI 落地代码的平均等待成本 |
| `health_score` | `ai_coverage_rate * 35 + acceptance_rate * 35 + (1 - rewrite_rate) * 30` | 项目健康度分数，范围约 `0-100` |

#### Response

**200 OK**
```json
{
  "total": 2,
  "page": 1,
  "page_size": 20,
  "filters": {
    "author": null,
    "project_id": null,
    "project_name": null,
    "tool": null,
    "model": null,
    "repo_url": null,
    "branch": null,
    "include_unknown": false,
    "min_total_commits": 1,
    "sort_by": "health_score",
    "sort_order": "asc"
  },
  "formula": {
    "health_score": "ai_coverage_rate * 35 + acceptance_rate * 35 + (1 - rewrite_rate) * 30",
    "ai_coverage_rate": "ai_commit_count / total_commits",
    "acceptance_rate": "ai_additions / total_ai_additions",
    "rewrite_rate": "mixed_additions / ai_additions",
    "direct_acceptance_rate": "ai_accepted / ai_additions",
    "ai_contribution_rate": "ai_additions / git_diff_added_lines",
    "waiting_seconds_per_ai_line": "time_waiting_for_ai / ai_additions"
  },
  "skipped_without_project": 3,
  "items": [
    {
      "project_id": "123456",
      "project_name": "my-mini-program",
      "health_score": 61.75,
      "total_commits": 12,
      "ai_commit_count": 9,
      "ai_coverage_rate": 0.75,
      "acceptance_rate": 0.64,
      "rewrite_rate": 0.42,
      "direct_acceptance_rate": 0.58,
      "ai_contribution_rate": 0.39,
      "waiting_seconds_per_ai_line": 1.82,
      "totals": {
        "git_diff_added_lines": 820,
        "human_additions": 310,
        "ai_additions": 320,
        "ai_accepted": 185,
        "mixed_additions": 135,
        "total_ai_additions": 500,
        "total_ai_deletions": 42,
        "time_waiting_for_ai": 582
      },
      "authors": ["dev@example.com"],
      "repo_urls": ["https://github.com/org/repo"],
      "branches": ["main"],
      "tools": ["claude-code"],
      "models": ["claude-sonnet-4-5"],
      "latest_created_at": "2026-03-27T09:30:00"
    }
  ]
}
```

**400 Bad Request**
```json
{
  "error": "Invalid request",
  "details": "Query parameter 'sort_by' is invalid"
}
```

---

### 6.5 CAS 对象列表

### 6.5 项目健康度趋势

#### `GET /api/admin/git-ai/projects/health/trend`

查看单个项目在最近一段时间内的 AI 健康度趋势，默认按天聚合 Committed 事件。

#### Query Parameters

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `project_id` | string | 否 | 项目 ID，和 `project_name` 至少提供一个 |
| `project_name` | string | 否 | 项目名称，和 `project_id` 至少提供一个 |
| `author` | string | 否 | 按作者筛选 |
| `tool` | string | 否 | 按工具筛选 |
| `model` | string | 否 | 按模型筛选 |
| `repo_url` | string | 否 | 按仓库筛选 |
| `branch` | string | 否 | 按分支筛选 |
| `granularity` | string | 否 | 聚合粒度，`day` 或 `week`，默认 `day` |
| `days` | int | 否 | 回溯天数，默认 `30` |
| `min_total_commits` | int | 否 | 每个时间桶的最小提交样本数，默认 `1` |

#### Response

**200 OK**
```json
{
  "filters": {
    "author": null,
    "project_id": "123456",
    "project_name": null,
    "tool": null,
    "model": null,
    "repo_url": null,
    "branch": null,
    "granularity": "day",
    "days": 30,
    "min_total_commits": 1
  },
  "formula": {
    "health_score": "ai_coverage_rate * 35 + acceptance_rate * 35 + (1 - rewrite_rate) * 30",
    "ai_coverage_rate": "ai_commit_count / total_commits",
    "acceptance_rate": "ai_additions / total_ai_additions",
    "rewrite_rate": "mixed_additions / ai_additions",
    "direct_acceptance_rate": "ai_accepted / ai_additions",
    "ai_contribution_rate": "ai_additions / git_diff_added_lines",
    "waiting_seconds_per_ai_line": "time_waiting_for_ai / ai_additions"
  },
  "items": [
    {
      "bucket": "2026-03-25",
      "granularity": "day",
      "project_id": "123456",
      "project_name": null,
      "health_score": 58.3,
      "total_commits": 3,
      "ai_commit_count": 2,
      "ai_coverage_rate": 0.6667,
      "acceptance_rate": 0.61,
      "rewrite_rate": 0.44,
      "direct_acceptance_rate": 0.56,
      "ai_contribution_rate": 0.35,
      "waiting_seconds_per_ai_line": 1.94,
      "totals": {
        "git_diff_added_lines": 180,
        "human_additions": 80,
        "ai_additions": 70,
        "ai_accepted": 39,
        "mixed_additions": 31,
        "total_ai_additions": 115,
        "total_ai_deletions": 9,
        "time_waiting_for_ai": 136
      },
      "authors": ["dev@example.com"],
      "repo_urls": ["https://github.com/org/repo"],
      "branches": ["main"],
      "tools": ["claude-code"],
      "models": ["claude-sonnet-4-5"],
      "latest_created_at": "2026-03-25T18:20:00"
    }
  ]
}
```

**400 Bad Request**
```json
{
  "error": "Invalid request",
  "details": "Query parameter 'project_id' or 'project_name' is required"
}
```

---

### 6.6 CAS 对象列表

#### `GET /api/admin/git-ai/cas/objects`

分页查询 CAS 对象。

#### Query Parameters

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | int | 否 | 页码，默认 `1` |
| `page_size` | int | 否 | 每页数量，默认 `20`，范围 `1-100` |
| `hash_prefix` | string | 否 | 按 hash 前缀过滤 |

#### Response

**200 OK**
```json
{
  "total": 20,
  "page": 1,
  "page_size": 20,
  "items": [
    {
      "hash": "abc123...",
      "metadata": { "key": "value" },
      "message_count": 5,
      "created_at": "2026-03-27T09:20:00",
      "updated_at": "2026-03-27T09:20:00"
    }
  ]
}
```

---

### 6.7 CAS 对象详情

#### `GET /api/admin/git-ai/cas/objects/{object_hash}`

查询单个 CAS 对象详情，包含完整 `content`。

#### Response

**200 OK**
```json
{
  "hash": "abc123...",
  "metadata": { "key": "value" },
  "message_count": 5,
  "content": {
    "messages": [
      { "type": "user", "text": "帮我写登录页" }
    ]
  },
  "created_at": "2026-03-27T09:20:00",
  "updated_at": "2026-03-27T09:20:00"
}
```

**404 Not Found**
```json
{ "error": "CAS object not found" }
```

---

### 6.8 Bundle 列表

#### `GET /api/admin/git-ai/bundles`

分页查询 Bundle 列表。

#### Query Parameters

| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `page` | int | 否 | 页码，默认 `1` |
| `page_size` | int | 否 | 每页数量，默认 `20`，范围 `1-100` |
| `title` | string | 否 | 标题模糊匹配 |

#### Response

**200 OK**
```json
{
  "total": 8,
  "page": 1,
  "page_size": 20,
  "items": [
    {
      "id": "bundle_abc123",
      "title": "feat: add login page",
      "prompt_count": 1,
      "file_count": 2,
      "url": "https://vue-fabric-editor.run.hzmantu.com/b/bundle_abc123",
      "created_at": "2026-03-27T09:10:00",
      "updated_at": "2026-03-27T09:10:00"
    }
  ]
}
```

---

### 6.9 Bundle 详情

#### `GET /api/admin/git-ai/bundles/{bundle_id}`

查询单个 Bundle 详情，包含完整 `data` 与请求头快照。

#### Response

**200 OK**
```json
{
  "id": "bundle_abc123",
  "title": "feat: add login page",
  "prompt_count": 1,
  "file_count": 2,
  "url": "https://vue-fabric-editor.run.hzmantu.com/b/bundle_abc123",
  "data": {
    "prompts": {},
    "files": {}
  },
  "request_headers": {
    "User-Agent": "git-ai/1.1.16"
  },
  "created_at": "2026-03-27T09:10:00",
  "updated_at": "2026-03-27T09:10:00"
}
```

**404 Not Found**
```json
{ "error": "Bundle not found" }
```

---

## 接口汇总

| 接口 | 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|------|
| 上报 Metrics | POST | `/worker/metrics/upload` | 可选 | 核心统计，每次 commit 触发 |
| CAS 上传 | POST | `/worker/cas/upload` | 可选 | 存储 prompt/transcript 内容 |
| CAS 读取 | GET | `/worker/cas/` | 可选 | 读取已存储内容 |
| 提交审核结果上传 | POST | `/worker/commit-review/upload` | 可选 | 上传提交前代码审核结果 |
| 创建 Bundle | POST | `/api/bundles` | 可选 | 生成可分享的归因链接 |
| Git-AI 管理总览 | GET | `/api/admin/git-ai/overview` | 可选 | 查看 Metrics/CAS/Bundle/Review 总览 |
| Git-AI Metrics 汇总 | GET | `/api/admin/git-ai/metrics/summary` | 可选 | 按条件统计事件分布 |
| Git-AI Metrics 事件列表 | GET | `/api/admin/git-ai/metrics/events` | 可选 | 分页查询事件明细 |
| Git-AI 项目健康度 | GET | `/api/admin/git-ai/projects/health` | 可选 | 按项目聚合 AI 覆盖率、采纳率、改写率等指标 |
| Git-AI 项目健康度趋势 | GET | `/api/admin/git-ai/projects/health/trend` | 可选 | 查看单项目按天或按周的健康度变化 |
| Git-AI CAS 对象列表 | GET | `/api/admin/git-ai/cas/objects` | 可选 | 分页查询 CAS 对象 |
| Git-AI CAS 对象详情 | GET | `/api/admin/git-ai/cas/objects/{object_hash}` | 可选 | 查询 CAS 内容详情 |
| Git-AI Bundle 列表 | GET | `/api/admin/git-ai/bundles` | 可选 | 分页查询 Bundle |
| Git-AI Bundle 详情 | GET | `/api/admin/git-ai/bundles/{bundle_id}` | 可选 | 查询 Bundle 完整详情 |
| 分页查询构建历史（后台） | GET | `/api/admin/size-history` | 可选 | 按条件分页查询构建历史 |
| 获取最新构建历史 | GET | `/api/size-history/latest` | 可选 | 获取指定项目和构建路径的最新构建记录 |
| 上传构建结果 | POST | `/api/size-history` | 可选 | 上传新的构建体积数据 |

> 认证方式优先级：`X-API-Key` > `Authorization: Bearer`。未认证时仍可上报，服务端可根据 `X-Distinct-ID` 做匿名追踪。



# Size Analysis API 接口文档

## 1. 获取最新构建历史

### 接口信息

- **Method**: `GET`
- **Path**: `/api/size-history/latest`
- **Description**: 获取指定项目和构建路径的最新一次构建历史记录

### 请求参数

#### Query Parameters

| 参数名 | 类型 | 必填 | 说明 | 示例 |
|--------|------|------|------|------|
| projectId | string | 是 | 项目 ID，通常为 CI_PROJECT_ID | `123456` |
| buildPath | string | 是 | 构建路径，需 URL 编码 | `dist/build/mp-weixin` |

#### Headers

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| Content-Type | string | 是 | 固定值：`application/json` |
| Authorization | string | 否 | Bearer Token，格式：`Bearer <token>` |

### 请求示例

```bash
curl -X GET \
  'https://api.example.com/api/size-history/latest?projectId=123456&buildPath=dist%2Fbuild%2Fmp-weixin' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-token-here'
```

### 响应数据

#### 成功响应

**Status Code**: `200 OK`

```json
{
  "success": true,
  "data": {
    "version": "1.2.3",
    "timestamp": 1710748800000,
    "totalSize": 2048576,
    "files": {
      "app.js": 512000,
      "common.js": 256000,
      "vendor.js": 1024000,
      "style.css": 128000
    },
    "projectId": "123456",
    "projectName": "my-mini-program",
    "buildPath": "dist/build/mp-weixin",
    "branch": "main",
    "commitId": "a1b2c3d4e5f6"
  },
  "message": "Success"
}
```

#### 无历史记录

**Status Code**: `200 OK`

```json
{
  "success": true,
  "data": null,
  "message": "No history found"
}
```

#### 错误响应

**Status Code**: `400 Bad Request` / `401 Unauthorized` / `500 Internal Server Error`

```json
{
  "success": false,
  "data": null,
  "message": "Error message"
}
```

### 响应字段说明

| 字段名 | 类型 | 说明 |
|--------|------|------|
| success | boolean | 请求是否成功 |
| data | object \| null | 历史记录数据，无记录时为 null |
| message | string | 响应消息 |

#### data 对象字段

| 字段名 | 类型 | 说明 | 示例 |
|--------|------|------|------|
| version | string | 构建版本号 | `"1.2.3"` |
| timestamp | number | 构建时间戳（毫秒） | `1710748800000` |
| totalSize | number | 总体积（字节） | `2048576` |
| files | object | 文件详情，key 为文件名，value 为大小（字节） | `{"app.js": 512000}` |
| projectId | string | 项目 ID | `"123456"` |
| projectName | string | 项目名称 | `"my-mini-program"` |
| buildPath | string | 构建路径 | `"dist/build/mp-weixin"` |
| branch | string | Git 分支名 | `"main"` |
| commitId | string | Git commit ID | `"a1b2c3d4e5f6"` |

---

## 2. 分页查询构建历史（后台）

### 接口信息

- **Method**: `GET`
- **Path**: `/api/admin/size-history`
- **Description**: 后台管理接口，支持按条件分页查询构建历史记录

### 请求参数

#### Query Parameters

| 参数名 | 类型 | 必填 | 说明 | 示例 |
|--------|------|------|------|------|
| page | number | 否 | 页码，默认 1，最小 1 | `1` |
| page_size | number | 否 | 每页大小，默认 20，范围 1-100 | `20` |
| projectId | string | 否 | 按项目 ID 精确筛选 | `123456` |
| buildPath | string | 否 | 按构建路径精确筛选 | `dist/build/mp-weixin` |
| branch | string | 否 | 按分支名精确筛选 | `main` |
| version | string | 否 | 按版本号精确筛选 | `1.2.3` |
| commitId | string | 否 | 按 commit ID 精确筛选 | `a1b2c3d4e5f6` |

### 请求示例

```bash
curl -X GET \
  'https://api.example.com/api/admin/size-history?page=1&page_size=20&projectId=123456&buildPath=dist%2Fbuild%2Fmp-weixin' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-token-here'
```

### 响应数据

#### 成功响应

**Status Code**: `200 OK`

```json
{
  "success": true,
  "data": {
    "total": 2,
    "page": 1,
    "page_size": 20,
    "items": [
      {
        "id": 101,
        "version": "1.2.3",
        "timestamp": 1710748800000,
        "totalSize": 2048576,
        "files": {
          "app.js": 512000,
          "vendor.js": 1024000
        },
        "projectId": "123456",
        "projectName": "my-mini-program",
        "buildPath": "dist/build/mp-weixin",
        "branch": "main",
        "commitId": "a1b2c3d4e5f6"
      }
    ]
  },
  "message": "Success"
}
```

#### 错误响应

**Status Code**: `400 Bad Request`

```json
{
  "success": false,
  "data": null,
  "message": "page_size must be a positive integer and no greater than 100"
}
```

---

## 3. 上传构建结果

### 接口信息

- **Method**: `POST`
- **Path**: `/api/size-history`
- **Description**: 上传新的构建体积数据到服务端

### 请求参数

#### Headers

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| Content-Type | string | 是 | 固定值：`application/json` |
| Authorization | string | 否 | Bearer Token，格式：`Bearer <token>` |

#### Request Body

```json
{
  "version": "1.2.3",
  "timestamp": 1710748800000,
  "totalSize": 2048576,
  "files": {
    "app.js": 512000,
    "common.js": 256000,
    "vendor.js": 1024000,
    "style.css": 128000
  },
  "projectId": "123456",
  "projectName": "my-mini-program",
  "buildPath": "dist/build/mp-weixin",
  "branch": "main",
  "commitId": "a1b2c3d4e5f6"
}
```

#### Body 字段说明

| 字段名 | 类型 | 必填 | 说明 | 示例 |
|--------|------|------|------|------|
| version | string | 是 | 构建版本号 | `"1.2.3"` |
| timestamp | number | 是 | 构建时间戳（毫秒） | `1710748800000` |
| totalSize | number | 是 | 总体积（字节） | `2048576` |
| files | object | 是 | 文件详情映射 | `{"app.js": 512000}` |
| projectId | string | 否 | 项目 ID | `"123456"` |
| projectName | string | 否 | 项目名称 | `"my-mini-program"` |
| buildPath | string | 否 | 构建路径 | `"dist/build/mp-weixin"` |
| branch | string | 否 | Git 分支名 | `"main"` |
| commitId | string | 否 | Git commit ID | `"a1b2c3d4e5f6"` |

### 请求示例

```bash
curl -X POST \
  'https://api.example.com/api/size-history' \
  -H 'Content-Type: application/json' \
  -H 'Authorization: Bearer your-token-here' \
  -d '{
    "version": "1.2.3",
    "timestamp": 1710748800000,
    "totalSize": 2048576,
    "files": {
      "app.js": 512000,
      "common.js": 256000
    },
    "projectId": "123456",
    "projectName": "my-mini-program",
    "buildPath": "dist/build/mp-weixin",
    "branch": "main",
    "commitId": "a1b2c3d4e5f6"
  }'
```

### 响应数据

#### 成功响应

**Status Code**: `200 OK`

```json
{
  "success": true,
  "data": {
    "id": "660a1b2c3d4e5f6789",
    "version": "1.2.3",
    "timestamp": 1710748800000,
    "totalSize": 2048576
  },
  "message": "Build history saved successfully"
}
```

#### 错误响应

**Status Code**: `400 Bad Request` / `401 Unauthorized` / `500 Internal Server Error`

```json
{
  "success": false,
  "data": null,
  "message": "Error message"
}
```

### 响应字段说明

| 字段名 | 类型 | 说明 |
|--------|------|------|
| success | boolean | 请求是否成功 |
| data | object \| null | 保存后的数据（可包含自动生成的 ID） |
| message | string | 响应消息 |

---