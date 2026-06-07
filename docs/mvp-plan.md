# MVP 实现计划

## 目标

本地可运行、可定时的 Rust CLI：自动获取论文 → 基于 Zotero 兴趣画像筛选 → LLM 精读 → 发送报告。失败后可恢复。

## 已实现

- TOML 配置加载与校验（`config/`）
- 文件系统 StateStore（`state/`）
- Zotero 增量同步 + snapshot（`zotero/`）
- arXiv source（RSS/export backend，`source/arxiv/`）
- OpenAI-compatible embedding + fastembed 本地（`embedding/`）
- 余弦相似度 reranker + Top-N（`rerank/`）
- PDF 下载 + pdftotext 提取 + 章节解析（`pdf/`）
- 轻量 metadata 提取（`metadata/`）
- OpenAI-compatible LLM 精读 + prompt 模板（`reader/`）
- HTML + 纯文本渲染（`render/`）
- SMTP 邮件发送（`deliver/`）
- CLI run/status 子命令（`cli.rs`）
- cache/archive/config 管理命令
- Web API + SSE 实时推送（`serve.rs`）
- 进度条 + ETA 指示（`progress.rs`）

## CLI

```text
daily-paper run [-d <dir>] [--date <yyyy-mm-dd>] [--dry-run]
daily-paper status [-d <dir>]
daily-paper serve [-d <dir>] [--port <port>]
daily-paper cache list|clean [--kind <kind>]
daily-paper archive list|report [--date <date>]
daily-paper config show|path|validate
```

配置参考 `config.example.toml`。

## 不做

- 多用户服务、写回 Zotero、SQLite backend
- 复杂推荐算法、人工反馈入口
- 多 source/sink 完整实现
- `retry` 命令（run 本身已支持选择性重试）

## 技术选型

| 用途 | 选择 |
|------|------|
| async runtime | tokio (full) |
| HTTP | reqwest (rustls-tls) |
| 序列化 | serde + serde_json + toml |
| CLI | clap derive |
| 错误 | thiserror (core) + anyhow (bin) |
| 日志 | tracing + tracing-subscriber |
| 时间 | chrono |
| 邮件 | lettre 同步 SmtpTransport + spawn_blocking |
| PDF | 外部 pdftotext 命令 |
| 测试 HTTP | wiremock |
| 进度 | indicatif |

## 测试策略

- **单元测试**：模型序列化、ID 规范化、hash 计算、路径安全、原子写入、章节解析、prompt 构建、HTML 渲染、配置校验
- **集成测试**（wiremock）：Zotero API 分页/429、arXiv 解析、embedding/reader 重试、完整 pipeline 端到端
- **验收**：`cargo test` 全部通过、`cargo clippy` 零警告
