# 数据模型草案

本文档定义第一版实现需要的核心数据结构。字段名倾向于直接映射到 Rust `struct`，同时也能序列化为 JSON 存入文件系统 StateStore。

## 设计原则

- 内部模型不直接暴露 Zotero、arXiv、邮件服务等外部 API 的原始结构。
- 所有可缓存实体都必须有稳定 ID 或 cache key。
- 大文本和大文件不直接塞进 run manifest，只记录路径和 hash。
- 精读前后的结构分开，避免把抓取、解析、LLM 输出混成一个状态。
- 第一版使用 JSON 存储，因此字段需要保持清晰、可读、便于手工检查。

## 基础类型

### DateWindow

表示本次抓取候选论文的时间窗口。

```rust
struct DateWindow {
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    label: String,
}
```

示例：

```json
{
  "start": "2026-05-26T00:00:00Z",
  "end": "2026-05-27T00:00:00Z",
  "label": "2026-05-26"
}
```

### ContentHash

用于缓存失效。

```rust
struct ContentHash {
    algorithm: String,
    value: String,
}
```

第一版统一使用 `sha256`。

## Run

`Run` 表示一次执行。即使某些阶段复用了缓存，也应该记录在本次 run 里。

```rust
struct RunManifest {
    run_id: String,
    parent_run_id: Option<String>,
    status: RunStatus,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    config_hash: String,
    resolved_config: ResolvedConfigSnapshot,
    cli_overrides: Vec<CliOverride>,
    date_window: DateWindow,
    stages: Vec<StageRecord>,
    warnings: Vec<WarningRecord>,
    error: Option<ErrorRecord>,
}
```

`retry --run-id` 不直接覆盖原 run，而是创建一个新的 run，并通过 `parent_run_id` 指向原 run。retry run 复用原 run 的脱敏 resolved config、时间窗口和 Top N selection。

```rust
struct ResolvedConfigSnapshot {
    base_hash: String,
    env_overrides: Vec<EnvOverride>,
    cli_overrides: Vec<CliOverride>,
    effective_hash: String,
    zotero: serde_json::Value,
    sources: serde_json::Value,
    embedding: serde_json::Value,
    reranker: serde_json::Value,
    reader: serde_json::Value,
    pdf: serde_json::Value,
    email: serde_json::Value,
}

struct EnvOverride {
    var_name: String,
    affected_field: String,
}
```

`ResolvedConfigSnapshot` 是脱敏后的配置快照，只保存 env var 名称，不保存密钥值。retry 时如果用户的环境变量值变了但名称不变，`effective_hash` 不变，这是期望行为。

```rust
struct CliOverride {
    key: String,
    value: String,
}
```

```rust
enum RunStatus {
    Running,
    Succeeded,
    Failed,
    Blocked,
    Cancelled,
}
```

`Blocked` 表示流程没有崩溃，但缺少完成条件。例如 Top N 中某篇论文 PDF 解析失败，导致报告不能发送。

`Cancelled` 表示用户中断（如 Ctrl+C），部分阶段已完成。

```rust
enum StageStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}
```

### StageRecord

```rust
struct StageRecord {
    stage: StageName,
    status: StageStatus,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    cache_hit: bool,
    input_hash: Option<String>,
    output_ref: Option<String>,
    error: Option<ErrorRecord>,
}
```

```rust
enum StageName {
    ZoteroSync,
    SourceFetch,
    Deduplicate,
    Embedding,
    Rerank,
    PdfFetch,
    TextExtract,
    MetadataFetch,
    DeepRead,
    Render,
    Send,
}
```

## Zotero 数据

### ZoteroSyncState

保存 Zotero 增量同步状态。

```rust
struct ZoteroSyncState {
    user_id: String,
    library_version: Option<u64>,
    last_success_at: Option<DateTime<Utc>>,
    last_snapshot_id: Option<String>,
    last_error: Option<ErrorRecord>,
}
```

### ZoteroSnapshot

一次可复用的 Zotero 文献快照。

```rust
struct ZoteroSnapshot {
    snapshot_id: String,
    user_id: String,
    library_version: Option<u64>,
    created_at: DateTime<Utc>,
    item_count: usize,
    items: Vec<LibraryPaper>,
}
```

`snapshot_id` 建议为：

```text
sha256(user_id + library_version + normalized_item_ids_and_versions)
```

### LibraryPaper

内部 Zotero 文献模型。

```rust
struct LibraryPaper {
    library_id: String,
    library_type: LibraryType,
    zotero_key: String,
    version: Option<u64>,
    item_type: String,
    parent_item: Option<String>,
    title: String,
    abstract_text: Option<String>,
    authors: Vec<Author>,
    year: Option<i32>,
    doi: Option<String>,
    arxiv_id: Option<String>,
    url: Option<String>,
    collection_keys: Vec<String>,
    collections: Vec<CollectionPath>,
    tags: Vec<String>,
    is_trashed: bool,
    date_added: Option<DateTime<Utc>>,
    date_modified: Option<DateTime<Utc>>,
    attachments: Vec<ZoteroAttachmentRef>,
}
```

Zotero snapshot 保存原始库视图，不保存 filter 产物。`interest_weight` 和排除规则只属于 `InterestProfile`。

```rust
enum LibraryType {
    User,
    Group,
}
```

```rust
struct ZoteroAttachmentRef {
    key: String,
    title: Option<String>,
    content_type: Option<String>,
    url: Option<String>,
}
```

### CollectionPath

```rust
struct CollectionPath {
    key: String,
    path: String,
}
```

`path` 示例：

```text
2026/survey/agent
active-projects/daily-paper
```

## 兴趣画像

### InterestProfile

```rust
struct InterestProfile {
    profile_id: String,
    zotero_snapshot_id: String,
    created_at: DateTime<Utc>,
    rules_hash: String,
    paper_count: usize,
    total_weight: f32,
    papers: Vec<InterestPaperRef>,
}
```

```rust
struct InterestPaperRef {
    library_id: String,
    weight: f32,
    matched_rules: Vec<String>,
}
```

被 `exclude = true` 命中的文献不进入 `InterestProfile.papers`。

`profile_id` 建议为：

```text
sha256(zotero_snapshot_id + rules_hash)
```

## 候选论文

### CandidatePaper

不同 source 获取的新论文统一成该结构。

```rust
struct CandidatePaper {
    paper_id: String,
    source: PaperSourceKind,
    source_id: String,
    title: String,
    abstract_text: String,
    authors: Vec<Author>,
    published_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
    categories: Vec<String>,
    doi: Option<String>,
    arxiv_id: Option<String>,
    landing_url: Option<String>,
    pdf_url: Option<String>,
    source_metadata: serde_json::Value,
}
```

```rust
enum PaperSourceKind {
    Arxiv,
    BioRxiv,
    MedRxiv,
    Other(String),
}
```

`paper_id` 规则：

1. 有 DOI：`doi:<normalized-doi>`
2. 有 arXiv ID：`arxiv:<normalized-arxiv-id>`
3. 否则：`source:<source-kind>:<source-id>`

标题 hash 只用于兜底去重，不建议作为主 ID。

### PaperIdAlias

一篇论文可能同时有 DOI 和 arXiv ID。Zotero 中记录的可能是 arXiv ID，候选论文源记录的是 DOI。去重阶段维护交叉解析表：

```rust
struct PaperIdAlias {
    canonical_id: String,
    aliases: Vec<String>,
    resolved_at: DateTime<Utc>,
}
```

所有后续引用统一使用 canonical form。

### paper_id 规范化

DOI 规范化：strip `https://doi.org/`、`http://doi.org/`、`DOI:` 前缀，lower-case，trim 尾部 `/`。

arXiv ID 规范化：剥离版本后缀（`2301.12345v3` -> `2301.12345`），在 `source_metadata` 中保留原始带版本的 ID。

### Author

```rust
struct Author {
    name: String,
    normalized_name: Option<String>,
    affiliation: Option<String>,
    url: Option<String>,
}
```

第一版不做复杂作者消歧。

## 去重结果

```rust
struct DedupResult {
    run_id: String,
    input_source_refs: Vec<String>,
    candidates: Vec<CandidatePaper>,
    duplicates: Vec<DuplicateRecord>,
    skipped_existing: Vec<ExistingLibraryMatch>,
}
```

```rust
struct ExistingLibraryMatch {
    candidate_paper_id: String,
    library_id: String,
    reason: DuplicateReason,
}
```

```rust
struct DuplicateRecord {
    kept_paper_id: String,
    dropped_paper_id: String,
    reason: DuplicateReason,
}
```

```rust
enum DuplicateReason {
    SameDoi,
    SameArxivId,
    SameSourceId,
    SameNormalizedTitle,
}
```

## Embedding

### EmbeddingRecord

```rust
struct EmbeddingRecord {
    embedding_id: String,
    provider_id: String,
    model_id: String,
    embedding_config_hash: String,
    input_hash: String,
    input_kind: EmbeddingInputKind,
    input_ref: String,
    vector_path: String,
    vector_dim: usize,
    created_at: DateTime<Utc>,
}
```

向量数据单独存储为二进制文件（`f32` little-endian），manifest 只记录路径和维度。理由：1536 维 float 序列化为 JSON 约 12KB/条，1000 篇文献 + 每日候选论文会导致 JSON 膨胀严重。

```rust
enum EmbeddingInputKind {
    LibraryPaper,
    CandidatePaper,
}
```

`input_hash`：

```text
sha256(provider_id + model_id + embedding_config_hash + normalized_title + normalized_abstract)
```

第一版 embedding 输入固定为：

```text
title + "\n\n" + abstract
```

## Rerank

### RerankResult

```rust
struct RerankResult {
    rerank_cache_key: String,
    produced_by_run_id: String,
    interest_profile_id: String,
    candidate_set_hash: String,
    model_id: String,
    algorithm: String,
    top_k_library_matches: usize,
    ranked: Vec<RankedPaper>,
}
```

`rerank_cache_key` 必须包含 `top_k_library_matches`，否则调整 k 值时旧缓存会误命中。完整 hash 输入见 `state-store.md`。

```rust
struct RankedPaper {
    paper_id: String,
    rank: usize,
    score: f32,
    matched_library_items: Vec<MatchedLibraryItem>,
}
```

```rust
struct MatchedLibraryItem {
    library_id: String,
    title: String,
    similarity: f32,
    weight: f32,
}
```

### ReadSelection

Top N 选择由完整 rerank 结果派生，单独缓存，避免修改 `top_n` 时重算 rerank。

```rust
struct ReadSelection {
    selection_id: String,
    produced_by_run_id: String,
    rerank_cache_key: String,
    top_n: usize,
    selected_paper_ids: Vec<String>,
}
```

`selected_paper_ids` 中的论文必须全部完成精读后才能发送报告。

## Metadata

### PaperMetadata

精读前的补充信息。

```rust
struct PaperMetadata {
    metadata_key: String,
    paper_id: String,
    source_metadata_hash: String,
    text_extract_key: String,
    metadata_extractor_version: String,
    fetched_at: DateTime<Utc>,
    institutions: Vec<String>,
    notable_authors: Vec<NotableAuthor>,
    project_url: Option<String>,
    code_url: Option<String>,
    demo_url: Option<String>,
    homepage_urls: Vec<String>,
    evidence: Vec<MetadataEvidence>,
    warnings: Vec<WarningRecord>,
}
```

```rust
struct NotableAuthor {
    name: String,
    reason: String,
}
```

第一版 `notable_authors` 只基于用户配置名单或静态列表匹配，不自动判断学术地位。

```rust
struct MetadataEvidence {
    kind: MetadataEvidenceKind,
    source: String,
    text: String,
}
```

```rust
enum MetadataEvidenceKind {
    SourceMetadata,
    ArxivPage,
    PdfFirstPage,
    ExtractedText,
    UrlInPaper,
}
```

`text` 只保存短摘录或定位信息，避免把长正文塞进 metadata。

## PDF 与正文

### PdfAsset

```rust
struct PdfAsset {
    paper_id: String,
    url: String,
    file_path: String,
    sha256: String,
    downloaded_at: DateTime<Utc>,
    byte_len: u64,
}
```

### ExtractedText

```rust
struct ExtractedText {
    text_extract_key: String,
    paper_id: String,
    pdf_sha256: String,
    extractor: String,
    extractor_config_hash: String,
    section_parser_version: String,
    extracted_at: DateTime<Utc>,
    text_path: String,
    text_sha256: String,
    sections: Vec<PaperSection>,
    warnings: Vec<WarningRecord>,
}
```

```rust
struct PaperSection {
    title: String,
    normalized_title: String,
    start_byte: usize,
    end_byte: usize,
}
```

第一版 extractor 默认建议为 `pdftotext`。如果环境没有安装，应返回明确错误。

## 精读

### ReadTask

```rust
struct ReadTask {
    run_id: String,
    paper_id: String,
    status: ReadTaskStatus,
    attempt_count: u32,
    max_attempts: u32,
    next_retry_after: Option<DateTime<Utc>>,
    failure_class: Option<ErrorKind>,
    cache_key: String,
    last_error: Option<ErrorRecord>,
    updated_at: DateTime<Utc>,
}
```

```rust
enum ReadTaskStatus {
    Pending,
    PdfFetched,
    TextExtracted,
    MetadataFetched,
    LlmDone,
    RetryableFailed,
    PermanentFailed,
}
```

状态转换补充：

- `RetryableFailed -> Pending`：同 run 内自动重试时（`attempt_count < max_attempts`），回到 `Pending` 重新走完整链路。
- 不同 run 的 retry 通过创建新 `ReadTask` 复用缓存，不修改原 task 状态。

### ReadResult

最终进入报告的数据。

```rust
struct ReadResult {
    paper_id: String,
    cache_key: String,
    generated_at: DateTime<Utc>,
    model_id: String,
    reader_template_hash: String,
    language: String,
    summary: String,
    metadata: PaperMetadataSummary,
    token_usage: Option<TokenUsage>,
    warnings: Vec<WarningRecord>,
}
```

```rust
struct TokenUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    total_tokens: Option<u32>,
}
```

`summary` 是面向最终邮件的一段文字，应该简洁说明：

- 这篇论文做了什么。
- 方法或技术路线是什么。
- 结果或贡献是什么。
- 为什么它可能和用户兴趣相关。
- 如果有重要机构、作者、代码或项目页，也合并进这一段。

### PaperMetadataSummary

```rust
struct PaperMetadataSummary {
    institutions: Vec<String>,
    notable_authors: Vec<String>,
    project_url: Option<String>,
    code_url: Option<String>,
}
```

## 报告与发送

### RenderedReport

```rust
struct RenderedReport {
    report_hash: String,
    report_instance_id: String,
    run_id: String,
    generated_at: DateTime<Utc>,
    title: String,
    html_path: String,
    text_path: Option<String>,
    ranked_paper_ids: Vec<String>,
    read_paper_ids: Vec<String>,
}
```

`report_hash` 是内容 hash，不包含 `run_id`；`report_instance_id` 用于区分不同 run 中生成的报告实例。

### ReportIndex

```rust
struct ReportIndex {
    date: String,
    run_id: String,
    report_path: String,
    html_path: String,
    text_path: Option<String>,
    generated_at: DateTime<Utc>,
    report_hash: String,
}
```

`ReportIndex` 存在 `dates/<date>/report.json`，只作为日期到最新报告 artifact 的索引；完整报告只写入 `reports/<run-id>/`。

约束：

- `read_paper_ids` 必须覆盖 Top N 的 `paper_id`。
- 如果 Top N 中存在未完成精读的论文，不生成最终可发送报告。

### DeliveryReceipt

```rust
struct DeliveryReceipt {
    delivery_key: String,
    report_hash: String,
    report_instance_id: String,
    sink_id: String,
    sink_config_hash: String,
    sent_at: DateTime<Utc>,
    recipient: String,
    provider_message_id: Option<String>,
}
```

同一 `delivery_key` 默认只发送一次。`delivery_key` 由 `report_hash + sink_id + sink_config_hash + recipient` 计算；`report_hash` 仍然只是报告内容 hash。

## 错误与警告

### ErrorRecord

```rust
struct ErrorRecord {
    kind: ErrorKind,
    message: String,
    retryable: bool,
    context: serde_json::Value,
    occurred_at: DateTime<Utc>,
}
```

```rust
enum ErrorKind {
    Config,
    Auth,
    RateLimited,
    RetryableNetwork,
    SourceUnavailable,
    BadSourceData,
    Storage,
    PdfDownload,
    PdfExtract,
    Embedding,
    Llm,
    Render,
    Delivery,
}
```

### WarningRecord

```rust
struct WarningRecord {
    kind: String,
    message: String,
    context: serde_json::Value,
}
```

## 缓存失效规则

| 变化 | 需要重算 |
| --- | --- |
| Zotero snapshot 改变 | InterestProfile、Rerank |
| Zotero filter 改变 | InterestProfile、Rerank |
| 候选论文集合改变 | Candidate embedding 中缺失部分、Rerank |
| embedding model 改变 | Embedding、Rerank |
| rerank 算法参数改变 | Rerank、Top N selection |
| `top_n` 变大 | 新增 Top N 论文需要补精读 |
| `top_n` 变小 | 可复用已精读结果，只改变报告 |
| reader template 改变 | ReadResult、Report |
| LLM model 改变 | ReadResult、Report |
| PDF 内容 hash 改变 | ExtractedText、ReadResult |
| PDF extractor 配置改变 | ExtractedText、ReadResult |
| metadata 抓取逻辑改变 | PaperMetadata、ReadResult |

## 待定细节

- `paper_id` 中 DOI 和 arXiv ID 的规范化函数需要单独实现和测试。
- PDF section 识别第一版可以简单基于标题正则，不追求完美结构化。
- Metadata 抓取逻辑版本进入 `metadata_key`。
