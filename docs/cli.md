# CLI 参考

## 核心命令

### `register`

注册本地 Python 脚本到缓存。

```bash
pyrunner register <script_path> [--desc "..."] [--tags "a,b"]
```

### `search`

按自然语言搜索缓存脚本。

```bash
pyrunner search <query> [--top-k 5] [--threshold 0.5] [--json]
```

### `get`

按 `script_id` 获取脚本元数据和绝对路径。

```bash
pyrunner get <script_id>
```

### `list`

列出缓存脚本。

```bash
pyrunner list [--tags "a,b"] [--limit 20]
```

### `update`

更新描述和标签。

```bash
pyrunner update <script_id> [--desc "..."] [--tags "a,b"]
```

### `delete`

删除脚本和相关元数据。

```bash
pyrunner delete <script_id> --yes
```

### `clean`

清理陈旧脚本。

```bash
pyrunner clean [--older-than 30] [--unused] [--dry-run]
```

## 辅助命令

### `run`

本地调试执行缓存脚本。

```bash
pyrunner run <script_id> -- arg1 arg2
```

返回字段：

- `command`
- `exit_code`
- `stdout`
- `stderr`
- `success`
- `duration_ms`

### `stats`

查看缓存统计信息。

```bash
pyrunner stats
```

## AI 命令

### `ai search`

```bash
pyrunner ai search --query "json script" --top-k 5 --threshold 0.8
```

### `ai check`

```bash
pyrunner ai check --query "json script" --threshold 0.85
```

### `ai get`

```bash
pyrunner ai get <script_id>
```

### `ai register`

三种输入方式：

```bash
pyrunner ai register --script-file ./script.py --desc "..."
pyrunner ai register --stdin --desc "..." < script.py
pyrunner ai register --script-text "print('hello')" --desc "..."
```
