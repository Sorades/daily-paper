# 项目结构

本文档定义 daily-paper 的 crate 组织、模块划分和依赖方向。

## Crate 布局

MVP 阶段使用 workspace + 2 个 crate，不过早拆分：

```text
daily-paper/
├── Cargo.toml                    # workspace root
├── Cargo.lock
├── config.example.toml
├── crates/
│   ├── daily-paper/              # bin crate：CLI 入口 + pipeline 编排
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── main.rs
│   │       ├── cli.rs
│   │       └── commands/
│   │           ├── mod.rs
│   │           ├── run.rs
│   │           ├── status.rs
│   │           └── retry.rs
│   │
│   └── daily-paper-core/         # lib crate：领域类型、trait、实现、状态存储
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── error.rs
│           ├── config/
│           ├── state/
│           ├── models/
│           ├── zotero/
│           ├── source/
│           ├── embedding/
│           ├── rerank/
│           ├── pdf/
│           ├── metadata/
│           ├── reader/
│           ├── render/
│           └── deliver/
│
├── templates/                    # 模板文件（渲染、reader prompt）
│   ├── report.html.jinja
│   ├── report.txt.jinja
│   └── reader-prompt.md
│
└── tests/                        # 集成测试
    ├── fixtures/
    ├── pipeline_test.rs
    └── cli_test.rs
```

### 为什么不用更多 crate

MVP 只有 arXiv 一个 source、SMTP 一个 sink、pdftotext 一个 extractor。过早拆成 9+ crate 会导致：

- 大量 Cargo.toml 维护开销。
- 跨 crate 类型传递需要频繁 `pub` 导出。
- 接口边界不稳定时重构成本高。

等第二版需要引入多 backend（SQLite、不同 reranker）时，再从 `daily-paper-core` 中抽取子 crate。

## 模块组织（daily-paper-core）

```text
daily-paper-core/src/
├── lib.rs                        # re-export 所有公共类型
│
├── error.rs                      # 统一错误类型（thiserror）
│
├── config/
│   ├── mod.rs
│   ├── raw.rs                    # RawConfig：TOML 直接反序列化
│   ├── resolved.rs               # ResolvedConfig：运行时使用，含真实 key，不序列化
│   ├── loader.rs                 # 文件加载、env var 展开
│   ├── validation.rs             # 必填字段校验
│   └── hash.rs                   # 脱敏 config_hash 计算
│
├── state/
│   ├── mod.rs
│   ├── path.rs                   # StatePath 类型安全路径
│   ├── store.rs                  # FileStateStore 实现
│   ├── lock.rs                   # 文件锁
│   └── atomic.rs                 # 原子写入
│
├── models/
│   ├── mod.rs
│   ├── common.rs                 # DateWindow, ContentHash, Author
│   ├── run.rs                    # RunManifest, StageRecord
│   ├── zotero.rs                 # LibraryPaper, ZoteroSnapshot
│   ├── interest.rs               # InterestProfile
│   ├── candidate.rs              # CandidatePaper
│   ├── dedup.rs                  # DedupResult
│   ├── embedding.rs              # EmbeddingRecord（不含 vector 数据）
│   ├── rerank.rs                 # RerankResult, RankedPaper
│   ├── pdf.rs                    # PdfAsset, ExtractedText
│   ├── metadata.rs               # PaperMetadata
│   ├── read.rs                   # ReadTask, ReadResult
│   ├── report.rs                 # RenderedReport, DeliveryReceipt
│   └── id.rs                     # paper_id 规范化、PaperIdAlias
│
├── zotero/
│   ├── mod.rs
│   ├── client.rs                 # Zotero API 调用
│   ├── sync.rs                   # 增量同步逻辑
│   ├── convert.rs                # Zotero JSON -> LibraryPaper
│   ├── collection.rs             # collection path 构建
│   └── profile.rs                # InterestProfile 生成
│
├── source/
│   ├── mod.rs
│   ├── traits.rs                 # PaperSource trait
│   └── arxiv/
│       ├── mod.rs
│       ├── client.rs             # arXiv API + Atom 解析
│       └── convert.rs            # arXiv entry -> CandidatePaper
│
├── embedding/
│   ├── mod.rs
│   ├── traits.rs                 # Embedder trait
│   ├── openai.rs                 # OpenAI-compatible 实现
│   └── cache.rs                  # embedding cache
│
├── rerank/
│   ├── mod.rs
│   ├── traits.rs                 # Reranker trait
│   ├── cosine.rs                 # embedding similarity
│   └── selection.rs              # Top N selection
│
├── pdf/
│   ├── mod.rs
│   ├── download.rs               # PDF 下载
│   ├── extract.rs                # pdftotext 外部命令
│   └── section.rs                # section 识别
│
├── metadata/
│   ├── mod.rs
│   └── fetcher.rs                # MVP: 仅从 arXiv 响应提取
│
├── reader/
│   ├── mod.rs
│   ├── traits.rs                 # Reader trait
│   ├── openai.rs                 # OpenAI-compatible chat/completions
│   ├── template.rs               # prompt 模板
│   └── input.rs                  # section-aware 文本裁剪
│
├── render/
│   ├── mod.rs
│   ├── html.rs                   # HTML 渲染
│   └── text.rs                   # 纯文本 fallback
│
└── deliver/
    ├── mod.rs
    ├── traits.rs                 # Sink trait
    ├── smtp.rs                   # SMTP email
    └── receipt.rs                # 幂等发送
```

## 依赖方向

```text
daily-paper (bin)
  └── daily-paper-core (lib)

daily-paper-core 内部依赖方向（单向）：
  config   ← 无依赖
  error    ← 无依赖
  models   ← 依赖 error
  state    ← 依赖 error, models
  zotero   ← 依赖 error, models, config, state
  source   ← 依赖 error, models, config, state
  embedding ← 依赖 error, models, config, state
  rerank   ← 依赖 error, models, config, state
  pdf      ← 依赖 error, models, config, state
  metadata ← 依赖 error, models, config, state
  reader   ← 依赖 error, models, config, state, pdf
  render   ← 依赖 error, models, config
  deliver  ← 依赖 error, models, config
```

关键原则：

- `models` 不依赖任何业务模块，是最底层的 crate。
- `error` 同样是最底层，不依赖任何业务模块。
- 各业务模块只依赖 `models`、`error`、`config`、`state`，彼此之间不直接依赖。
- `reader` 依赖 `pdf` 是因为它需要读取 `ExtractedText`。
- Pipeline 编排在 bin crate 的 `commands/run.rs` 中，按顺序调用各模块。

## 错误策略

### 分层规则

```text
daily-paper-core  →  thiserror 结构化错误枚举
daily-paper (bin)  →  anyhow::Result，用 .context() 附加运行时上下文
```

`core` 内部不使用 `anyhow`，因为调用方需要 match 错误类型做分支处理（如 `RateLimited` 触发退避、`Blocked` 中止发送）。

### 错误枚举

```rust
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("config error: {0}")]
    Config(String),

    #[error("auth failed: {0}")]
    Auth(String),

    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    #[error("retryable network error: {0}")]
    RetryableNetwork(String),

    #[error("source unavailable: {0}")]
    SourceUnavailable(String),

    #[error("bad source data: {0}")]
    BadSourceData(String),

    #[error("storage error: {0}")]
    Storage(#[from] std::io::Error),

    #[error("PDF download failed: {0}")]
    PdfDownload(String),

    #[error("PDF extract failed: {0}")]
    PdfExtract(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("render error: {0}")]
    Render(String),

    #[error("delivery error: {0}")]
    Delivery(String),

    #[error(transparent)]
    Http(#[from] reqwest::Error),
}
```

### 恢复策略映射

| 错误类型 | 恢复动作 | 说明 |
|---|---|---|
| `Config` | FailRun | 不自动重试 |
| `Auth` | FailRun | 不自动重试 |
| `RateLimited` | RetryWithBackoff | 按 retry-after 或指数退避 |
| `RetryableNetwork` | RetryWithBackoff | 指数退避 |
| `SourceUnavailable` | FailRun | source 不可用 |
| `BadSourceData` | SkipAndWarn | 单条数据问题 |
| `Storage` | FailRun | 文件系统错误 |
| `PdfDownload` | BlockRun | 单篇论文，阻塞发送 |
| `PdfExtract` | BlockRun | 单篇论文，阻塞发送 |
| `Embedding` | RetryThenFail | 先重试，仍失败则 run failed |
| `Llm` | BlockRun | 单篇论文，阻塞发送 |
| `Render` | FailRun | 渲染失败 |
| `Delivery` | RetryThenFail | 报告级别重试 |

## Async Trait 策略

MVP 不使用 `dyn` trait object。理由：

- Rust 1.75+ 支持 `async fn in trait`，但 `dyn dispatch` 仍需额外处理。
- MVP 只有 arXiv 一个 source、SMTP 一个 sink，不需要运行时多态。

MVP 使用 enum dispatch：

```rust
enum PaperSourceImpl {
    Arxiv(ArxivSource),
}

impl PaperSourceImpl {
    async fn fetch(&self, window: &DateWindow) -> Result<Vec<CandidatePaper>> {
        match self {
            Self::Arxiv(s) => s.fetch(window).await,
        }
    }
}

enum SinkImpl {
    Smtp(SmtpSink),
}

impl SinkImpl {
    async fn send(&self, report: &RenderedReport) -> Result<DeliveryReceipt> {
        match self {
            Self::Smtp(s) => s.send(report).await,
        }
    }
}
```

等 source/sink 多到 enum 臃肿时再抽 trait object。

## 并发控制

各模块内部使用 `tokio::sync::Semaphore` 控制并发：

```rust
pub struct OpenAiEmbedder {
    client: reqwest::Client,
    config: EmbeddingConfig,
    semaphore: tokio::sync::Semaphore,
}

impl OpenAiEmbedder {
    pub async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let mut handles = Vec::new();
        for text in texts {
            let permit = self.semaphore.acquire().await?;
            let handle = tokio::spawn(async move {
                let result = self.embed_single(text).await;
                drop(permit);
                result
            });
            handles.push(handle);
        }
        // ...
    }
}
```

配置通过参数传入，不硬编码：

```toml
[embedding]
max_concurrency = 4

[reader]
max_concurrency = 2
```

## Tracing 结构

每个 run 一个 root span，每个 stage 一个 child span，run_id 作为全局字段注入：

```rust
use tracing::{info_span, Instrument};

pub async fn execute(ctx: &RunContext) -> Result<()> {
    let run_span = info_span!("run", run_id = %ctx.run_id);

    async {
        let zotero = stage_zotero_sync(ctx).instrument(info_span!("zotero_sync")).await?;
        let candidates = stage_source_fetch(ctx).instrument(info_span!("source_fetch")).await?;
        // ...
        Ok(())
    }
    .instrument(run_span)
    .await
}
```

## Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = ["crates/*"]

[workspace.dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
thiserror = "2"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter", "json"] }
chrono = { version = "0.4", features = ["serde"] }
sha2 = "0.10"
clap = { version = "4", features = ["derive"] }
lettre = { version = "0.11", features = ["smtp-transport", "tokio1-native-tls"] }
minijinja = "2"
quick-xml = "0.36"
regex = "1"
url = "2"
```

选型说明：

- **chrono vs time**：选 chrono。lettre、tracing-subscriber 等间接依赖都拉 chrono，避免同时出现两套时间类型。
- **reqwest**：`rustls-tls` 而非 `native-tls`，避免拉入 OpenSSL 编译链。
- **lettre**：用同步 `SmtpTransport` + `spawn_blocking`，不引入 lettre 的 async feature（实验性）。
- **clap**：derive 模式，代码量少、类型安全。

## MVP 不需要的依赖

原设计中提到的 `sysinfo` crate 用于检查 lock 文件对应的 PID 是否存在。MVP 改为仅检查 lock 文件 age，省掉这个较重的依赖：

```rust
fn is_stale_lock(lock_path: &Path, stale_after: Duration) -> Result<bool> {
    let metadata = std::fs::metadata(lock_path)?;
    let age = metadata.modified()?.elapsed().unwrap_or(Duration::MAX);
    Ok(age > stale_after)
}
```

## 后续拆分时机

当以下条件满足时，考虑从 `daily-paper-core` 拆出子 crate：

- 需要独立发布某个模块（如 `daily-paper-zotero` 被多个 bin 引用）。
- 某个模块的编译时间显著拖慢整体构建。
- 需要引入真正的多 backend（SQLite StateStore、不同 reranker 实现）。

拆分策略：trait 和类型放一个 `types` crate，实现放各自的 crate，依赖单向：`impl crate -> types crate`。
