# 文件 StateStore 设计

本文档定义第一版默认状态存储。目标是在不引入数据库的情况下，支持缓存、checkpoint、失败恢复和手工检查。

## 总体原则

- StateStore 是一个接口，第一版实现为文件系统 backend。
- 所有写入必须原子化：先写临时文件，再 rename。
- 不依赖文件修改时间作为业务状态。
- 所有缓存判断基于显式 ID、hash、远端 version 和配置 hash。
- 大文件和 manifest 分开存储。
- 单用户、单进程运行是默认场景，但仍需防止定时任务重叠。

## 根目录

安装后的默认根目录不放在当前工作目录，而是使用系统 app data 目录：

```text
Linux:   ~/.local/share/daily-paper/
macOS:   ~/Library/Application Support/daily-paper/
Windows: %APPDATA%/daily-paper/
```

配置文件默认位置：

```text
Linux:   ~/.config/daily-paper/config.toml
macOS:   ~/Library/Application Support/daily-paper/config.toml
Windows: %APPDATA%/daily-paper/config.toml
```

实现上建议使用 Rust `directories` crate 解析平台目录。MVP 使用：

```rust
ProjectDirs::from("", "", "daily-paper")
```

并取：

```text
config-root = ProjectDirs::config_dir()
state-root  = ProjectDirs::data_dir()
```

MVP 不使用平台 cache dir，避免系统自动清理导致昂贵缓存丢失；可重建缓存也放在 `<state-root>/cache/`。

可通过配置或 CLI 覆盖 state 目录：

```toml
[state]
dir = "/path/to/daily-paper-state"
```

```text
daily-paper run --state-dir ./.daily-paper
```

项目内 `.daily-paper/` 只作为显式项目模式使用，不作为安装后默认路径。

路径展开示例：

```text
# 默认安装模式，Linux
~/.local/share/daily-paper/
  lock
  state/
  cache/
  reports/
  deliveries/

# 显式项目模式
./.daily-paper/
  lock
  state/
  cache/
  reports/
  deliveries/
```

## 目录布局

```text
<state-root>/
  lock
  state/
    runs/
      <run-id>.json
      <run-id>.config.json
      latest
    dates/
      <date>/
        report.json                  # ReportIndex，指向 reports/<run-id>/report.*
    zotero/
      sync-state.json
      snapshots/
        <snapshot-id>.json
    interest-profiles/
      <profile-id>.json
    dedup/
      <run-id>.json
  cache/
    sources/
      <source-kind>/
        <source-fetch-key>.json
        latest-by-window/
          <window-label>.json
    embeddings/
      <model-id>/
        <input-hash>.json          # EmbeddingRecord metadata（不含 vector）
        <input-hash>.vec           # f32 little-endian 二进制向量
    id-aliases/
      <canonical-id>.json          # PaperIdAlias 交叉解析表
    rerank/
      <rerank-cache-key>.json
    selections/
      <selection-id>.json
    papers/
      <paper-id>/
        candidate.json
        metadata.json
        paper.pdf
        pdf.json
        extracted.txt
        extracted.json
        read-task.json
        read-result.json
  reports/
    <report-hash>.html
    <report-hash>.txt
    <report-hash>.json
  deliveries/
    <delivery-key>.latest.json
    history/
      <delivery-key>.<timestamp>.json
```

`latest` 可以是一个小 JSON 文件，而不是 symlink，避免跨平台问题：

```json
{
  "run_id": "2026-05-27T12-00-00Z",
  "updated_at": "2026-05-27T12:03:00Z"
}
```

## Run ID

建议格式：

```text
<utc-timestamp>-<short-random>
```

示例：

```text
2026-05-27T12-00-00Z-a13f9c
```

不能只用日期，因为同一天可能手动运行多次。

## 文件命名约定

### paper-id 转路径

`paper_id` 可能包含 `/`、`:` 等字符。路径中应使用安全编码。

建议：

```text
paper_dir = sha256(paper_id)[0..16] + "-" + slug(title)
```

manifest 中保留真实 `paper_id`。

### model-id 转路径

`model_id` 同样需要路径安全编码：

```text
model_dir = urlsafe_base64(model_id)
```

## 原子写入

写 JSON manifest：

1. 序列化为 pretty JSON。
2. 写入同目录临时文件：`<target>.tmp.<pid>.<random>`。
3. flush。
4. rename 到目标路径。

如果实现上能做到，也应 fsync 父目录；MVP 可以先不做。

## Lock

需要防止两个 run 同时写 `<state-root>/state`。

MVP 使用 `create_new` 创建 lock 文件，并用 `sysinfo` 检查本机进程是否仍存在。后续如果需要更强跨平台文件锁，可以改用 `fs4` 或 `fd-lock`。

### Lock 文件内容

```json
{
  "pid": 12345,
  "hostname": "local",
  "process_start_time": "2026-05-27T11:59:58Z",
  "created_at": "2026-05-27T12:00:00Z",
  "run_id": "2026-05-27T12-00-00Z-a13f9c"
}
```

### 获取锁

- 使用 create-new 语义创建 `<state-root>/lock`。
- 如果文件已存在，读取其中内容。
- 如果对应进程仍存在，当前 run 退出并提示已有任务运行。
- 如果进程不存在，且 lock 年龄超过配置阈值，则认为 stale lock，可以覆盖。
- 清理 stale lock 后必须重新用 create-new 获取锁；如果 create-new 失败，说明另一个进程已经抢到锁，当前 run 退出。
- 如果无法确认 lock 是否 stale，MVP 应保守退出，并提示用户手动删除 lock 文件。

配置：

```toml
[state.lock]
stale_after_minutes = 360
```

第一版只需要支持本机判断。跨机器共享目录不是目标。

## Stage Checkpoint

每个阶段完成后更新 run manifest。

阶段输出不应只存在于 run manifest 中，而要独立存储。例如：

- Zotero snapshot 写入 `zotero/snapshots/<snapshot-id>.json`
- Rerank result 写入 `rerank/<rerank-cache-key>.json`
- Read result 写入 `papers/<paper-dir>/read-result.json`

run manifest 只记录引用：

```json
{
  "stage": "Rerank",
  "status": "Succeeded",
  "cache_hit": true,
  "output_ref": "state/rerank/abc123.json"
}
```

## 缓存键

所有 hash 输入都必须先规范化：

- 使用稳定字段顺序。
- 列表中无业务顺序的元素先排序。
- 去掉 secret value，只保留 env var 名称或 provider id。
- 不依赖 pretty JSON 空白。
- 时间统一用 UTC RFC3339。
- 字符串 trim，并对 DOI、arXiv ID、URL 使用各自的规范化函数。

### Zotero Snapshot

```text
snapshot_id = sha256(user_id + library_version + item_keys_and_versions)
```

如果无法获取完整版本信息，退化为：

```text
sha256(normalized_zotero_items_json)
```

### Interest Profile

```text
profile_id = sha256(zotero_snapshot_id + zotero_filter_rules_hash)
```

### Source Fetch

```text
source_fetch_key = sha256(source_kind + source_config_hash + date_window)
```

source fetch 文件必须按 `<source-fetch-key>.json` 保存，避免同一天不同 category 或不同 source 配置互相覆盖。`latest-by-window/<window-label>.json` 只作为可读索引。

### Embedding

```text
input_hash = sha256(provider_id + model_id + embedding_config_hash + normalized_input_text)
```

### Rerank

```text
rerank_cache_key = sha256(
  interest_profile_id
  + candidate_set_hash
  + embedding_model_id
  + reranker_config_hash   // 必须显式包含 top_k_library_matches
)
```

`reranker_config_hash` 必须包含 `top_k_library_matches`，否则调整 k 值时旧缓存会误命中。

### Metadata

```text
metadata_key = sha256(
  paper_id
  + source_metadata_hash
  + text_extract_key
  + metadata_config_hash
  + metadata_extractor_version
)
```

### Text Extract

```text
text_extract_key = sha256(
  pdf_sha256
  + extractor_id
  + extractor_config_hash
  + section_parser_version
)
```

### ReadSelection

```text
selection_id = sha256(rerank_cache_key + top_n)
```

### Read Result

```text
read_cache_key = sha256(
  paper_id
  + pdf_sha256
  + text_extract_key
  + metadata_key
  + reader_template_hash
  + llm_model_id
  + language
)
```

### Report

```text
report_hash = sha256(
  date_window
  + selected_paper_ids
  + read_result_cache_keys
  + renderer_template_hash
)
```

`report_hash` 是内容 hash，不包含 `run_id`。`run_id` 只写入 report metadata。这样同一内容跨 run 重跑时不会默认重复发送。

### Delivery

```text
delivery_key = sha256(
  report_hash
  + sink_id
  + sink_config_hash
  + recipient
)
```

发送幂等基于 `delivery_key`，而不是只基于 `report_hash`。同一报告发给不同 recipient 或不同 sink 时，应视为不同 delivery。

## 恢复流程

完整 `run` 的恢复判断：

1. 获取 lock。
2. 创建 run manifest。
3. 读取配置，写入脱敏 resolved config snapshot，并计算 `config_hash`。
4. Zotero：如果远端未变化，使用上次 snapshot；否则写新 snapshot。
5. InterestProfile：按 `profile_id` 查缓存。
6. SourceFetch：按 source/window/config 查缓存；缺失则请求 source。
7. Dedup：通常按 run 生成，可重算，成本低。
8. Embedding：逐条查 embedding cache，缺失才请求。
9. Rerank：按 `rerank_cache_key` 查缓存。
10. ReadSelection：按 `rerank_cache_key + top_n` 派生 Top N。
11. DeepRead：对 Top N 逐篇检查 `read-result.json` 的 cache key。
12. Render：只有 Top N 全部精读成功才渲染。
13. Send：同一 report/sink 已发送则跳过，除非 `--force-send`。
14. 释放 lock。

## Retry Run

`daily-paper retry --run-id <run-id>` 创建一个新的 retry run，而不是覆盖原 run。

retry run 必须：

- 在 `RunManifest.parent_run_id` 中记录原 run。
- 复用原 run 的脱敏 resolved config snapshot。
- 复用原 run 的 date window。
- 复用原 run 的 rerank result 和 ReadSelection。
- 只执行缺失、失败或 cache key 不匹配的阶段。

普通 `daily-paper run` 会重新按当前配置和当前候选集合选择 Top N；它不承担补偿旧 run 的语义。

## 精读任务恢复

单篇论文的恢复顺序：

1. `pdf.json` 存在且 `paper.pdf` hash 匹配：跳过下载。
2. `extracted.json` 存在且 `text_extract_key` 匹配：跳过正文提取。
3. `metadata.json` 存在且 `metadata_key` 匹配：跳过 metadata 抓取。
4. `read-result.json` 存在且 cache key 匹配：跳过 LLM。
5. 否则从缺失阶段继续。

如果某篇 Top N 失败：

- 写入 `read-task.json`。
- run 标记为 `Blocked`。
- 不渲染可发送报告。
- `daily-paper retry` 只重试失败论文；普通 run 重新执行当天流程。

## 发送幂等

发送前检查：

```text
deliveries/<delivery-key>.latest.json
```

存在则默认跳过。

发送开始前先写 delivery attempt：

```text
deliveries/history/<delivery-key>.<timestamp>.attempt.json
```

发送成功后再写历史 receipt，并更新 `deliveries/<delivery-key>.latest.json`。SMTP 难以提供真正 exactly-once；如果发送成功但 final receipt 写入失败，下次可能重复发送。MVP 应在邮件中设置稳定 `Message-ID` 或自定义 header，便于识别重复邮件。

`--force-send`：

- 可以重复发送。
- 新 receipt 文件应写入 history，并更新 latest：

```text
deliveries/history/<delivery-key>.<timestamp>.json
deliveries/<delivery-key>.latest.json
```

## 清理策略

MVP 不自动删除缓存。

后续可增加：

```text
daily-paper gc --older-than 90d
daily-paper gc --keep-runs 30
```

默认保留：

- Zotero 最近 snapshot。
- 所有 embedding。
- 最近 N 天 source fetch。
- 最近 N 天 PDF 和 extracted text。
- 所有已发送报告 receipt。

## FileStateStore 接口草案

MVP 先实现 concrete `FileStateStore`，不要急着抽成 trait object。泛型 async trait 对 object safety 不友好，后续确实需要多 backend 时再抽接口。

### StatePath

所有文件访问都必须通过 typed `StatePath` 构造，不能把外部字符串直接拼到根目录后面。

约束：

- `StatePath` 必须是相对路径。
- 禁止 `..`、绝对路径、空 path segment。
- 用户输入只能作为 ID 参与编码后的文件名，不能直接作为路径片段。
- MVP 不跟随 state root 内部的 symlink。
- 写入前确保目标路径仍位于 `<state-root>` 内。

```rust
struct FileStateStore;

impl FileStateStore {
    async fn acquire_lock(&self, run_id: &str) -> Result<RunLock> { todo!() }
    async fn write_json<T: Serialize>(&self, path: StatePath, value: &T) -> Result<()> { todo!() }
    async fn read_json<T: DeserializeOwned>(&self, path: StatePath) -> Result<Option<T>> { todo!() }
    async fn exists(&self, path: StatePath) -> Result<bool> { todo!() }
    async fn write_bytes(&self, path: StatePath, bytes: &[u8]) -> Result<ContentHash> { todo!() }
    async fn read_bytes(&self, path: StatePath) -> Result<Option<Vec<u8>>> { todo!() }
}
```

实现时可以先做同步文件 IO，再在 async 边界用 `spawn_blocking` 包起来；MVP 不必过早优化。

## 需要测试的行为

- 原子写入不会留下半截 JSON。
- stale lock 可以被清理。
- 非 stale lock 会阻止第二个 run。
- cache key 变化后不会误用旧结果。
- Top N 中单篇失败时，run 进入 `Blocked` 且不发送。
- retry 时只执行缺失或失败阶段。
