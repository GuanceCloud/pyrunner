# pyrunner 帮助文档

欢迎使用 `pyrunner`。

`pyrunner` 是一个面向 AI Agent 的本地 Python 脚本缓存 CLI，核心目标是：

- 缓存历史脚本
- 检索可复用脚本
- 返回可直接执行的绝对路径
- 记录基础使用信息

推荐阅读顺序：

1. [快速开始](./quickstart.md)
2. [CLI 参考](./cli.md)
3. [AI Agent 集成](./agent.md)
4. [工作流示例](./workflows.md)
5. [故障排查](./troubleshooting.md)

如果你想让 agent 快速学习如何使用这个工具，可直接阅读仓库根目录的 `SKILL.md`。

核心原则：

- 核心能力是缓存、检索、复用
- `run` 是辅助命令，主要用于本地调试
- 对 Agent 来说，推荐流程是 `search/check/get -> 自己调用 Python tool`
