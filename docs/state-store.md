# 文件 StateStore 设计

实现位于 `daily-paper-core/src/state/`。

## 总体原则

- 所有写入原子化：先写临时文件，再 rename
- 不依赖文件修改时间作为业务状态
- 缓存判断基于显式 ID、hash、远端 version 和配置 hash
- 大文件和 manifest 分开存储
- 单用户单进程是默认场景，但仍需防止定时任务重叠

## 根目录

默认使用 `--directory/-d` 参数或配置文件所在目录。dev 模式默认 `./data`，release 模式使用平台 app data 目录。

## 目录布局

```text
<root>/
├── config.toml
├── lock                          # 进程锁
├── runs/                         # RunManifest JSON
├── cache/
│   ├── arxiv/                    # arXiv 候选列表
│   ├── embeddings/               # embedding 向量 + metadata
│   ├── models/                   # fastembed 模型文件
│   ├── papers/                   # PDF、正文、精读结果
│   ├── rerank/                   # rerank 结果
│   ├── reports/                  # 版本化报告（按 run_id）
│   ├── deliveries/               # 发送记录
│   └── zotero/snapshots/         # Zotero 快照
├── archive/<date>/               # 每日归档
│   ├── candidates.json
│   └── report/                   # HTML 报告快照
└── ui/                           # Web UI 静态文件
```

缓存子目录由 `CACHE_KINDS` 常量统一定义（`state/store.rs`），`ensure_dirs`、`cache list`、`cache clean` 共用。

## 关键组件

### StatePath（`state/path.rs`）

所有文件访问通过 typed `StatePath` 构造。禁止 `..`、绝对路径、空 path segment。防止路径遍历。

### 原子写入（`state/atomic.rs`）

JSON 序列化 → 写入同目录临时文件 → rename 到目标路径。

### 进程锁（`state/lock.rs`）

使用 `create_new` 语义创建 lock 文件。stale 检测基于文件 age（默认 6 小时），不依赖 PID。

### StateStore（`state/store.rs`）

提供 `read_json`、`write_json`、`read_bytes`、`write_bytes`、`exists`、`ensure_dirs` 等方法。

## 缓存键

所有 hash 输入先规范化（稳定字段顺序、列表排序、trim、标准 ID 格式）。详见各模块的 `compute_*_key` 函数。

## CLI 命令

```text
daily-paper cache list [--kind <kind>]     # 列出缓存
daily-paper cache clean --kind <kind>      # 清理指定缓存
daily-paper archive list [--date <date>]   # 列出归档
daily-paper archive report --date <date>   # 查看报告
daily-paper config show                    # 显示配置
daily-paper config path                    # 显示路径
```

有效 kind：`arxiv, embeddings, models, papers, rerank, zotero, deliveries, reports, runs, all`

## 恢复流程

1. 获取 lock → 创建 run manifest
2. 读取配置，计算 config_hash
3. 按阶段顺序检查缓存：Zotero snapshot → 候选论文 → embedding → rerank → 精读 → 报告 → 发送
4. 网络请求前先查缓存，昂贵请求后立即写入
5. 释放 lock

## 发送幂等

`delivery_key = sha256(report_hash + sink_id + sink_config + recipient)`。同一 key 默认只发送一次，`--force-send` 可重复发送。
