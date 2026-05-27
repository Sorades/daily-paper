# MVP 实现计划

本文档定义第一版要实现的功能边界、默认技术路线和阶段性里程碑。

## MVP 目标

第一版要实现一个可本地运行、可定时运行的 Rust CLI：

```text
daily-paper run
```

默认读取平台 config 目录中的 `config.toml`。项目模式可以显式传入：

```text
daily-paper run --config daily-paper.toml --state-dir ./.daily-paper
```

它应该能：

1. 增量同步 Zotero 文献。
2. 从 arXiv 拉取指定时间窗口的新论文。
3. 用 Zotero 全库兴趣画像对候选论文 rerank。
4. 选择 Top N。
5. 下载 Top N PDF 并提取正文。
6. 抓取轻量 metadata。
7. 调用 OpenAI-compatible LLM 为每篇论文生成一段总结。
8. 渲染 HTML 邮件。
9. 通过 SMTP 发送。
10. 失败后下次运行从中断点恢复。

## MVP 不做

- Web UI。
- 多用户服务。
- 写回 Zotero。
- 本地 embedding 模型管理。
- GROBID 服务集成。
- 自动判断”学术大牛”。
- 人工反馈入口。
- SQLite backend。
- 多 source 的完整实现。
- 多 sink 的完整实现。
- 复杂推荐算法调参界面。
- `retry` 命令（推迟到 M7 之后）。
- PDF 首页文本解析、机构识别、代码链接抓取（M5 metadata 最小化）。

## 默认技术选择

详见 `project-structure.md` 的 Workspace Cargo.toml 章节。

### Rust crate

```text
tokio              async runtime（features = ["full"]）
reqwest            HTTP client（features = ["json", "rustls-tls"]，不用 native-tls）
serde/serde_json   config and state serialization
toml               config file
clap               CLI（derive 模式）
thiserror          core crate 内部错误类型
anyhow             bin crate 边界错误
tracing            logs
tracing-subscriber logs output（features = ["env-filter", "json"]）
chrono             datetime（选 chrono，不用 time；生态兼容更广）
sha2               hashes
lettre             SMTP email（同步 SmtpTransport + spawn_blocking）
minijinja          HTML templates
quick-xml          arXiv Atom parsing
url                URL parse and normalize
regex              section and link extraction
globset            Zotero collection path filters
uuid               run id suffix
tempfile           atomic write
directories        platform config/data directories
```

选型说明：

- **chrono vs time**：选 chrono。lettre、tracing-subscriber 等间接依赖都拉 chrono，避免两套时间类型混用。
- **reqwest**：用 `rustls-tls` 而非 `native-tls`，避免拉入 OpenSSL 编译链。
- **lettre**：用同步 `SmtpTransport` + `spawn_blocking`，不引入 lettre 的 async feature（实验性）。
- **clap**：derive 模式，代码量少、类型安全。

不需要的依赖：

- **sysinfo**：lock stale 检测改为仅检查文件 age，不检查 PID 进程是否存在，省掉这个较重的依赖。
- **data-encoding**：paper-id 转路径使用 sha256 hex 前缀 + slug，不需要 base64。

### PDF 提取

MVP 默认使用外部命令：

```text
pdftotext
```

原因：

- 对学术论文文本提取足够稳定。
- 实现成本低。
- 比一开始集成 GROBID 轻。

要求：

- 启动时检查 `pdftotext` 是否可用。
- 命令默认使用 `pdftotext -enc UTF-8 -layout <pdf> -`。
- 每次提取必须设置 timeout。
- 配置最大 PDF size 和最大输出字符数，避免异常文件拖垮运行。
- 空文本或极短文本视为提取失败。
- section 识别只做 regex heuristic；section offset 使用 UTF-8 byte offset。
- 不可用时给出明确错误。
- PDF 提取失败时该论文精读失败，run 标记为 `Blocked`，不发送报告。

后续可在 `PdfExtractor` trait 下增加其他实现。

### LLM 与 Embedding

MVP 使用 OpenAI-compatible HTTP API。

配置区分：

```toml
[embedding]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "text-embedding-3-small"

[reader]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "gpt-4o-mini"
top_n = 10
language = "zh-CN"
```

Embedding 和 Reader 可以使用同一个 provider，也可以分开配置。

MVP 只承诺支持 OpenAI 格式响应：

- embeddings endpoint: `/embeddings`
- chat endpoint: `/chat/completions`
- auth: `Authorization: Bearer <key>`
- 支持 429/5xx retry。
- 支持 timeout、max retries、max concurrency。
- 如果 provider 返回非 OpenAI 兼容字段，MVP 不做适配。

### StateStore

MVP 使用文件系统 StateStore。安装后的默认目录使用系统 app data 目录：

```text
Linux:   ~/.local/share/daily-paper/
macOS:   ~/Library/Application Support/daily-paper/
Windows: %APPDATA%/daily-paper/
```

默认配置文件位置：

```text
Linux:   ~/.config/daily-paper/config.toml
macOS:   ~/Library/Application Support/daily-paper/config.toml
Windows: %APPDATA%/daily-paper/config.toml
```

项目内 `.daily-paper/` 只在用户显式传入 `--state-dir ./.daily-paper` 或配置 `[state].dir` 时使用。

也就是说，完整目录布局里的 `<state-root>` 在默认安装模式下是系统 app data 目录；在项目模式下就是 `./.daily-paper/`。

实现 lock、atomic write、cache lookup、run manifest。

## 第一版配置草案

```toml
[state]
# 可选。不设置时使用系统 app data 目录。
# dir = "/path/to/daily-paper-state"

[zotero]
user_id = "123456"
api_key_env = "ZOTERO_API_KEY"
max_snapshot_age_hours = 168

[[zotero.filters]]
path = "active-projects/**"
weight = 2.0

[[zotero.filters]]
path = "ignored/**"
exclude = true

[[sources]]
kind = "arxiv"
categories = ["cs.AI", "cs.CL", "cs.LG"]
include_cross_list = false

[embedding]
kind = "openai-compatible"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "text-embedding-3-small"
batch_size = 64
timeout_secs = 60
max_retries = 5
max_concurrency = 4

[reranker]
kind = "embedding_similarity"
top_k_library_matches = 20

[metadata]
# MVP: 仅从 arXiv 响应提取，不做 PDF 首页解析和机构识别
# notable_authors 名单匹配推迟到第二版
# notable_authors = ["Yoshua Bengio", "Geoffrey Hinton"]

[reader]
kind = "openai-compatible"
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
model = "gpt-4o-mini"
top_n = 10
language = "zh-CN"
require_full_text = true
on_read_failure = "block"
timeout_secs = 120
max_retries = 5
max_concurrency = 2
max_input_tokens = 60000

[pdf]
extractor = "pdftotext"
timeout_secs = 60
max_pdf_mb = 50
max_text_chars = 300000

[email]
smtp_server = "smtp.example.com"
smtp_port = 465
sender = "sender@example.com"
receiver = "receiver@example.com"
password_env = "SMTP_PASSWORD"
```

## CLI 范围

MVP 命令：

```text
daily-paper run
daily-paper status
```

`retry` 命令推迟到 M7 之后。retry 的核心价值是"只重试失败论文"，需要完善的 run manifest 读取逻辑，可以在 run 流程稳定后再加。

### run

```text
daily-paper run --config daily-paper.toml
daily-paper run --config daily-paper.toml --date 2026-05-26
daily-paper run --config daily-paper.toml --dry-run
daily-paper run --config daily-paper.toml --state-dir ./.daily-paper
```

参数：

```text
--config <path>        配置文件路径
--state-dir <path>     覆盖状态目录
--date <yyyy-mm-dd>    抓取该日期对应的新论文
--dry-run              执行到 render，但不发送
--force-zotero-sync    忽略 Zotero snapshot 缓存
--force-rerank         忽略 rerank 缓存
--force-read           忽略 read-result 缓存
--force-send           即使 report 已发送也再次发送
```

默认行为：

- 未传 `--config` 时读取平台默认配置文件。
- 未传 `--state-dir` 且配置没有 `[state].dir` 时使用平台 app data 目录。
- 未传 `--date` 时抓取运行日之前的自然日，按本地时区计算日期窗口，再转换为 UTC 存储。
- 优先级为 CLI 参数 > 配置文件 > 内置默认值。

### status

```text
daily-paper status
daily-paper status --run-id <run-id>
```

输出：

- run 状态。
- 各阶段成功/失败/cache hit。
- Top N 论文精读状态。
- 如果 blocked，显示阻塞论文和错误。

### retry

```text
daily-paper retry --run-id <run-id>
daily-paper retry --run-id <run-id> --paper-id <paper-id>
```

行为：

- 创建新的 retry run，并通过 `parent_run_id` 指向原 run。
- 重用原 run 的脱敏 resolved config、时间窗口、rerank 结果和 Top N selection。
- 只重试失败或缺失的阶段。
- 默认不重新发送已发送报告。

## 里程碑

### M0: 项目骨架

产出：

- Rust workspace 或 single crate。
- CLI skeleton。
- 配置加载。
- tracing 日志。
- typed error。

验收：

- `daily-paper --help` 可运行。
- 无配置时错误清楚。

### M1: StateStore

产出：

- 文件 StateStore。
- lock。
- atomic JSON write。
- run manifest。

验收：

- 两个并发 run 中第二个会失败退出。
- 模拟半途失败后，已完成阶段的 manifest 仍可读取。

### M2: Zotero Sync

产出：

- Zotero API client。
- 增量同步。
- collection path 构建。
- filter 权重。
- Zotero snapshot。
- 分页、429 backoff、trashed item 处理。
- snapshot freshness 策略。

验收：

- 首次运行生成 snapshot。
- 第二次 Zotero 无变化时复用 snapshot。
- 429/网络错误有 retry 和清晰错误。

### M3: arXiv Source

产出：

- arXiv fetch。
- category 配置。
- include cross-list 配置。
- 日期窗口语义。
- 分页上限。
- CandidatePaper 序列化。

验收：

- 指定日期窗口能拉到候选论文。
- 同一论文不会因为多个 category 重复出现。

MVP 规则：

- 日期窗口使用 UTC。
- 候选论文以 submitted date 为主；如果 source API 只能稳定提供 updated date，需要在 source result 中明确记录该语义。
- 默认只收 primary category 命中订阅分类的论文。
- `include_cross_list = true` 时允许 cross-list category 命中。

### M4: Embedding + Rerank

产出：

- OpenAI-compatible embedding client。
- embedding cache。
- cosine similarity。
- weighted top-k aggregation。
- Top N selection。

验收：

- 第二次运行不会重复请求已有 embedding。
- 改变 `top_n` 后只影响 selection 和后续 read。

### M5: PDF + Metadata

产出：

- PDF 下载。
- `pdftotext` 提取。
- 从 arXiv API 响应中提取已有 metadata（authors、abstract、comment）。
- 从 arXiv abstract page 提取 links。

MVP 不做：

- PDF 首页文本解析、机构识别。
- GitHub/project page 链接抓取。
- 代码仓库自动发现。

这些功能对最终邮件质量提升有限，但实现成本高，推迟到第二版。

验收：

- PDF 已下载时不会重复下载。
- PDF 提取失败时 run blocked，不发送。
- metadata 抓取失败不阻塞，但记录 warning。

### M6: Deep Read

产出：

- reader template。
- section-aware 文本裁剪。
- OpenAI-compatible chat/completions client。
- read-result cache。

验收：

- 每篇 Top N 生成一段中文总结。
- LLM 单篇失败时只标记该篇失败。
- 下次 retry 只重试失败论文。

### M7: Render + Email

产出：

- HTML template。
- text fallback。
- SMTP 发送。
- delivery receipt。

验收：

- Top N 全部精读完成才发送。
- 同一 report 默认不重复发送。
- `--dry-run` 不发送但产出报告。

## 推荐算法 MVP

MVP 使用 embedding similarity。

输入：

- Zotero 全库文献 embedding。
- 候选论文 embedding。
- 每篇 Zotero 文献的 `interest_weight`。

算法：

```text
for candidate in candidates:
    sims = cosine(candidate, each_library_paper)
    top = top_k(sims, k = top_k_library_matches)
    score = weighted_average(top.similarity, top.interest_weight)
rank by score desc
select top_n
```

后续可以增加二阶段 rerank：

```text
embedding similarity Top 30 -> LLM/reranker API -> final Top N
```

接口上不要把 reranker 写死成单一实现。

## 精读输入策略

不能把全文无脑塞进 LLM。

MVP 策略：

1. 必须先下载 PDF 并完整提取正文。
2. 总是包含 title、abstract、authors、metadata。
3. 从完整正文中优先选取这些 section：
   - introduction
   - method / approach / model
   - experiments / results
   - conclusion / discussion
4. 如果 section 识别失败，则按文本位置取：
   - 开头部分
   - 中间部分若干窗口
   - 结尾部分
5. 控制输入 token 上限，超限时优先保留 abstract、introduction、conclusion。
6. 后续可以把全文 chunk map-reduce 加入 reader，但 MVP 先做 section-aware selection。

输出要求：

- 一段中文。
- 不超过配置的字数上限。
- 合并说明机构、作者亮点、代码或项目链接。
- 不输出长列表式读书笔记。

## 失败策略

| 阶段 | 默认行为 |
| --- | --- |
| Zotero 网络失败 | 有可用 snapshot 时降级使用；否则失败 |
| Source fetch 失败 | run failed |
| Embedding 单条失败 | retry；仍失败则 run failed |
| Rerank 失败 | run failed |
| Metadata 失败 | warning，不阻塞 |
| PDF 下载失败 | run blocked，不发送 |
| PDF 提取失败 | run blocked，不发送 |
| LLM 单篇失败 | run blocked，不发送 |
| Render 失败 | run failed |
| Email 失败 | run failed，但 report 保留，可 retry send |

## 开发顺序建议

按依赖顺序做，不要先做完整模板或漂亮邮件。项目结构见 `project-structure.md`。

```text
M0 骨架 (config + error + CLI)
 ↓
M1 StateStore
 ↓
┌──────────────┬──────────────┐
M2 Zotero      M3 arXiv       （可并行，共享 dp-core 模型）
└──────┬───────┴──────┬───────┘
       ↓              ↓
    M4 Embedding + Rerank
       ↓
    M5 PDF + Metadata
       ↓
    M6 Deep Read
       ↓
    M7 Render + Email
```

M2 和 M3 共享 `CandidatePaper`/`LibraryPaper` 模型，但彼此无运行时依赖，可由两人并行开发。M4 必须等 M2+M3 都完成。

## 验收样例

最小真实验收：

1. 准备一个 Zotero 账号和 API key。
2. 配置 arXiv `cs.AI`、`cs.CL`。
3. 设置 `top_n = 3`。
4. 运行：

```text
daily-paper run --config daily-paper.toml --state-dir ./.daily-paper --date 2026-05-26
```

期望：

- `.daily-paper/` 下有 `state/`、`cache/`、`reports/`、`deliveries/`。
- Top 3 均有 PDF、extracted text、read result。
- 生成 HTML。
- 邮件发送成功。
- 第二次运行 cache hit，不重复请求 Zotero 全量数据、不重复下载 PDF、不重复 LLM 精读。

## 测试策略

### 单元测试

- `dp-models`：序列化/反序列化、ID 生成、hash 计算、paper_id 规范化。
- `dp-state`：原子写入、锁行为、路径安全、StatePath 禁止 `..`。
- `dp-source`：去重逻辑、arXiv XML 解析。
- `dp-pdf`：section 识别、文本裁剪。
- `dp-render`：模板渲染输出。

### 集成测试（mock 外部 HTTP）

使用 **wiremock** mock HTTP 服务。理由：mockall 需要为每个 trait 方法手写闭包，wiremock 直接录制 JSON fixture 更直观，且可模拟 429、timeout 等边界情况。

- 完整 pipeline 端到端。
- 失败恢复：中间阶段失败后 retry 只执行缺失阶段。
- 缓存命中：第二次运行不重复请求。

### 验收测试（真实 API）

- Zotero + arXiv + OpenAI 真实调用。
- 需要 API key，标记 `#[ignore]`。
