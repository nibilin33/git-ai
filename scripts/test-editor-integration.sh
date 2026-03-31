#!/bin/bash
# 测试 git-ai 编辑器集成功能（commit review → VSCode Problems）

set -e

echo "🧪 测试 git-ai 编辑器集成功能"
echo "================================"
echo ""

# 创建测试仓库
TEST_DIR=$(mktemp -d)
echo "📁 测试目录: $TEST_DIR"
cd "$TEST_DIR"

# 初始化 git 仓库
git init
git config user.name "Test User"
git config user.email "test@example.com"

# 创建初始提交
echo "initial content" > base.txt
git add base.txt
git commit -m "Initial commit"

# 启用 commit review (假设已在 ~/.git-ai/config.json 中配置)
# {
#   "commit_review": {
#     "enabled": true,
#     "qwen_api_key": "your-api-key"
#   }
# }

# 创建有问题的代码
cat > test.rs <<'EOF'
fn main() {
    let x = Some(42);
    let value = x.unwrap();  // 潜在 panic
    
    let raw_ptr: *const i32 = std::ptr::null();
    let result = unsafe { *raw_ptr };  // 空指针解引用
    
    println!("{}", value);
}
EOF

git add test.rs

echo ""
echo "📝 准备提交有问题的代码..."
echo "   test.rs 包含: unwrap() 和空指针解引用"
echo ""
echo "⏳ 运行 git commit (将触发 pre-commit review)..."
echo ""

# 尝试提交（可能会触发审核）
# 如果审核失败，脚本会报错
if GIT_AI_DEBUG=1 git commit -m "feat: add test code" 2>&1 | tee /tmp/git-ai-commit-output.log; then
    echo ""
    echo "✅ 提交成功（审核通过或未启用）"
else
    echo ""
    echo "❌ 提交被阻止（审核失败）"
fi

# 检查审核结果文件
REVIEW_FILE="$TEST_DIR/.git/ai/last_review.json"
if [ -f "$REVIEW_FILE" ]; then
    echo ""
    echo "📊 审核结果文件已生成:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    cat "$REVIEW_FILE" | python3 -m json.tool
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    
    # 解析结果
    RECOMMENDATION=$(cat "$REVIEW_FILE" | python3 -c "import sys, json; print(json.load(sys.stdin)['recommendation'])" 2>/dev/null || echo "unknown")
    FINDINGS_COUNT=$(cat "$REVIEW_FILE" | python3 -c "import sys, json; print(len(json.load(sys.stdin)['findings']))" 2>/dev/null || echo "0")
    
    echo "🎯 审核结果:"
    echo "   建议: $RECOMMENDATION"
    echo "   发现问题: $FINDINGS_COUNT 个"
    echo ""
    
    if [ "$FINDINGS_COUNT" -gt 0 ]; then
        echo "📝 问题列表:"
        cat "$REVIEW_FILE" | python3 -c "
import sys, json
review = json.load(sys.stdin)
for i, finding in enumerate(review['findings'], 1):
    print(f\"   {i}. [{finding['severity'].upper()}] {finding['title']}\")
    print(f\"      文件: {finding['file']}\")
    print(f\"      详情: {finding['details']}\")
    print()
"
    fi
    
    echo "✨ VSCode 扩展集成测试:"
    echo "   1. 在 VSCode 中打开此目录: $TEST_DIR"
    echo "   2. 确保 git-ai 扩展已安装并激活"
    echo "   3. 检查以下位置:"
    echo "      - Problems 面板应显示 $FINDINGS_COUNT 个问题"
    echo "      - 状态栏左下角应显示审核状态"
    echo "      - 收到桌面通知（如果审核失败）"
    echo ""
    echo "   配置选项 (settings.json):"
    echo "   {\"
    echo "     \"gitai.review.enableDiagnostics\": true,"
    echo "     \"gitai.review.showSuccessNotification\": false"
    echo "   }"
    echo ""
else
    echo ""
    echo "⚠️  审核结果文件未生成"
    echo "   可能原因:"
    echo "   1. commit review 功能未启用"
    echo "   2. git-ai 未安装或未在 PATH 中"
    echo "   3. ~/.git-ai/config.json 中未配置 API key"
    echo ""
    echo "   检查 commit 输出:"
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    cat /tmp/git-ai-commit-output.log
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
fi

echo ""
echo "🧹 清理测试目录: $TEST_DIR"
cd /tmp
rm -rf "$TEST_DIR"

echo ""
echo "✅ 测试完成"
