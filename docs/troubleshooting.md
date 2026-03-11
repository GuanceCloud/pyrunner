# 故障排查

## 1. `search` 没有结果

检查：

- 描述是否足够清晰
- 标签是否合理
- 查询词是否过于泛化
- 阈值是否太高

可以先试：

```bash
pyrunner search "your query" --threshold 0.3
```

## 2. `run` 执行失败

常见原因：

- 本地没有 `python3`
- 脚本本身运行报错
- 脚本依赖没有安装

建议：

- 先用 `get` 拿到 `path`
- 再手动运行确认：

```bash
python3 /absolute/path/to/script.py
```

## 3. 注册重复脚本

`pyrunner` 会按内容 hash 去重。

如果脚本内容完全相同，即使描述不同，也会返回已有脚本。

## 4. 删除后找不到脚本

`delete --yes` 会删除：

- 数据库记录
- FTS 索引记录
- 脚本文件

## 5. 测试命令

```bash
cargo test
```
