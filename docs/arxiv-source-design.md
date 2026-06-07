# arXiv Source 设计

## 背景

`daily-paper` 需要两类不同的 arXiv 获取能力：

- **每日订阅**：在固定时间获取最近一次 arXiv announcement 中的新论文。
- **历史回填**：对指定日期补抓候选论文，结果应与该日期窗口真实对应。

旧实现只使用 `https://rss.arxiv.org/rss/{category}`。RSS feed 适合每日订阅，但不是历史查询接口。代码里虽然传入 `DateWindow`，实际只能在 RSS 当前返回的 item 中按 `pubDate` 做本地过滤。也就是说，`--date` 对历史日期没有真实抓取能力；没有缓存时，返回空结果并不代表那天没有论文。

当前实现已经拆分为显式 backend：`rss` 和 `export`。配置必须写明 backend。

## 设计目标

- 默认日常运行保持轻量、稳定、低请求量。
- 历史日期必须语义诚实：RSS 不支持真实历史查询时写入 warning，不把空结果解释为“当天无论文”。
- 所有 arXiv 网络访问统一限速，避免不同 stage、category、分页、重试绕过限制。
- source 结果进入统一 `CandidatePaper` 模型，后续 dedup、embedding、rerank、report 不感知 backend 差异。
- 支持未来扩展到更适合 metadata harvesting 的接口，而不重写 pipeline。

## 非目标

- 不把 export search API 作为默认日常 backend。
- 不在 source 阶段下载 PDF。
- 不保证历史回填与 arXiv Web UI 的人类浏览列表逐项完全一致；目标是基于公开 metadata 的可解释候选集。
- 不为绕过 arXiv 限制做并发、多 IP 或批量冲刺。

## 接口选择

### RSS

RSS endpoint：

```text
https://rss.arxiv.org/rss/{category}
```

优点：

- 请求小，适合每天按 category 订阅。
- 返回内容天然接近 “updates/new papers” 使用场景。
- 当前代码已经能解析 `announce_type`、category、author、abstract。

限制：

- 只能拿 feed 当前暴露的更新项。
- 没有日期查询参数。
- `DateWindow` 只能做本地过滤，不能驱动远端返回历史数据。

RSS 的语义应命名为 `DailyUpdates`，而不是 `DateSearch`。

### export API

export search endpoint：

```text
https://export.arxiv.org/api/query
```

典型查询：

```text
search_query=(cat:cs.AI OR cat:cs.LG) AND submittedDate:[202606060000 TO 202606070000]
sortBy=submittedDate
sortOrder=ascending
start=0
max_results=2000
```

优点：

- 可以表达日期窗口。
- 可以组合 category、关键词、作者等条件。
- 返回 Atom XML，字段能映射到现有 `CandidatePaper`。

风险：

- 官方要求 legacy API 总体不超过每 3 秒 1 个请求，并使用单连接。
- category 分开查询、分页、重试、历史多日回填都会放大请求数。
- 宽查询或大结果集会慢，且官方建议超过 1000 结果时细化查询；bulk metadata 更适合 OAI-PMH。

export API 只适合作为显式、限速、可中断的历史查询 backend，不适合作为默认日常 RSS 替代品。

### OAI-PMH

OAI-PMH 是面向 metadata harvesting 的接口，适合历史回填和批量同步。它通常比 search API 更符合“按日期抓 metadata，再本地过滤 category”的使用方式。

优点：

- 设计目标更接近 metadata harvesting。
- 支持按日期窗口和 resumption token 分页。
- 更适合构建可恢复的本地历史索引。

代价：

- 实现复杂度高于 RSS 和 export API。
- 需要处理 set、分页 token、断点续跑、增量状态。

长期方向应优先考虑 OAI-PMH，而不是把 export API 扩展成批量抓取工具。

## 推荐架构

```text
ArxivSource
  ├── RssDailyBackend
  ├── ExportSearchBackend
  └── OaiPmhBackend (planned)

ArxivRateLimiter
ArxivCache
ArxivAtomParser
CandidatePaper converter
```

### Backend 能力模型

每个 backend 显式声明能力：

```text
supports_daily_updates: bool
supports_date_window: bool
supports_backfill: bool
recommended_for_default_run: bool
```

调度规则：

| 场景 | 默认 backend | 行为 |
|------|--------------|------|
| 未传 `--date` | RSS | 获取最近更新 |
| `--date` 是本地今天 | RSS | 允许，但仍按 RSS 当前 item 过滤 |
| `--date` 是历史日期，无缓存 | RSS | 返回空候选并写 warning，提示 RSS 不支持历史查询 |
| `--date` 是历史日期，有缓存 | cache | 直接复用 |
| `backend = "export"` | export | 按日期窗口真实查询 |

这样 `--date` 不再暗示所有 source 都支持历史抓取。日期同时承担两个角色：

- archive/cache 的逻辑日期；
- 对支持 date-window 的 backend，作为远端查询条件。

## 配置设计

backend 是必填字段。日常运行使用 RSS：

```toml
[[sources]]
kind = "arxiv"
categories = ["cs.AI", "cs.LG"]
include_cross_list = false
backend = "rss"
```

历史回填可显式使用 export API：

```toml
[[sources]]
kind = "arxiv"
categories = ["cs.AI", "cs.LG"]
backend = "export"
max_results_per_page = 1000
max_pages = 3
```

当前实现使用进程内全局限速：最多 1 个 in-flight，请求间隔至少 3 秒。限速暂不暴露为配置。

## export API 请求策略

如果实现 export backend，应遵守以下策略：

1. 多个 category 合并成一个 OR 查询，避免每个 category 一次请求。
2. 查询必须包含日期窗口，禁止无界宽查询。
3. `max_results` 默认不超过 1000；需要更多结果时分页。
4. 每次分页、重试、不同 source 请求都经过同一个 `ArxivRateLimiter`。
5. 遇到 429/503 时尊重 `Retry-After`，否则使用保守退避。
6. 设置 `max_pages`，超过后失败并提示用户收窄 category 或改用 OAI-PMH。

示例：

```text
((cat:cs.AI OR cat:cs.LG) AND submittedDate:[202606060000 TO 202606070000])
```

分页：

```text
start=0&max_results=1000
start=1000&max_results=1000
start=2000&max_results=1000
```

每个请求之间至少间隔 3 秒。

## 缓存与幂等

source cache key 必须包含：

- backend 名称和 backend 版本；
- date window label、start、end；
- categories 排序后的列表；
- `include_cross_list`；
- 查询参数版本，例如 page size、announce type 策略；
- parser/converter 版本。

建议 cache key 前缀：

```text
arxiv:rss-daily:v2
arxiv:export-date:v1
arxiv:oai-date:v1
```

历史回填要记录分页状态：

```text
cache/arxiv/backfill/{date}/{backend}/state.json
cache/arxiv/backfill/{date}/{backend}/page-{n}.xml
archive/{date}/candidates.json
```

这样中断后可以从已完成页面继续，不需要重复请求。

## 错误语义

### RSS + 历史日期

当用户指定历史日期且无缓存时，RSS backend 写入 manifest warning：

```text
arXiv RSS does not support historical date queries.
Requested date: 2026-05-01.
Use a backfill-capable backend such as export, or rerun with an existing archive cache.
```

pipeline 仍允许空候选并提前成功结束，避免日常自动任务被历史日期误用阻断。

### export 分页超限

当结果超过 `max_pages`：

```text
arXiv export query exceeded configured page limit.
Narrow categories, reduce the date window, or use OAI-PMH backfill.
```

### rate limit

所有 429/503 进入 `RateLimited` 或 `RetryableNetwork`，并写入 manifest warning/error。不能在 backend 内部无限重试。

## 后续计划

1. 为 export backfill 增加断点续跑状态，保存每页原始 XML。
2. 实现 `OaiPmhBackend`，作为推荐历史回填方案。
3. 评估是否把 arXiv 限速参数暴露为配置。

## 设计原则

RSS 是 daily feed，不是历史搜索。export API 是 search，不是 bulk harvester。OAI-PMH 是 metadata harvesting，不是 daily UX。把三者的职责分清，pipeline 才能保持简单、稳定、可解释。

`DateWindow` 不应该被 source 盲目接受。它只有在 backend 声明支持 date-window query 时，才是远端查询条件；否则只是 archive/cache label。这个边界是整个设计的核心。

## 参考

- arXiv API User's Manual: https://info.arxiv.org/help/api/user-manual.html
- Terms of Use for arXiv APIs: https://info.arxiv.org/help/api/tou.html
