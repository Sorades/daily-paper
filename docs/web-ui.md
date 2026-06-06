# Web UI 设计

本文档定义 `daily-paper serve` 前端的实现范围、页面结构和 API 契约。前端是本地单用户管理界面，不做多用户权限、远程部署或账号系统。

## 目标

- 浏览每日 run、报告和错误状态
- 触发完整 pipeline 或指定阶段重跑
- 观察运行中的 stage、进度和结束状态
- 查看、保存并 reload 配置
- 查看和清理缓存
- 删除错误或过期 run/report
- 手动发送已有报告

静态资源由 serve 从 `<data_dir>/ui` 提供，根路径 `/` 重定向到 `/ui/`。报告文件由 `/report/{run_id}/report.html` 暴露。

## 页面与交互

### Dashboard

- 默认进入当前年月视图。
- 调用 `GET /api/stats/{year}/{month}` 获取每日 run 统计。
- 点击某天进入 date detail。
- 提供上一月、下一月、今天按钮。
- 显示 running 状态入口，若有运行中 run，连接 SSE 并展示当前 stage。

### Date Detail

- 调用 `GET /api/date/{date}`。
- 列出当天 run，按开始时间倒序显示：状态、开始时间、结束时间、报告是否存在、错误摘要。
- 每个 run 可打开详情、打开报告、发送报告、删除。
- 支持删除当天所有 run：`DELETE /api/date/{date}`。

### Run Detail

- 调用 `GET /api/run/{run_id}` 获取完整 `RunManifest`。
- 展示 stage 列表：stage、status、cache_hit、耗时、error。
- 展示 warnings 和 run-level error。
- 如果报告存在，提供打开 `/report/{run_id}/report.html`。
- 如果报告存在，提供 `POST /api/run/{run_id}/send`。
- 删除单个 run：`DELETE /api/run/{run_id}`。

### Run Launcher

- 调用 `POST /api/run`。
- 默认完整运行，不传 `stages`。
- 可选字段：

```json
{
  "date": "2026-06-06",
  "stages": ["rerank", "deep-read", "render"],
  "from_run": "20260606-120000-abcdef",
  "dry_run": true,
  "send_email": false,
  "no_email": true,
  "force_zotero_sync": false,
  "force_rerank": false,
  "force_read": false,
  "force_send": false,
  "max_candidates": 20
}
```

- `stages` 为空或缺省表示完整 pipeline。
- `stages` 非空且 `from_run` 缺省时，后端会自动选择满足依赖的最新 source run。
- Web 默认不发邮件：`dry_run=true`、`no_email=true`。只有用户明确勾选发送时设置 `send_email=true`、`no_email=false`、`dry_run=false`。
- 成功响应：

```json
{
  "run_id": "20260606-120000-abcdef",
  "message": "pipeline started"
}
```

- `409` 表示已有 pipeline 正在运行。
- `400` 表示日期、stage 或 source run 无效。

### Live Run

- 连接 `GET /api/run/stream?run_id={run_id}`。
- SSE event name 固定为 `pipeline`，data 是 `PipelineEvent` JSON。
- 事件类型：

```json
{"Started":{"run_id":"..."}}
{"StageStart":{"run_id":"...","stage":"Rerank"}}
{"StageEnd":{"run_id":"...","stage":"Rerank","status":"Succeeded","cache_hit":false,"duration_ms":1234}}
{"Progress":{"run_id":"...","stage":"DeepRead","current":2,"total":10,"message":"..."}}
{"Ended":{"run_id":"...","status":"Succeeded"}}
```

前端必须容忍 SSE 断线。断线后重新请求 `GET /api/run/{run_id}` 作为权威状态，再决定是否重连。

### Config

- `GET /api/config` 返回：

```json
{"path":"/path/to/config.toml","content":"..."}
```

- `PUT /api/config` body：

```json
{"content":"...toml..."}
```

- `POST /api/config/reload` 重新加载配置到当前 serve 进程。
- 前端保存配置后应提示用户 reload；不自动 reload，避免编辑中配置立刻影响运行。

### Cache

- `GET /api/cache` 返回每个 cache kind 的 size 和 file_count。
- cache kind 来自 core 的 `CACHE_KINDS`：`arxiv, embeddings, models, papers, rerank, zotero, deliveries, reports, runs`。
- `DELETE /api/cache/{kind}` 清理单类缓存。
- `DELETE /api/cache/all` 清理所有缓存并由后端恢复标准目录结构。
- 清理 `runs` 或 `reports` 会影响 Dashboard 和报告链接，前端需要二次确认。

### Status

- `GET /api/status` 返回：

```json
{"pipeline_running": true}
```

### Logs

- `GET /api/logs/stream` SSE 端点。
- 事件类型：`log`（日志行 JSON）、`ping`（心跳）。
- 先发送缓冲历史，再实时推送新日志。

## API 错误处理

错误响应统一是：

```json
{"message":"human readable error"}
```

前端按 HTTP status 决定交互：

| Status | 用法 |
|--------|------|
| 400 | 输入无效，展示 message 并保留表单 |
| 404 | 资源不存在，提示刷新列表 |
| 409 | pipeline 已在运行，引导用户查看 Live Run |
| 500 | 后端错误，展示 message |

## UI 实现建议

- 前端源码使用 TypeScript 编写，源码目录和构建链在实现阶段确定。
- `daily-paper serve` 只负责提供构建产物；产物目录仍是 `<data_dir>/ui`。
- 使用 hash route：`#/`, `#/date/2026-06-06`, `#/run/{run_id}`, `#/config`, `#/cache`。
- 所有 API 使用相对路径，保证 serve 的端口和路径可迁移。
- 状态以后端为准。前端只缓存当前页面数据，不做持久本地状态。
- 所有 destructive action 使用确认弹窗或确认 modal：删除 run、删除 date、清理 cache、清理 all。

## 验收标准

- 从 `/ui/` 可以完成完整运行、查看 SSE 进度、查看 run detail 和报告。
- 未配置邮件发送时，完整 run 默认不会发送邮件。
- 手动发送报告失败时，run detail 能看到失败信息。
- 配置页面能保存 TOML，并能 reload 当前 serve 配置。
- cache 页面能显示所有 `CACHE_KINDS`，清理后大小刷新。
- SSE 断线或页面刷新后，能通过 `GET /api/run/{run_id}` 恢复状态。
