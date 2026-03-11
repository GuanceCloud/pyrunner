# pyrunner 设计文档

## 1. 文档目标

`pyrunner` 是一个本地 CLI，用于让 AI Agent 复用历史 Python 脚本，减少重复生成、提升执行效率，并为脚本的检索、缓存和清理提供统一接口。

这份文档的目标不是描述“未来所有可能功能”，而是定义一个可以直接实现的 **V1 设计**，同时为后续扩展预留边界。

## 2. V1 范围

### 2.1 V1 必须支持

- 注册脚本：保存脚本文件并写入元数据
- 查找脚本：按描述、标签、函数名进行搜索
- 检查复用：判断是否已有可复用脚本
- 获取脚本：返回脚本路径和元数据
- 清理缓存：删除过期或长期未使用脚本

### 2.2 V1 可选辅助能力

- 执行脚本：按 `script_id` 调用缓存脚本，便于本地调试
- 记录执行结果：更新 `last_used`、`use_count` 和 `usage_history`

### 2.3 V1 不做

- 脚本版本树 / 分支管理
- 多语言脚本缓存
- 插件系统
- 复杂权限沙箱
- 分布式存储或远程同步
- 高级语义检索模型

## 3. 设计原则

- **本地优先**：所有数据默认存储在本机目录
- **可解释**：搜索结果必须给出得分明细
- **可调用**：返回结果要能直接被 Agent 用 tool call 执行
- **弱依赖**：V1 依赖尽量少，避免引入不必要复杂度
- **缓存优先**：核心价值是缓存、检索和复用，不是托管所有运行时能力
- **逐步增强**：解析、匹配、执行辅助先做可用版本，再逐步增强

## 4. 系统概览

```text
AI Agent / User
      │
      ▼
  pyrunner CLI
      │
      ├── Script Store      # 脚本文件存储
      ├── Metadata Store    # SQLite 元数据
      ├── Search Index      # SQLite FTS5
      └── Runner            # 辅助执行
```

## 5. 生命周期

### 5.1 注册流程

```text
输入脚本 -> 规范化内容 -> 计算 hash -> 检查是否重复
                                     │
                      已存在 ---------┘
                                     ▼
                        提取元数据 / 函数 / 依赖
                                     ▼
                           保存脚本文件到缓存目录
                                     ▼
                            写入 scripts + 关联表
                                     ▼
                              更新 search_text / FTS
                                     ▼
                                 返回 script_id
```

### 5.2 检索流程

```text
输入 query -> query 规范化 -> FTS 检索候选 -> 计算综合得分
                                              ▼
                                     过滤阈值并排序
                                              ▼
                                   返回 top-k + 得分明细
```

### 5.3 执行流程（辅助）

```text
输入 script_id -> 读取脚本元数据 -> 选择解释器 -> 构造受控执行参数
                                                   ▼
                                    执行脚本并捕获 stdout/stderr
                                                   ▼
                                        更新 last_used / use_count
```

说明：

- 对 AI Agent 来说，推荐路径仍然是 `search/check/get -> 拿到 path -> 自己调用 Python tool`
- `pyrunner run` 主要用于本地调试、快速验证和统一记录使用历史

## 6. 存储设计

### 6.1 目录结构

```text
~/.pyrunner/
├── scripts/
│   ├── 2026-03/
│   │   ├── <hash>_<ts>.py
│   │   └── <hash>_<ts>.py.meta.json
├── metadata.db
├── config.toml
└── logs/
    └── pyrunner.log
```

说明：

- 脚本按月份分目录，便于查看和清理
- 文件名统一为 `<hash>_<unix_ts>.py`
- 可选 `.meta.json` 仅用于调试或导出，不作为主数据源
- 真正的主数据源是 SQLite

### 6.2 路径约束

- 数据库存储脚本的 **绝对路径**
- 对外接口返回的 `path` 必须是绝对路径
- CLI 内部一律先将输入路径 canonicalize，再写库

## 7. 数据模型

### 7.1 脚本元数据

```json
{
  "id": "uuid",
  "path": "/Users/alice/.pyrunner/scripts/2026-03/abc123_1710000000.py",
  "hash": "sha256:abc123...",
  "description": "Process CSV files and generate statistics",
  "language": "python",
  "entrypoint": "__main__",
  "interpreter": "python3",
  "created_at": "2026-03-11T08:00:00Z",
  "last_used": "2026-03-11T08:30:00Z",
  "use_count": 5,
  "tags": ["csv", "statistics", "data-processing"],
  "functions": [
    {
      "name": "process_csv",
      "signature": "process_csv(filepath: str, columns: list[str] | None = None) -> DataFrame",
      "description": "Load and process CSV file"
    }
  ],
  "dependencies": ["pandas", "numpy"],
  "input_types": ["csv"],
  "output_types": ["json", "table"],
  "parameters": {
    "filepath": { "type": "str", "required": true },
    "columns": { "type": "list[str]", "required": false }
  }
}
```

### 7.2 数据库 Schema

```sql
CREATE TABLE scripts (
    id TEXT PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    hash TEXT NOT NULL,
    description TEXT,
    language TEXT NOT NULL DEFAULT 'python',
    entrypoint TEXT,
    interpreter TEXT,
    created_at TEXT NOT NULL,
    last_used TEXT,
    use_count INTEGER NOT NULL DEFAULT 0,
    input_types TEXT NOT NULL DEFAULT '[]',
    output_types TEXT NOT NULL DEFAULT '[]',
    parameters TEXT NOT NULL DEFAULT '{}',
    search_text TEXT NOT NULL DEFAULT ''
);

CREATE TABLE tags (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
    UNIQUE(script_id, tag)
);

CREATE TABLE functions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id TEXT NOT NULL,
    name TEXT NOT NULL,
    signature TEXT,
    description TEXT,
    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
);

CREATE TABLE dependencies (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id TEXT NOT NULL,
    dependency TEXT NOT NULL,
    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE,
    UNIQUE(script_id, dependency)
);

CREATE TABLE usage_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    script_id TEXT NOT NULL,
    used_at TEXT NOT NULL,
    context TEXT,
    exit_code INTEGER,
    duration_ms INTEGER,
    FOREIGN KEY (script_id) REFERENCES scripts(id) ON DELETE CASCADE
);

CREATE VIRTUAL TABLE scripts_fts USING fts5(
    script_id UNINDEXED,
    search_text
);

CREATE INDEX idx_scripts_hash ON scripts(hash);
CREATE INDEX idx_scripts_last_used ON scripts(last_used);
CREATE INDEX idx_tags_script ON tags(script_id);
CREATE INDEX idx_tags_tag ON tags(tag);
CREATE INDEX idx_functions_script ON functions(script_id);
CREATE INDEX idx_functions_name ON functions(name);
CREATE INDEX idx_dependencies_script ON dependencies(script_id);
CREATE INDEX idx_dependencies_dep ON dependencies(dependency);
```

### 7.3 FTS 同步策略

V1 不使用 external content FTS 模式，避免 `scripts` 表与 FTS 虚表内容不同步。

采用更直接的方案：

- `scripts.search_text` 保存可搜索文本
- 注册或更新脚本时，由应用层拼装 `search_text`
- 同步写入 `scripts_fts(script_id, search_text)`
- 删除脚本时显式删除对应 FTS 记录

`search_text` 推荐由以下内容拼接而成：

- description
- tags
- function names
- function signatures
- dependencies

示例：

```text
process csv files generate statistics csv statistics process_csv pandas numpy
```

## 8. 搜索与匹配

### 8.1 候选召回

V1 采用两阶段：

1. **召回**：使用 FTS5 从 `scripts_fts` 找到候选集
2. **重排**：在 Rust 中按多维特征计算综合得分

### 8.2 得分公式

```text
final_score =
  0.50 * fts_score +
  0.20 * tag_score +
  0.20 * function_score +
  0.10 * usage_score
```

各维度定义：

- `fts_score`：将 FTS 原始分数映射到 `[0, 1]`
- `tag_score`：查询标签与脚本标签的 overlap 比例
- `function_score`：查询中命中的函数名/关键词比例
- `usage_score`：`log(1 + use_count) / log(1 + max_use_count)`

### 8.3 FTS 分数归一化

SQLite 的 `bm25()` 越小越好，不适合直接混合。

V1 建议在同一次候选集中做 min-max 归一化：

```text
raw = -bm25()
fts_score = (raw - min_raw) / (max_raw - min_raw)
```

如果候选只有 1 条，则直接记为 `1.0`。

### 8.4 阈值定义

- `>= 0.85`：可直接复用
- `0.70 ~ 0.85`：可复用但建议人工确认
- `0.50 ~ 0.70`：仅作参考
- `< 0.50`：不建议复用

### 8.5 搜索 SQL 示例

```sql
SELECT
    script_id,
    bm25(scripts_fts) AS bm25_score
FROM scripts_fts
WHERE scripts_fts MATCH ?
LIMIT ?;
```

## 9. CLI 设计

### 9.1 核心命令

```bash
pyrunner register <script_path> --desc "描述" --tags "tag1,tag2"
pyrunner search <query> [--top-k 5] [--threshold 0.8] [--json]
pyrunner get <script_id>
pyrunner list [--tags "tag1"] [--limit 20]
pyrunner update <script_id> [--desc "新描述"] [--tags "tag1,tag2"]
pyrunner clean [--older-than 30] [--unused] [--dry-run]
pyrunner delete <script_id> [--yes]
```

### 9.2 辅助命令

```bash
pyrunner run <script_id> [-- arg1 arg2]
pyrunner stats
```

### 9.3 AI Agent 命令

AI 接口必须满足两个要求：

- 结构化输出，默认 JSON
- 返回的 `path` 必须可直接交给 Agent 的 Python tool call 执行

```bash
pyrunner ai search --query "处理 CSV 文件" --top-k 5 --threshold 0.8
pyrunner ai check --query "处理 CSV 文件" --threshold 0.85
pyrunner ai get <script_id>
pyrunner ai register --script-file ./script.py --desc "描述" --tags "csv,stats"
cat script.py | pyrunner ai register --stdin --desc "描述" --tags "csv,stats"
pyrunner ai register --script-text "print('hello')" --desc "描述"
```

### 9.4 AI JSON 响应

#### `ai check`

```json
{
  "exists": true,
  "script_id": "uuid",
  "path": "/Users/alice/.pyrunner/scripts/2026-03/abc123_1710000000.py",
  "score": 0.92,
  "action": "reuse",
  "execute_command": ["python3", "/Users/alice/.pyrunner/scripts/2026-03/abc123_1710000000.py"]
}
```

#### `ai search`

```json
{
  "results": [
    {
      "script_id": "uuid",
      "path": "/Users/alice/.pyrunner/scripts/2026-03/abc123_1710000000.py",
      "score": 0.89,
      "description": "Process CSV files and generate statistics",
      "match_details": {
        "fts_score": 0.86,
        "tag_score": 0.75,
        "function_score": 0.90,
        "usage_score": 0.50
      }
    }
  ],
  "total": 1
}
```

说明：

- `execute_command` 使用数组，而不是 shell 字符串，避免转义歧义
- 主推荐路径仍然是使用 `path`

## 10. 执行定位与安全模型

### 10.1 产品定位

`pyrunner` 不试图成为完整的脚本运行时管理器。

对 Agent 来说，优先工作流是：

1. `ai check` / `ai search`
2. `ai get`
3. Agent 自己调用 Python tool 执行返回的绝对路径

因此 `run` 在 V1 里是辅助能力，不承担复杂运行时编排职责。

### 10.2 解释器选择优先级

1. 配置文件中的默认解释器
2. 系统发现的 `python3`

### 10.3 执行约束

V1 的辅助执行建议支持以下最小能力：

- 捕获 stdout / stderr
- 返回 exit code
- 记录 duration
- 更新 `use_count` / `last_used`

### 10.4 V1 不承诺

- 真正 OS 级隔离沙箱
- 网络隔离
- 文件系统只读挂载
- syscall 级拦截

因此 V1 的安全目标是 **降低误用风险**，不是 **隔离不可信代码**。
更复杂的超时、环境变量管理、cwd 和权限控制，优先交给 Agent tool runtime，而不是 `pyrunner run`。

## 11. 元数据提取

### 11.1 V1 策略

V1 使用“尽量准确 + 实现简单”的策略：

- 函数解析：先做轻量解析
- 依赖提取：仅提取显式 `import` / `from ... import ...`
- 参数与类型：尽量读取注解；拿不到时允许为空

### 11.2 推荐实现

优先级建议：

1. 先用正则实现最小版本
2. 为解析失败场景保底
3. 后续升级到 `tree-sitter-python` 或其他 AST 解析器

### 11.3 已知限制

正则方案会漏掉：

- 多行函数签名
- 装饰器影响下的复杂定义
- `import a.b as c`
- 条件导入
- 动态导入

因此文档和实现都应明确：V1 解析结果是 **best effort**，不保证完整。

## 12. 错误模型

### 12.1 CLI 返回码

- `0`：成功
- `1`：通用业务错误
- `2`：参数错误
- `3`：脚本不存在
- `4`：执行失败
- `5`：数据库错误

### 12.2 常见错误场景

- 注册的脚本路径不存在
- `ai register` 同时传入多个脚本来源
- 搜索没有任何结果
- Agent 或本地环境里找不到 `python3`
- 数据库损坏或 schema 不兼容

## 13. 配置设计

```toml
cache_dir = "/Users/alice/.pyrunner"
default_interpreter = "python3"
max_scripts = 1000
max_age_days = 90
default_timeout_secs = 60

[matching]
similarity_threshold = 0.85
fts_weight = 0.50
tag_weight = 0.20
function_weight = 0.20
usage_weight = 0.10

[logging]
level = "info"
file = "/Users/alice/.pyrunner/logs/pyrunner.log"
```

## 14. Rust 模块划分

```text
src/
├── main.rs
├── lib.rs
├── cli/
│   ├── mod.rs
│   ├── commands.rs
│   └── output.rs
├── config/
│   └── mod.rs
├── db/
│   ├── mod.rs
│   ├── connection.rs
│   ├── migrations.rs
│   └── queries.rs
├── models/
│   ├── mod.rs
│   ├── script.rs
│   └── result.rs
├── services/
│   ├── mod.rs
│   ├── register.rs
│   ├── search.rs
│   ├── parser.rs
│   ├── runner.rs
│   └── cleanup.rs
└── utils/
    ├── hash.rs
    └── paths.rs
```

说明：

- V1 使用单进程 CLI，数据库层先采用单连接 + 事务
- 暂不引入连接池
- 若未来引入后台服务，再评估连接池或异步架构

## 15. 技术选型

### 15.1 核心依赖

- `clap`：CLI
- `rusqlite`：SQLite
- `serde` / `serde_json`：序列化
- `anyhow`：错误处理
- `sha2`：hash
- `chrono`：时间
- `dirs`：用户目录
- `toml`：配置
- `regex`：V1 解析保底方案

### 15.2 暂不引入

- `tokio`
- 连接池
- 语义向量数据库
- 插件框架

如果未来新增后台服务、批量并发执行或异步 I/O，再重新评估 async。

## 16. 性能设计

### 16.1 V1 性能重点

- 避免重复注册相同脚本
- 限制候选集规模，例如先召回 `top_k * 5`
- 对 `hash`、`tag`、`function name` 建索引
- 使用事务批量写入脚本关联表

### 16.2 后续优化方向

- 预编译查询
- 增量重建搜索索引
- 缓存热门查询结果
- 更强的函数签名匹配

## 17. 可观测性

### 17.1 指标

- 注册次数
- 搜索次数
- 平均搜索耗时
- 缓存命中率
- 复用率
- 辅助执行次数

### 17.2 日志

建议记录：

- script_id
- hash
- query
- top_k
- threshold
- exit_code
- duration_ms

日志中不应直接记录敏感脚本正文。

## 18. 测试策略

### 18.1 单元测试

- hash 计算
- query 规范化
- 得分计算
- 清理策略

### 18.2 集成测试

- 注册 -> 搜索 -> 获取
- 注册重复脚本去重
- `ai check` 阈值行为
- `run` 的成功 / 非零退出码
- `clean --dry-run`

### 18.3 夹具

- 简单函数脚本
- 包含依赖导入的脚本
- 多函数脚本
- 空脚本 / 非法脚本

## 19. 里程碑

### Milestone 1

- 配置加载
- SQLite 初始化
- `register/get/list`

### Milestone 2

- FTS 检索
- 综合打分
- `search/ai search/ai check`

### Milestone 3

- 辅助执行
- 使用历史
- `run/stats/clean`

## 20. 后续扩展

以下能力明确放在 V1 之后：

- tree-sitter / AST 级解析
- 多语言脚本支持
- 版本管理
- 远程同步
- 插件化匹配器
- 向量检索与语义召回

## 21. 一句话总结

`pyrunner` 的 V1 应该是一个 **缓存与检索优先、结果可解释、接口稳定** 的 Python 脚本缓存 CLI；执行能力存在，但主要是辅助，而不是产品中心。
