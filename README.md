# pyrunner

本地 Python 脚本缓存 CLI，重点是 **缓存、检索、复用**。

推荐工作流：

1. 用 `pyrunner search/check/get` 找到脚本
2. 让 agent tool call 直接执行返回的脚本路径

`pyrunner run` 仍然可用，但主要用于本地调试和快速验证。

当前仓库提供一版可运行的 Rust CLI，覆盖：

- CLI 命令结构
- 配置加载
- SQLite 初始化
- 核心数据模型
- 脚本注册、搜索、获取、列出
- AI 检索与复用判断
- 删除、统计、清理预览
- 辅助执行与使用历史记录

## 核心命令

```bash
pyrunner register ./demo.py --desc "示例脚本" --tags "demo,test"
pyrunner search "demo script"
pyrunner get <script_id>
pyrunner list
pyrunner update <script_id> --desc "新描述" --tags "demo,updated"
pyrunner clean --dry-run
pyrunner delete <script_id> --yes
```

## 辅助命令

```bash
pyrunner run <script_id> -- arg1 arg2
pyrunner stats
```

`run` 会直接调用 Python 解释器执行缓存脚本，并返回：

- `command`
- `exit_code`
- `stdout`
- `stderr`
- `duration_ms`

## 开发

```bash
cargo build
cargo run -- --help
```

## 文档

- Markdown 文档：`docs/README.md`
- 静态 HTML 文档站入口：`site/index.html`
- Agent 学习用技能文件：`SKILL.md`
- 跨平台构建说明：`BUILD.md`
