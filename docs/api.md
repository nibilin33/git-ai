# git-ai Server API 文档

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
        "21": "claude-sonnet-4-5"
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

### `POST {COMMIT_REVIEW_UPLOAD_URL}`

当启用提交前代码审核且配置了 `COMMIT_REVIEW_UPLOAD_URL` 时，客户端会在研发确认是否继续提交后，将本次审核结果上传到服务端。

说明：

- 该接口路径不是固定的 git-ai 平台内置路径，而是由客户端常量 `COMMIT_REVIEW_UPLOAD_URL` 指定。
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

#### `feedback` 字段（可选）

用户对审核质量的反馈，仅在交互式场景且用户选择提供反馈时存在。

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `helpfulness_score` | number | 否 | 有用性评分，1-5（5 表示非常有帮助） |
| `agrees_with_recommendation` | boolean | 否 | 是否认同 AI 的建议 |
| `false_positive_indices` | number[] | 否 | 误报的问题索引（0-based），对应 `findings` 数组 |
| `comment` | string | 否 | 用户补充说明 |

#### Response

推荐服务端返回如下成功结果：

**200 OK**
```json
{
  "success": true
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

## 接口汇总

| 接口 | 方法 | 路径 | 认证 | 说明 |
|------|------|------|------|------|
| 上报 Metrics | POST | `/worker/metrics/upload` | 可选 | 核心统计，每次 commit 触发 |
| CAS 上传 | POST | `/worker/cas/upload` | 可选 | 存储 prompt/transcript 内容 |
| CAS 读取 | GET | `/worker/cas/` | 可选 | 读取已存储内容 |
| 提交审核结果上传 | POST | `COMMIT_REVIEW_UPLOAD_URL` | 可选 | 上传提交前代码审核结果 |
| 创建 Bundle | POST | `/api/bundles` | 可选 | 生成可分享的归因链接 |

> 认证方式优先级：`X-API-Key` > `Authorization: Bearer`。未认证时仍可上报，服务端可根据 `X-Distinct-ID` 做匿名追踪。
