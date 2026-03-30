效果证明 —— AI 参与了多少需求、贡献了多少最终上线的代码？
能力边界 —— 哪些场景适合用 AI、哪些不适合？更关键的是，「能跑通」不等于「能力可靠」
问题诊断 —— 采纳率只有 40%，是模型能力不行、知识库内容不够、还是 Prompt 写得不好
Token 成本 —— 
代码质量：上线后的 bug 率、返工率。

# 工程测试
echo "test" >> test.txt
git add test.txt
GIT_AI_DEBUG=1 git commit -m "feat:#XFZ2025-5064 test metrics logging"
cat ~/.git-ai/internal/logs/*.log
GIT_AI_DEBUG=1 git-ai flush-logs