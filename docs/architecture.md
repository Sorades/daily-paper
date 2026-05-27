# Daily Paper 架构草案

本文档描述一个 Rust 版论文推荐与精读框架的初始设计。目标不是复刻某个一次性脚本，而是把流程拆成可恢复、可缓存、可扩展的有状态流水线。

相关文档：

- `project-structure.md` — crate 组织、模块划分、依赖方向、错误策略。
- `data-model.md` — 核心数据结构。
- `state-store.md` — 文件系统状态存储设计。
- `mvp-plan.md` — MVP 范围、里程碑、技术选型。

## 背景

参考项目 `zotero-arxiv-daily` 的核心流程可以概括为：

1. 从 Zotero 获取用户已有论文，形成兴趣上下文。
2. 从 arXiv 等论文源获取最新论文。
3. 基于已有论文和候选论文做相关性排序。
4. 对 Top N 候选论文调用大模型生成精读结果。
5. 渲染成 HTML 并发送到目标，例如邮件。

这个流程本身是合理的，但脚本式实现容易出现几个问题：

- 某个阶段失败后需要从头执行。
- Zotero 信息短期内变化不大，但频繁全量请求容易触发限制。
- embedding、rerank、LLM 精读结果缺少稳定缓存。
- 单篇论文的 LLM 请求失败会影响整个运行。
- 输出发送失败时，前面昂贵的计算结果可能无法复用。

因此本项目应优先解决状态管理、缓存、幂等执行、错误恢复和模块边界。

## 设计目标

- 支持每日自动运行，也支持本地手动运行。
- 每个阶段可独立重试，失败后再次运行不重复已完成工作。
- Zotero 同步使用增量机制和本地缓存。
- 默认使用全部 Zotero 文献构建兴趣画像，但允许通过 collection/path/filter 调整权重或排除范围。
- 论文源可扩展，arXiv 只是第一种 source。
- reranker、LLM reader、renderer、delivery sink 可替换。
- 默认保留足够的中间状态，便于调试和审计。
- 第一版保持实现范围克制，不做复杂插件系统。

## 非目标

- 第一版不做 Web UI。
- 第一版不做多用户服务。
- 第一版不写回 Zotero。
- 第一版不追求复杂推荐算法，先实现稳定的数据流和恢复机制。
- 第一版不把所有论文源、所有发送渠道都做完。

## 总体流程

```text
config
  |
  v
zotero_sync
  |
  v
source_fetch
  |
  v
deduplicate
  |
  v
embedding
  |
  v
rerank
  |
  v
deep_read
  |
  v
render
  |
  v
send
```

每次运行生成一个 `run` 记录。各阶段写入自己的状态和产物。后续运行可以根据缓存键、状态表和配置 hash 判断是否复用已有结果。

## 核心模块

模块组织详见 `project-structure.md`。以下描述各模块职责。

MVP 使用 workspace + 2 crate：`daily-paper`（bin）和 `daily-paper-core`（lib）。各阶段实现作为 core 内部模块，不拆独立 crate。

### 1. 配置模块

职责：

- 加载配置文件（TOML）。
- 展开 `api_key_env` 等环境变量字段为真实值。
- 校验必填字段。
- 生成脱敏配置快照和配置 hash。

配置分两层：

```rust
/// TOML 直接反序列化，secret 字段是 xxx_env: String
struct RawConfig { /* ... */ }

/// 运行时使用，含真实 key，不序列化到磁盘
struct ResolvedConfig { /* ... */ }
```

脱敏快照只序列化 `RawConfig`，`config_hash` 对脱敏后的 `RawConfig` 计算。

示例：

```toml
[zotero]
user_id = "123456"
api_key_env = "ZOTERO_API_KEY"

[[zotero.filters]]
path = "2026/survey/**"
weight = 2.0

[[zotero.filters]]
path = "archive/**"
weight = 0.2

[[sources]]
kind = "arxiv"
categories = ["cs.AI", "cs.CL", "cs.LG"]
include_cross_list = false

[reranker]
kind = "embedding_similarity"
embedder = "openai-compatible"
model = "text-embedding-3-small"

[reader]
kind = "openai-compatible"
model = "gpt-4o-mini"
top_n = 10
language = "zh-CN"
require_full_text = true

[email]
smtp_server = "smtp.example.com"
smtp_port = 465
sender = "sender@example.com"
receiver = "receiver@example.com"
password_env = "SMTP_PASSWORD"
```

### 2. Zotero 同步模块

职责：

- 从 Zotero API 拉取用户文献库。
- 默认使用全部文献。
- 支持 collection path filter，用于调整权重或排除范围。
- 将 Zotero item 转成内部 `LibraryPaper`。
- 使用 Zotero library version 做增量同步。
- 失败时保留上一次成功快照。

关键状态：

- `zotero_library_version`
- `zotero_items`
- `zotero_collections`
- `zotero_snapshot`

策略：

- 首次运行全量拉取。
- 后续运行优先使用 `If-Modified-Since-Version` 或 `since` 风格的增量同步。
- 如果远端未变化，直接使用本地 snapshot。
- 如果 Zotero 请求失败，但本地存在未过期 snapshot，可以降级使用本地数据并标记 warning。
- 对 429 或临时网络错误执行指数退避。

### 2.1 兴趣画像模块

职责：

- 从 Zotero 文献生成用户兴趣画像。
- 默认纳入全部 Zotero 文献。
- 根据 collection path、tag、时间等规则计算每篇 Zotero 文献的权重。

第一版重点支持 collection path 权重：

```toml
[[zotero.filters]]
path = "active-projects/**"
weight = 2.0

[[zotero.filters]]
path = "old-reading/**"
weight = 0.2

[[zotero.filters]]
path = "ignored/**"
exclude = true
```

规则：

- 未命中任何 filter 的文献仍然保留，默认权重为 `1.0`。
- `exclude = true` 的文献不进入兴趣画像。
- 如果一篇文献命中多个 filter，第一版可以采用最高权重；后续再考虑更复杂的合并策略。

### 3. 论文源模块

职责：

- 从不同论文源获取候选论文。
- 将不同源的数据归一化为内部 `CandidatePaper`。
- 保证 source 内部去重。

抽象接口（MVP 使用 enum dispatch，不用 dyn trait）：

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
```

第一版 source：

- `arxiv`

后续 source：

- `biorxiv`
- `medrxiv`
- `semantic_scholar`
- `openreview`
- `rss`

### 4. 去重模块

职责：

- 合并不同 source 的候选论文。
- 避免已经在 Zotero 中存在的论文再次推荐。
- 避免同一论文在不同 source 或不同分类中重复出现。

去重键优先级：

1. DOI
2. arXiv ID 或 source-native ID
3. 规范化标题

输出：

- `candidate_papers`
- `duplicate_links`
- `skipped_existing_library_items`

### 5. Embedding 模块

职责：

- 为 Zotero 论文和候选论文生成 embedding。
- 缓存 embedding。
- 支持本地模型或 API 模型。

缓存键由 `state-store.md` 统一定义，必须包含 provider、model、embedding config 和规范化输入文本。

```text
input_hash = hash(provider_id + model_id + embedding_config_hash + input_text_normalized)
```

输入文本建议：

```text
title + "\n\n" + abstract
```

失败策略：

- 单条 embedding 请求失败先按 retry policy 重试。
- 重试后仍失败时，MVP 默认使 run failed，避免在缺失向量的情况下产生不可解释排序。
- API 批量请求失败后可以拆小 batch 重试。
- 已有 embedding 不重复计算。

### 6. Rerank 模块

职责：

- 基于 Zotero 兴趣上下文对候选论文排序。
- 产出每篇论文的相关性分数和排序解释信息。
- 根据配置选择 Top N 进入精读阶段。

第一版算法：

- 对候选论文 embedding 与 Zotero item embedding 计算余弦相似度。
- Zotero item 使用兴趣画像模块产出的权重。
- 候选论文分数为加权相似度聚合结果。

可能的聚合方式：

```text
score(candidate) =
  weighted_average(top_k_similarity(candidate, zotero_items))
```

rerank 缓存键：

```text
hash(
  interest_profile_id
  + candidate_paper_ids
  + embedding_model_id
  + reranker_config
)
```

输出：

- `ranked_papers`
- `score`
- `matched_library_items`
- `rerank_cache_key`

Top N 选择不属于 rerank 缓存本身，而是由 `rerank_cache_key + top_n` 派生。这样修改 `top_n` 时不需要重算完整排序，只需要补读新增进入 Top N 的论文。

### 7. 精读模块

职责：

- 对 Top N 论文生成 LLM 精读结果。
- 每篇论文是独立任务。
- 失败可以局部重试。
- 第一版要求读取全文，而不是只读 abstract。
- 必须下载 PDF 并完整提取正文；LLM 输入可以在完整正文基础上做 section selection 或 chunk 汇总，但不能只依赖 abstract。
- 抓取或推断补充 metadata，例如研究机构、配置名单中的知名作者、项目主页、代码链接等。
- 输出一段紧凑的关键信息总结，而不是生成很长的结构化读书笔记。

单篇论文状态：

```text
pending
  -> pdf_fetched
  -> text_extracted
  -> metadata_fetched
  -> llm_done
```

失败状态：

```text
retryable_failed
permanent_failed
```

精读缓存键由 `state-store.md` 统一定义，必须包含 PDF 内容、正文提取 key、metadata key、reader template、LLM model 和语言。

```text
read_cache_key = hash(
  paper_id
  + pdf_sha256
  + text_extract_key
  + metadata_key
  + reader_template_hash
  + llm_model_id
  + language
)
```

建议内部输出结构：

```text
summary
metadata
relevance
links
```

其中 `summary` 是最终报告中展示的一段文字。`metadata` 可包含：

```text
institutions
notable_authors
project_url
code_url
paper_url
pdf_url
```

失败策略：

- PDF 下载失败：标记该论文精读失败，不发送最终报告，等待重试。
- PDF 解析失败：标记该论文精读失败，不发送最终报告，等待重试。
- LLM 请求失败：标记单篇失败，不影响其他论文。
- Top N 中任意论文未完成精读时，默认不发送报告。
- 可以后续增加 `allow_partial_report = true`，但第一版不启用。

### 7.1 Metadata 抓取模块

职责：

- 从论文源、PDF 首页、arXiv 页面和论文正文 URL 中提取补充信息。
- 尽量识别研究机构、配置名单中的知名作者、项目链接和代码链接。
- 为精读 prompt 提供上下文。

第一版优先级：

1. 论文源自带 metadata。
2. PDF 首页文本。
3. arXiv abstract page 中的 comments、journal reference、links。
4. 论文正文中的 GitHub、project page、homepage 链接。

注意：

- 该模块不追求完美实体消歧。
- “学术大牛”识别第一版只做轻量启发式，例如作者是否出现在用户配置的名单中，或是否命中可维护的知名作者列表。
- MVP 不主动爬取作者主页或外部个人主页；只提取论文页面、PDF 和正文中已经出现的信息。
- 如果 metadata 抓取失败，不应阻塞精读；只在总结中省略相关信息。

### 8. 渲染模块

职责：

- 将 rerank 结果和精读结果渲染成 HTML。
- 支持纯文本摘要版本，便于某些发送渠道使用。
- 模板与业务逻辑分离。

输入：

- run metadata
- ranked papers
- deep read results
- warnings
- source statistics

输出：

- HTML
- text fallback

### 9. 发送模块

职责：

- 将报告发送到目标。
- 第一版实现 email sink。
- 发送结果持久化，避免默认重复发送。
- 只有 Top N 论文全部完成精读后才发送报告。

抽象接口（MVP 使用 enum dispatch）：

```rust
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

第一版 sink：

- SMTP email

后续 sink：

- local file
- webhook
- Telegram
- Slack
- Notion

发送幂等：

- 对 report 内容生成 `report_hash`，不包含 `run_id`。
- 对发送目标生成 `delivery_key = hash(report_hash + sink_id + sink_config_hash + recipient)`。
- 发送成功后记录 `delivery_receipt`。
- 如果同一 `delivery_key` 已发送，默认跳过。
- 提供 `--force-send` 手动重复发送。

## 存储设计

存储实现位于 `daily-paper-core/src/state/`。详细设计见 `state-store.md`。

第一版使用文件系统存储，不引入数据库。

默认目录结构：

```text
<state-root>/
  state/
    runs/
      2026-05-27.json
    zotero/
      sync-state.json
      snapshots/
        <snapshot-id>.json
  cache/
    sources/
      arxiv/
        <source-fetch-key>.json
        latest-by-window/
          2026-05-27.json
    embeddings/
      <model-id>/
        <input-hash>.json
    rerank/
      <rerank-cache-key>.json
    selections/
      <selection-id>.json
    papers/
      <paper-id>/
        metadata.json
        paper.pdf
        extracted.txt
        read-result.json
  reports/
    <report-hash>.html
    <report-hash>.txt
  deliveries/
    <delivery-key>.latest.json
```

默认根目录按平台解析：

```text
Linux:   ~/.local/share/daily-paper/
macOS:   ~/Library/Application Support/daily-paper/
Windows: %APPDATA%/daily-paper/
```

配置文件默认按平台解析：

```text
Linux:   ~/.config/daily-paper/config.toml
macOS:   ~/Library/Application Support/daily-paper/config.toml
Windows: %APPDATA%/daily-paper/config.toml
```

这里的 `<state-root>` 默认就是上述平台 app data 目录。用户可以通过配置或 CLI 显式覆盖 state 目录。项目内 `.daily-paper/` 只作为显式项目模式使用，不作为安装后默认路径。

写入策略：

- 所有状态文件先写临时文件，再原子 rename。
- 大文件按内容或稳定 ID 存储，避免重复下载。
- manifest 文件只记录路径、hash、状态和错误摘要。
- 不依赖文件修改时间判断缓存有效性，优先使用显式 hash 和远端 version。

### 可选 SQLite Backend

如果后续需要更强的查询能力，可以保留 SQLite backend 作为第二阶段实现。

主要表：

```text
runs
zotero_sync_state
zotero_items
zotero_snapshots
candidate_papers
paper_sources
embeddings
rerank_results
deep_read_tasks
deep_read_results
rendered_reports
deliveries
```

### runs

记录每次执行：

```text
id
started_at
finished_at
status
config_hash
window_start
window_end
error_summary
```

### embeddings

记录 embedding 缓存：

```text
id
model_id
input_hash
input_text_preview
vector
created_at
```

第一版可以把 vector 存成 binary blob 或 JSON。性能不是第一优先级。

### deep_read_tasks

记录单篇论文精读状态：

```text
id
run_id
paper_id
status
attempt_count
last_error
cache_key
updated_at
```

## 错误分类

错误应显式分类，避免所有失败都变成一个不可恢复异常。

错误类型定义在 `daily-paper-core/src/error.rs`，使用 `thiserror`。bin crate 用 `anyhow::Context` 附加运行时上下文。详见 `project-structure.md` 错误策略章节。

建议类别：

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

是否可重试应由错误类别和上下文决定：

```text
Auth                 不自动重试
Config               不自动重试
RateLimited          按 retry-after 或指数退避
RetryableNetwork     指数退避
BadSourceData        通常不重试
PdfExtract           单篇论文级别失败，阻塞最终发送，等待重试
Llm                  单篇论文级别重试，失败时阻塞最终发送
Delivery             报告级别重试
```

## 恢复策略

再次运行时按以下顺序判断：

1. 配置是否变化。
2. Zotero snapshot 是否可复用。
3. 候选论文是否已抓取。
4. embedding 是否已存在。
5. rerank 输入是否与缓存匹配。
6. Top N 论文是否已有精读结果。
7. report 是否已渲染。
8. report 是否已发送。

关键原则：

- 网络请求之前先查缓存。
- 昂贵请求之后立即写入 state store。
- 单篇论文失败不影响整次 run 的其他论文。
- Top N 中任意论文精读失败时，默认不发送报告。
- 再次运行时只重试失败的单篇论文任务，而不是从头开始。

## CLI 设计

长期可考虑的 debug 命令：

```text
daily-paper sync-zotero
daily-paper fetch-source
daily-paper rerank
daily-paper read
daily-paper render
daily-paper send
```

MVP 只实现 `run`、`status`、`retry`。

### run

执行完整流程。

常用参数：

```text
--config <path>
--state-dir <path>
--date <yyyy-mm-dd>
--window yesterday|today|last-n-days
--force-zotero-sync
--force-rerank
--force-read
--force-send
--dry-run
```

### status

查看最近一次运行的阶段状态、失败任务、缓存命中率。

### retry

重试失败任务。

例如：

```text
daily-paper retry --run-id 123
daily-paper retry --run-id 123 --paper-id arxiv:2605.12345
```

## 第一版实现范围

建议 MVP 包含：

- TOML 配置（`dp-config`）。
- 文件系统 StateStore（`dp-state`）。
- Zotero 增量同步和本地 snapshot（`dp-zotero`）。
- arXiv source（`dp-source`）。
- OpenAI-compatible embedding client（`dp-embedding`）。
- embedding similarity reranker（`dp-rerank`）。
- OpenAI-compatible LLM reader（`dp-reader`）。
- PDF 下载和正文提取（`dp-pdf`）。
- 轻量 metadata 抓取（`dp-metadata`，MVP 仅从 arXiv 响应提取）。
- HTML renderer（`dp-render`）。
- SMTP email sink（`dp-deliver`）。
- CLI `run`、`status`（`retry` 推迟到 M7 之后）。

暂缓：

- Web UI。
- 多用户部署。
- 复杂插件系统。
- 写回 Zotero。
- 多种 source 和 sink 的完整实现。
- 本地 embedding 模型下载和管理。
- SQLite backend。

## 已确定决策

1. Zotero 兴趣画像默认使用全部文献。
2. collection path filter 主要用于调整权重或排除范围。
3. 精读必须读取全文，不能只依赖 abstract。
4. 每篇 Top N 论文最终输出一段关键信息总结。
5. 精读阶段需要尽量抓取 metadata，包括研究机构、作者背景、项目页和代码链接。
6. Top N 做成配置参数。
7. Top N 论文都必须精读完成后才发送报告。
8. 第一版默认不用数据库，先使用文件系统 StateStore。
9. 第一版不做人工反馈入口。
10. MVP 使用 workspace + 2 crate（`daily-paper` + `daily-paper-core`），不过早拆分。
11. 错误策略：core 用 `thiserror`，bin 用 `anyhow`。
12. MVP 使用 enum dispatch，不用 dyn trait object。
13. 选 chrono 而非 time。
14. reqwest 用 rustls-tls，不用 native-tls。
15. lettre 用同步 SmtpTransport + spawn_blocking。
16. Lock stale 检测只检查文件 age，不依赖 sysinfo。
17. Embedding 向量单独存储为二进制文件，manifest 只记录路径和维度。
18. `retry` 命令推迟到 M7 之后。

## 术语说明

### reader template hash

`reader_template_hash` 指精读提示词模板的内容 hash。它不是产品功能，而是缓存失效机制。

如果精读 prompt 改了，同一篇论文用新 prompt 生成的结果可能不同，因此旧的 `read-result.json` 不应该被继续复用。第一版不需要用户手动维护“prompt 版本号”，直接对模板文件内容计算 hash 即可。

## 并发控制

各模块内部使用 `tokio::sync::Semaphore` 控制并发，配置通过参数传入：

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

## 后续需要继续细化

1. Zotero 和 arXiv API 的具体请求、分页、限流和日期语义。
2. OpenAI-compatible provider 的请求路径、限流、重试和并发策略。
3. PDF section 识别和全文 chunk 汇总策略。
4. Token 裁剪策略：使用 `tiktoken-rs` 或字符/token 比率估算，将正文裁剪到 `max_input_tokens`。
5. 第二版 crate 拆分时机：当需要独立发布模块或引入多 backend 时，从 `daily-paper-core` 中抽取子 crate。
