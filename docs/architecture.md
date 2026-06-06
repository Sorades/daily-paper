# Daily Paper 架构

daily-paper 是一个 Rust CLI，用于每日自动获取、筛选、精读论文并发送报告。

## 流程

```text
config → zotero_sync → source_fetch → deduplicate → embedding → rerank → deep_read → render → send
```

每次运行生成一个 run 记录。各阶段写入自己的状态和产物。后续运行根据缓存键、状态表和配置 hash 判断是否复用已有结果。

详见 `project-structure.md`（crate 组织）、`data-model.md`（数据结构）、`state-store.md`（存储设计）。

## 设计目标

- 每个阶段可独立重试，失败后再次运行不重复已完成工作
- Zotero 增量同步 + 本地缓存
- 论文源可扩展（arXiv 是第一种）
- reranker、reader、renderer、sink 可替换
- 保留足够中间状态便于调试和审计

## 非目标

- Web UI / 多用户服务（有基础 SSE 推送，但不做完整 Web 应用）
- 写回 Zotero
- 复杂推荐算法
- SQLite backend

## 各模块职责

### 配置

加载 TOML，校验必填字段，填充默认值。分两层：`RawConfig`（直接反序列化）和 `ResolvedConfig`（运行时使用，含默认值）。

### Zotero 同步

从 Zotero API 增量拉取文献库。使用 library version 做增量同步。支持 collection path filter 调整权重或排除范围。失败时可降级使用本地 snapshot。

### 论文源

从 arXiv RSS 获取候选论文，归一化为内部 `CandidatePaper`。支持 category 订阅和 cross-list 配置。

### 去重

合并不同 source 的候选论文，排除已在 Zotero 中的论文。去重键：DOI > arXiv ID > source ID。

### Embedding

为 Zotero 文献和候选论文生成向量。支持 OpenAI-compatible API 和 fastembed 本地模型。输入固定为 `title + "\n\n" + abstract`。结果按 input_hash 缓存。

### Rerank

基于余弦相似度对候选论文排序。Zotero 文献使用兴趣画像权重，加权 top-k 聚合。Top-N 选择独立缓存，修改 `top_n` 不需要重算 rerank。

### 精读

对 Top-N 论文生成 LLM 总结。流程：PDF 下载 → 正文提取 → 章节选择 → metadata 抓取 → LLM 生成。每篇论文独立，失败可重试（默认策略：retry，最多 3 次，仅重试瞬态错误）。必须读取全文，不能只依赖 abstract。

### 渲染

将排序结果和精读结果渲染为 HTML 报告。同时写入 cache（版本化）和 archive（每日快照）。

### 发送

通过 SMTP 发送报告。同一 delivery_key 默认只发送一次（幂等）。

## 错误分类

| 类型 | 恢复动作 |
|------|----------|
| Config / Auth / Storage | 不重试，run failed |
| RateLimited / RetryableNetwork | 指数退避重试 |
| PdfDownload / PdfExtract / Llm | 单篇论文级别，阻塞发送 |
| SourceUnavailable | run failed |
| BadSourceData | 跳过并警告 |
| Delivery | 报告级别重试 |

## 恢复策略

再次运行时按阶段顺序判断缓存：配置 hash → Zotero snapshot → 候选论文 → embedding → rerank → 精读 → 报告 → 发送。网络请求前先查缓存，昂贵请求后立即写入。

## 并发与可观测性

- 各模块用 `Semaphore` 控制并发
- tracing：每个 run 一个 root span，每个 stage 一个 child span
- PipelineEvent 通过 broadcast channel 推送 SSE（StageStart/StageEnd/Progress/Ended）
- CLI 使用 indicatif 显示进度条和 ETA
