# AI Agent 集成

## 推荐模式

对 Agent 来说，推荐把 `pyrunner` 当作：

- 缓存层
- 检索层
- 元数据层

而不是完整的运行时管理器。

如果是让 agent 预先学习这个工具，优先读取仓库根目录的 `SKILL.md`。

## 推荐调用顺序

### 模式一：先查再跑

```text
1. pyrunner ai check
2. pyrunner ai get
3. agent 自己调用 python tool
```

### 模式二：先搜再挑

```text
1. pyrunner ai search
2. agent 根据 score 选择脚本
3. agent 自己调用 python tool
```

## 为什么推荐 agent 自己执行

- 参数控制更灵活
- cwd / env / timeout 更适合交给 agent runtime
- 避免把复杂运行时能力塞进 `pyrunner`

## `ai check` 示例

```bash
pyrunner ai check --query "download json and transform data" --threshold 0.5
```

返回示例：

```json
{
  "exists": true,
  "script_id": "abc123_1773222673",
  "path": "/Users/you/.pyrunner/scripts/2026-03/abc123_1773222673.py",
  "score": 0.91,
  "action": "reuse",
  "execute_command": [
    "python3",
    "/Users/you/.pyrunner/scripts/2026-03/abc123_1773222673.py"
  ]
}
```

## 最佳实践

- 用清晰的 `--desc`
- 给脚本打少量高价值标签
- 优先复用高分脚本
- 让 Agent 自己执行返回的 `path`
