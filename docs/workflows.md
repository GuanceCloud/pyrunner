# 工作流示例

## 场景一：复用已有脚本

```text
用户需求 -> pyrunner ai check -> exists=true -> ai get -> python tool 执行
```

## 场景二：没有命中，生成新脚本

```text
用户需求 -> pyrunner ai check -> exists=false -> agent 生成脚本 -> ai register -> 后续复用
```

## 场景三：本地调试

```text
register -> search -> run
```

## 场景四：整理缓存

```text
list -> stats -> clean --dry-run -> clean -> delete --yes
```

## 示例命令

### 注册 + 搜索

```bash
pyrunner register ./csv_tool.py --desc "process csv data" --tags "csv,data"
pyrunner search "csv data processor"
```

### 更新标签

```bash
pyrunner update <script_id> --tags "csv,data,report"
```

### 清理旧脚本

```bash
pyrunner clean --older-than 30 --dry-run
pyrunner clean --older-than 30
```
