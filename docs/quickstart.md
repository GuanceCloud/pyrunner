# 快速开始

## 1. 构建

```bash
cargo build
```

如果你想直接运行：

```bash
cargo run -- --help
```

## 2. 注册脚本

```bash
pyrunner register ./demo.py --desc "示例脚本" --tags "demo,test"
```

返回示例：

```json
{
  "script_id": "abc123_1773222673",
  "path": "/Users/you/.pyrunner/scripts/2026-03/abc123_1773222673.py"
}
```

## 3. 搜索脚本

```bash
pyrunner search "demo test script"
```

## 4. 获取脚本路径

```bash
pyrunner get <script_id>
```

## 5. 推荐的 Agent 工作流

```text
search/check/get -> 获得脚本 path -> agent 自己调用 python tool 执行
```

例如：

```bash
python3 /absolute/path/to/script.py arg1 arg2
```

## 6. 本地调试执行

如果你只是想快速验证脚本，也可以用：

```bash
pyrunner run <script_id> -- arg1 arg2
```

这个命令会直接执行脚本，并返回 `stdout`、`stderr`、`exit_code`。
