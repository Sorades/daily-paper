# 项目结构

daily-paper 使用 workspace + 2 crate：`daily-paper`（bin）和 `daily-paper-core`（lib）。

## Crate 布局

```text
daily-paper/
├── Cargo.toml                    # workspace root
├── config.example.toml           # 配置模板
├── docs/                         # 设计文档
├── ui/                           # Web UI 前端（Svelte 5 + TypeScript）
│   ├── package.json
│   ├── vite.config.ts
│   └── src/
│       ├── types.ts              # 镜像 Rust 模型的 TS 类型
│       ├── api.ts                # 类型化 fetch 封装
│       ├── router.svelte.ts      # Hash 路由
│       ├── stores/               # SSE 连接管理
│       ├── components/           # 可复用组件
│       └── pages/                # 页面组件
├── crates/
│   ├── daily-paper/              # bin crate：CLI 入口 + pipeline 编排
│   │   └── src/
│   │       ├── main.rs
│   │       ├── cli.rs            # clap derive 定义
│   │       ├── log_layer.rs      # tracing → LogBuffer + broadcast layer
│   │       ├── progress.rs       # indicatif 进度条 + SSE 推送
│   │       └── commands/
│   │           ├── run.rs        # pipeline 主流程
│   │           ├── serve.rs      # Web API + SSE
│   │           ├── status.rs
│   │           ├── archive.rs
│   │           ├── cache.rs
│   │           └── config.rs
│   │
│   └── daily-paper-core/         # lib crate：领域类型、实现、状态存储
│       └── src/
│           ├── error.rs          # 统一错误类型（thiserror）
│           ├── config/           # 加载、校验、解析
│           ├── state/            # 文件锁、原子写入、路径安全、存储
│           ├── models/           # 纯数据模型
│           ├── zotero/           # Zotero API + 转换 + 兴趣画像
│           ├── source/arxiv/     # arXiv RSS + 转换
│           ├── embedding/        # OpenAI-compatible + fastembed 本地
│           ├── rerank/           # 余弦相似度 + Top-N 选择
│           ├── pdf/              # 下载、pdftotext 提取、章节解析
│           ├── metadata/         # 论文元数据提取
│           ├── reader/           # LLM 精读 + prompt 模板
│           ├── render/           # HTML + 纯文本渲染
│           └── deliver/          # SMTP 邮件发送
│
└── tests/                        # 集成测试 fixtures
    └── fixtures/
```

## 为什么不用更多 crate

MVP 只有 arXiv 一个 source、SMTP 一个 sink。过早拆分增加 Cargo.toml 维护开销和跨 crate 类型传递成本。等需要多 backend 时再从 core 中抽取子 crate。

## 依赖方向（单向）

```text
daily-paper (bin)
  └── daily-paper-core (lib)

core 内部：
  models, error        ← 最底层，无业务依赖
  config, state        ← 依赖 models, error
  业务模块              ← 依赖 models, error, config, state，彼此不直接依赖
  reader               ← 额外依赖 pdf（读取 ExtractedText）
```

Pipeline 编排在 bin crate 的 `commands/run.rs` 中，按顺序调用各模块。

## 错误策略

- **core**：`thiserror` 结构化错误枚举，调用方按类型做分支处理（如 `RateLimited` 触发退避）
- **bin**：`anyhow::Result` + `.context()` 附加运行时上下文
- `Error::is_retryable()` 方法判断是否值得重试（`RetryableNetwork` 和 `RateLimited`）

## 并发控制

各模块内部使用 `tokio::sync::Semaphore`，配置通过参数传入（`max_concurrency`）。

## 设计决策

- **enum dispatch** 而非 dyn trait：MVP source/sink 数量少，enum 匹配更简单
- **chrono** 而非 time：生态兼容（lettre、tracing-subscriber 都拉 chrono）
- **reqwest rustls-tls**：避免 OpenSSL 编译链
- **lettre 同步 SmtpTransport** + spawn_blocking：不引入实验性 async feature
- **Lock stale 检测**：只检查文件 age，不依赖 sysinfo/PID
- **配置**：直接 API key 字段，dev 模式从 `./config.toml` 加载，release 模式用平台目录
- **CACHE_KINDS 常量**：单一数据源，list/clean/ensure_dirs 共用
- **进度指示**：indicatif 驱动 CLI 进度条 + PipelineEvent SSE 推送到 Web UI
- **Web UI**：静态文件放在 `<data_dir>/ui`，由 `daily-paper serve` 提供；API 契约见 `docs/web-ui.md`
