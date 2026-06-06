# 数据模型

核心数据结构定义在 `daily-paper-core/src/models/`。本模块只包含纯数据类型，不包含业务逻辑。

## 设计原则

- 内部模型不直接暴露外部 API 的原始结构
- 所有可缓存实体都有稳定 ID 或 cache key
- 大文本和大文件不塞进 manifest，只记录路径和 hash
- 精读前后的结构分开
- JSON 存储，字段清晰可读

## 核心实体

### Run 模型（`models/run.rs`）

- `RunManifest`：一次执行的完整记录（run_id、状态、config_hash、时间窗口、各阶段记录）
- `StageName`：pipeline 阶段枚举（ZoteroSync → SourceFetch → ... → Send）
- `PipelineEvent`：实时事件，用于 SSE 推送（Started/StageStart/StageEnd/Progress/Ended）
- `ErrorRecord` / `WarningRecord`：错误和警告的结构化记录

### Zotero 模型（`models/zotero.rs`）

- `LibraryPaper`：Zotero 文献的内部表示，包含 collection path、tag、attachment 等
- `ZoteroSnapshot`：一次可复用的文献快照

### 论文模型（`models/candidate.rs`）

- `CandidatePaper`：不同 source 归一化后的候选论文
- `paper_id` 规则：`doi:<doi>` > `arxiv:<id>` > `source:<kind>:<id>`
- DOI 和 arXiv ID 各有规范化函数（strip 前缀、版本后缀、大小写）

### 兴趣画像（`models/interest.rs`）

- `InterestProfile`：从 Zotero 文献生成，包含每篇文献的权重和匹配规则
- 权重由 collection path filter 决定，`exclude = true` 的文献不进入画像

### 去重（`models/dedup.rs`）

- `DedupResult`：候选论文 + 去重记录 + 已有库匹配

### PDF（`models/pdf.rs`）

- `ExtractedText`：提取结果，包含章节列表和文本 hash
- `PaperSection`：章节标题 + 字节偏移

### 精读（`models/read.rs`）

- `ReadResult`：LLM 生成的总结 + metadata + token 使用量
- `TokenUsage`：输入/输出/总 token 数

### 报告（`models/report.rs`）

- `RenderedReport`：渲染结果，report_hash 是内容 hash（不含 run_id），同一内容跨 run 不会重复发送

### 通用类型（`models/common.rs`）

- `DateWindow`：时间窗口
- `Author`：作者（name、affiliation、url）
- `sha256_hex`：SHA-256 十六进制摘要

## 缓存键设计

所有 hash 输入先规范化（稳定字段顺序、列表排序、trim、DOI/arXiv ID 规范化）。详见 `state-store.md` 的缓存键章节。

关键缓存键：

- **embedding**：`sha256(provider + model + config + input_text)`
- **rerank**：`sha256(profile_id + candidate_set + model + reranker_config)`
- **read**：`sha256(paper_id + pdf_sha256 + extract_key + metadata_key + template_hash + model + language)`
- **delivery**：`sha256(report_hash + sink_id + sink_config + recipient)`

## 缓存失效

| 变化 | 需要重算 |
|------|----------|
| Zotero snapshot / filter | InterestProfile、Rerank |
| embedding model | Embedding、Rerank |
| rerank 参数 | Rerank、Top-N selection |
| `top_n` 变大 | 新增论文需补精读 |
| reader template / LLM model | ReadResult、Report |
| PDF 内容 | ExtractedText、ReadResult |
