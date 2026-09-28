<script lang="ts">
  import { getConfig, saveConfig, reloadConfig } from '../api'
  import type { ConfigResponse } from '../types'
  import Spinner from '../components/Spinner.svelte'
  import ErrorBanner from '../components/ErrorBanner.svelte'
  import Icon from '../components/Icon.svelte'
  import { parseToml, serializeToml, type ParsedConfig } from '../utils/toml'

  let viewMode = $state<'form' | 'raw'>('form')
  let rawContent = $state('')
  let parsed = $state<ParsedConfig>(parseToml(''))
  let data = $state<ConfigResponse | null>(null)

  let loading = $state(true)
  let saving = $state(false)
  let reloading = $state(false)
  let error = $state<string | null>(null)
  let successMsg = $state<string | null>(null)
  let dirty = $state(false)

  // ArXiv categories input helper
  let categoriesText = $state('')

  async function loadConfig() {
    loading = true
    error = null
    try {
      data = await getConfig()
      rawContent = data.content
      parsed = parseToml(rawContent)
      if (parsed.sources && parsed.sources[0]) {
        categoriesText = (parsed.sources[0].categories || []).join(', ')
      }
      dirty = false
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      loading = false
    }
  }

  function markDirty() {
    dirty = true
    successMsg = null
  }

  function handleCategoriesChange() {
    const list = categoriesText
      .split(',')
      .map((s) => s.trim())
      .filter(Boolean)
    if (!parsed.sources || parsed.sources.length === 0) {
      parsed.sources = [{ kind: 'arxiv', categories: list }]
    } else {
      parsed.sources[0].categories = list
    }
    markDirty()
  }

  function syncRawFromForm() {
    try {
      rawContent = serializeToml(parsed)
    } catch (e) {
      console.error('Failed to serialize TOML:', e)
    }
  }

  function syncFormFromRaw() {
    try {
      parsed = parseToml(rawContent)
      if (parsed.sources && parsed.sources[0]) {
        categoriesText = (parsed.sources[0].categories || []).join(', ')
      }
    } catch (e) {
      console.error('Failed to parse TOML:', e)
    }
  }

  function switchMode(mode: 'form' | 'raw') {
    if (viewMode === 'raw' && mode === 'form') {
      syncFormFromRaw()
    } else if (viewMode === 'form' && mode === 'raw') {
      syncRawFromForm()
    }
    viewMode = mode
  }

  async function handleSave() {
    saving = true
    error = null
    successMsg = null

    if (viewMode === 'form') {
      syncRawFromForm()
    }

    try {
      await saveConfig(rawContent)
      dirty = false
      successMsg = '配置已成功保存！点击“重载配置”即可在后台进程中即时生效。'
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      saving = false
    }
  }

  async function handleReload() {
    reloading = true
    error = null
    successMsg = null
    try {
      await reloadConfig()
      successMsg = '后台进程已成功热重载新配置。'
      await loadConfig()
    } catch (e: unknown) {
      error = e instanceof Error ? e.message : String(e)
    } finally {
      reloading = false
    }
  }

  $effect(() => {
    loadConfig()
  })
</script>

<div class="config-page">
  <!-- Top Header with Actions -->
  <div class="page-header">
    <div class="header-left">
      <h2>系统设置</h2>
      <p class="header-desc">
        管理定时任务、Zotero 库、arXiv 订阅分类、大模型推理及邮件通知。
      </p>
    </div>

    <div class="header-actions">
      <!-- Mode Toggle Segmented Control -->
      <div class="mode-toggle">
        <button
          type="button"
          class="mode-btn"
          class:active={viewMode === 'form'}
          onclick={() => switchMode('form')}
        >
          <Icon name="settings" size={14} />
          <span>图形表单</span>
        </button>
        <button
          type="button"
          class="mode-btn"
          class:active={viewMode === 'raw'}
          onclick={() => switchMode('raw')}
        >
          <Icon name="file-text" size={14} />
          <span>TOML 代码</span>
        </button>
      </div>

      <div class="action-btn-group">
        <button
          type="button"
          class="secondary sm"
          onclick={loadConfig}
          disabled={loading || saving || reloading}
          title="放弃未保存的更改"
        >
          <Icon name="refresh" size={14} />
          <span>放弃修改</span>
        </button>

        <button
          type="button"
          class="secondary sm"
          onclick={handleReload}
          disabled={reloading || saving}
          title="在运行中的进程中重载配置"
        >
          {#if reloading}
            <Spinner size="small" />
            <span>重载中...</span>
          {:else}
            <Icon name="refresh" size={14} />
            <span>重载服务</span>
          {/if}
        </button>

        <button
          type="button"
          class="primary sm"
          onclick={handleSave}
          disabled={saving || !dirty}
        >
          {#if saving}
            <Spinner size="small" color="#ffffff" />
            <span>保存中...</span>
          {:else}
            <Icon name="check" size={14} />
            <span>{dirty ? '保存配置 *' : '已保存'}</span>
          {/if}
        </button>
      </div>
    </div>
  </div>

  {#if error}
    <ErrorBanner message={error} onDismiss={() => (error = null)} />
  {/if}

  {#if successMsg}
    <div class="success-banner">
      <Icon name="check" size={16} />
      <span class="banner-text">{successMsg}</span>
      <div class="banner-actions">
        {#if !dirty && successMsg.includes('重载配置')}
          <button class="primary sm" onclick={handleReload} disabled={reloading}>
            立即重载
          </button>
        {/if}
        <button class="dismiss-btn" onclick={() => (successMsg = null)}>
          <Icon name="x" size={14} />
        </button>
      </div>
    </div>
  {/if}

  {#if loading}
    <div class="loading-state">
      <Spinner size="large" />
      <p>正在读取配置...</p>
    </div>
  {:else if data}
    {#if viewMode === 'form'}
      <div class="form-layout">
        <!-- Card 1: 自动化与定时调度 -->
        <section class="config-card">
          <div class="card-header">
            <div class="card-icon primary">
              <Icon name="clock" size={18} />
            </div>
            <div class="card-header-text">
              <h3>自动化与定时调度</h3>
              <p>每天早晨自动抓取、分析最新论文并生成简报</p>
            </div>
            <label class="switch-wrap">
              <input
                type="checkbox"
                bind:checked={parsed.schedule!.enabled}
                onchange={markDirty}
              />
              <span class="switch-slider"></span>
              <span class="switch-label">{parsed.schedule!.enabled ? '已开启' : '已关闭'}</span>
            </label>
          </div>

          <div class="card-body">
            <div class="form-grid cols-2">
              <div class="form-field">
                <label for="sched-time">每日执行时间 (本地时间)</label>
                <input
                  id="sched-time"
                  type="time"
                  bind:value={parsed.schedule!.time}
                  oninput={markDirty}
                  disabled={!parsed.schedule!.enabled}
                />
                <span class="field-hint">默认每天 07:30 自动执行 pipeline</span>
              </div>

              <div class="form-field">
                <label for="web-port">Web 服务端口</label>
                <input
                  id="web-port"
                  type="number"
                  bind:value={parsed.web!.port}
                  oninput={markDirty}
                  placeholder="8991"
                />
                <span class="field-hint">Serve 命令启动的 HTTP 端口</span>
              </div>
            </div>
          </div>
        </section>

        <!-- Card 2: 关注领域与 Zotero 兴趣库 -->
        <section class="config-card">
          <div class="card-header">
            <div class="card-icon success">
              <Icon name="paper" size={18} />
            </div>
            <div class="card-header-text">
              <h3>关注领域与个性化兴趣库</h3>
              <p>定义每日抓取的 arXiv 学科，以及用于计算语义匹配度的 Zotero 收藏</p>
            </div>
          </div>

          <div class="card-body">
            <div class="form-field">
              <label for="arxiv-cats">arXiv 关注领域 (逗号分隔)</label>
              <input
                id="arxiv-cats"
                type="text"
                bind:value={categoriesText}
                oninput={handleCategoriesChange}
                placeholder="cs.AI, cs.CL, cs.LG"
              />
              <span class="field-hint">
                常用分类：<code>cs.AI</code> (人工智能), <code>cs.CL</code> (自然语言处理), <code>cs.LG</code> (机器学习), <code>cs.CV</code> (计算机视觉)
              </span>
            </div>

            <div class="form-grid cols-2" style="margin-top: 14px;">
              <div class="form-field">
                <label for="zotero-user">Zotero User ID</label>
                <input
                  id="zotero-user"
                  type="text"
                  bind:value={parsed.zotero!.user_id}
                  oninput={markDirty}
                  placeholder="如: 1234567"
                />
                <span class="field-hint">Zotero 账号设置中的用户数字编号</span>
              </div>

              <div class="form-field">
                <label for="zotero-key">Zotero API Key</label>
                <input
                  id="zotero-key"
                  type="password"
                  bind:value={parsed.zotero!.api_key}
                  oninput={markDirty}
                  placeholder="从 Zotero API 页面创建并粘贴"
                />
                <span class="field-hint">需要具备个人文献库只读权限</span>
              </div>
            </div>
          </div>
        </section>

        <!-- Card 3: AI 大模型与语义理解 -->
        <section class="config-card">
          <div class="card-header">
            <div class="card-icon info">
              <Icon name="zap" size={18} />
            </div>
            <div class="card-header-text">
              <h3>AI 大模型与语义分析</h3>
              <p>配置用于深度研读论文全文的 LLM，以及向量语义匹配模型</p>
            </div>
          </div>

          <div class="card-body">
            <!-- Reader LLM Section -->
            <div class="subsection-title">
              <span>深度精读大模型 (Reader LLM)</span>
            </div>

            <div class="form-grid cols-3">
              <div class="form-field">
                <label for="reader-url">API Base URL</label>
                <input
                  id="reader-url"
                  type="text"
                  bind:value={parsed.reader!.base_url}
                  oninput={markDirty}
                  placeholder="https://api.openai.com/v1"
                />
              </div>

              <div class="form-field">
                <label for="reader-key">API Key</label>
                <input
                  id="reader-key"
                  type="password"
                  bind:value={parsed.reader!.api_key}
                  oninput={markDirty}
                  placeholder="sk-..."
                />
              </div>

              <div class="form-field">
                <label for="reader-model">模型名称</label>
                <input
                  id="reader-model"
                  type="text"
                  bind:value={parsed.reader!.model}
                  oninput={markDirty}
                  placeholder="如: gpt-4o-mini, gemini-2.5-pro"
                />
              </div>
            </div>

            <div class="form-grid cols-3" style="margin-top: 14px;">
              <div class="form-field">
                <label for="reader-topn">精读论文数量 (Top-N)</label>
                <input
                  id="reader-topn"
                  type="number"
                  bind:value={parsed.reader!.top_n}
                  oninput={markDirty}
                  min="1"
                  max="50"
                  placeholder="10"
                />
                <span class="field-hint">评分最高的前 N 篇进入全文分析</span>
              </div>

              <div class="form-field">
                <label for="reader-lang">输出简报语言</label>
                <select
                  id="reader-lang"
                  bind:value={parsed.reader!.language}
                  onchange={markDirty}
                >
                  <option value="zh-CN">简体中文 (zh-CN)</option>
                  <option value="en">English (en)</option>
                </select>
                <span class="field-hint">生成的日报 HTML 内容语言</span>
              </div>

              <div class="form-field">
                <label for="read-fail">精读失败策略</label>
                <select
                  id="read-fail"
                  bind:value={parsed.reader!.on_read_failure}
                  onchange={markDirty}
                >
                  <option value="block">中断退出 (Block)</option>
                  <option value="retry">自动重试 (Retry)</option>
                  <option value="skip">跳过继续 (Skip)</option>
                </select>
                <span class="field-hint">某篇论文解析失败时的处理</span>
              </div>
            </div>

            <!-- Embedding Section -->
            <div class="subsection-title" style="margin-top: 22px;">
              <span>向量嵌入模型 (Embedding)</span>
            </div>

            <div class="form-grid cols-2">
              <div class="form-field">
                <label for="embed-kind">向量计算方式</label>
                <select
                  id="embed-kind"
                  bind:value={parsed.embedding!.kind}
                  onchange={markDirty}
                >
                  <option value="openai-compatible">OpenAI 兼容 API</option>
                  <option value="fastembed">Fastembed (本地 CPU 运行)</option>
                </select>
                <span class="field-hint">本地运行无需额外 API 消耗</span>
              </div>

              <div class="form-field">
                <label for="embed-model">嵌入模型名称</label>
                <input
                  id="embed-model"
                  type="text"
                  bind:value={parsed.embedding!.model}
                  oninput={markDirty}
                  placeholder="text-embedding-3-small 或 qwen3-embedding:0.6b"
                />
                <span class="field-hint">如 text-embedding-3-small, qwen3-embedding:0.6b</span>
              </div>
            </div>

            {#if parsed.embedding!.kind === 'openai-compatible'}
              <div class="form-grid cols-2" style="margin-top: 14px;">
                <div class="form-field">
                  <label for="embed-url">Embedding API Base URL</label>
                  <input
                    id="embed-url"
                    type="text"
                    bind:value={parsed.embedding!.base_url}
                    oninput={markDirty}
                    placeholder="https://api.openai.com/v1 或 http://127.0.0.1:23000/v1"
                  />
                  <span class="field-hint">独立配置 Embedding 服务的端点地址</span>
                </div>

                <div class="form-field">
                  <label for="embed-key">Embedding API Key</label>
                  <input
                    id="embed-key"
                    type="password"
                    bind:value={parsed.embedding!.api_key}
                    oninput={markDirty}
                    placeholder="sk-..."
                  />
                  <span class="field-hint">用于 Embedding 请求的鉴权密钥</span>
                </div>
              </div>
            {/if}
          </div>
        </section>

        <!-- Card 4: 邮件推送通知 -->
        <section class="config-card">
          <div class="card-header">
            <div class="card-icon warning">
              <Icon name="send" size={18} />
            </div>
            <div class="card-header-text">
              <h3>邮件投递与推送 (SMTP)</h3>
              <p>在 Pipeline 运行完毕后自动将排版精美的 HTML 简报投递至你的邮箱</p>
            </div>
          </div>

          <div class="card-body">
            <div class="form-grid cols-3">
              <div class="form-field col-span-2">
                <label for="smtp-server">SMTP 服务器主机</label>
                <input
                  id="smtp-server"
                  type="text"
                  bind:value={parsed.email!.smtp_server}
                  oninput={markDirty}
                  placeholder="smtp.example.com"
                />
              </div>

              <div class="form-field">
                <label for="smtp-port">SMTP 端口</label>
                <input
                  id="smtp-port"
                  type="number"
                  bind:value={parsed.email!.smtp_port}
                  oninput={markDirty}
                  placeholder="465"
                />
                <span class="field-hint">SSL: 465 / STARTTLS: 587</span>
              </div>
            </div>

            <div class="form-grid cols-3" style="margin-top: 14px;">
              <div class="form-field">
                <label for="email-sender">发件人地址</label>
                <input
                  id="email-sender"
                  type="email"
                  bind:value={parsed.email!.sender}
                  oninput={markDirty}
                  placeholder="digest@yourdomain.com"
                />
              </div>

              <div class="form-field">
                <label for="email-receiver">收件人地址</label>
                <input
                  id="email-receiver"
                  type="email"
                  bind:value={parsed.email!.receiver}
                  oninput={markDirty}
                  placeholder="your-name@example.com"
                />
              </div>

              <div class="form-field">
                <label for="email-pass">SMTP 密码 / 授权码</label>
                <input
                  id="email-pass"
                  type="password"
                  bind:value={parsed.email!.password}
                  oninput={markDirty}
                  placeholder="••••••••••••"
                />
              </div>
            </div>
          </div>
        </section>
      </div>
    {:else}
      <!-- Raw TOML Editor Mode -->
      <div class="raw-card">
        <div class="raw-toolbar">
          <div class="raw-path">
            <Icon name="file-text" size={15} />
            <span>配置文件位置：</span>
            <code>{data.path}</code>
          </div>
          <span class="raw-hint">在此处编辑将直接修改底层 config.toml 文本</span>
        </div>

        <div class="raw-editor-wrap">
          <textarea
            class="raw-textarea"
            bind:value={rawContent}
            oninput={markDirty}
            spellcheck="false"
          ></textarea>
        </div>
      </div>
    {/if}
  {/if}
</div>

<style>
  .config-page {
    display: flex;
    flex-direction: column;
    gap: 24px;
    max-width: 1040px;
    margin: 0 auto;
  }

  /* Page Header */
  .page-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 16px;
  }

  .header-left {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
    color: var(--text-primary);
    letter-spacing: -0.02em;
  }

  .header-desc {
    font-size: 13px;
    color: var(--text-secondary);
    margin: 0;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  /* Segmented Mode Toggle */
  .mode-toggle {
    display: flex;
    background: var(--bg-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    padding: 3px;
    gap: 2px;
  }

  .mode-btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: var(--radius-md);
    border: none;
    background: transparent;
    box-shadow: none;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-secondary);
    cursor: pointer;
    transition: all var(--transition-fast);
  }

  .mode-btn:hover {
    color: var(--text-primary);
  }

  .mode-btn.active {
    background: var(--bg-surface);
    color: var(--primary);
    box-shadow: var(--shadow-xs);
    font-weight: 600;
  }

  .action-btn-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  /* Loading & Success States */
  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    padding: 80px 20px;
    gap: 16px;
    color: var(--text-secondary);
  }

  .success-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 18px;
    border-radius: var(--radius-md);
    background: var(--success-light);
    color: var(--success-text);
    border: 1px solid var(--success-border);
    font-size: 13px;
    font-weight: 500;
  }

  .banner-text {
    flex: 1;
  }

  .banner-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .dismiss-btn {
    background: transparent;
    border: none;
    box-shadow: none;
    padding: 4px;
    color: inherit;
    cursor: pointer;
  }

  /* Form Layout */
  .form-layout {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }

  /* Config Card */
  .config-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
    transition: box-shadow var(--transition-fast);
  }

  .config-card:hover {
    box-shadow: var(--shadow-md);
  }

  .card-header {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 18px 24px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border-subtle);
  }

  .card-icon {
    width: 36px;
    height: 36px;
    border-radius: var(--radius-lg);
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }

  .card-icon.primary {
    background: var(--primary-light);
    color: var(--primary);
  }

  .card-icon.success {
    background: var(--success-light);
    color: var(--success);
  }

  .card-icon.info {
    background: var(--info-light);
    color: var(--info);
  }

  .card-icon.warning {
    background: var(--warning-light);
    color: var(--warning);
  }

  .card-header-text {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .card-header-text h3 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
    color: var(--text-primary);
  }

  .card-header-text p {
    margin: 0;
    font-size: 12px;
    color: var(--text-secondary);
  }

  /* Switch Slider Toggle */
  .switch-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    user-select: none;
  }

  .switch-wrap input {
    display: none;
  }

  .switch-slider {
    width: 40px;
    height: 22px;
    background: var(--bg-tertiary);
    border-radius: var(--radius-full);
    position: relative;
    transition: background var(--transition-fast);
  }

  .switch-slider::before {
    content: '';
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.2);
    transition: transform var(--transition-fast);
  }

  .switch-wrap input:checked + .switch-slider {
    background: var(--primary);
  }

  .switch-wrap input:checked + .switch-slider::before {
    transform: translateX(18px);
  }

  .switch-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }

  /* Card Body & Form Fields */
  .card-body {
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
  }

  .subsection-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-primary);
    margin-bottom: 12px;
    padding-bottom: 6px;
    border-bottom: 1px solid var(--border-subtle);
    display: flex;
    align-items: center;
  }

  .form-grid {
    display: grid;
    gap: 16px;
  }

  .form-grid.cols-2 {
    grid-template-columns: repeat(2, 1fr);
  }

  .form-grid.cols-3 {
    grid-template-columns: repeat(3, 1fr);
  }

  .col-span-2 {
    grid-column: span 2;
  }

  .form-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .form-field label {
    font-size: 13px;
    font-weight: 500;
    color: var(--text-primary);
  }

  .field-hint {
    font-size: 11px;
    color: var(--text-tertiary);
    line-height: 1.4;
  }

  input,
  select {
    width: 100%;
    height: 38px;
    background: var(--bg-surface);
    color: var(--text-primary);
  }

  /* Raw TOML Mode */
  .raw-card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-sm);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .raw-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 20px;
    background: var(--bg-secondary);
    border-bottom: 1px solid var(--border);
    font-size: 13px;
  }

  .raw-path {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-secondary);
  }

  .raw-hint {
    font-size: 12px;
    color: var(--text-tertiary);
  }

  .raw-editor-wrap {
    display: flex;
  }

  .raw-textarea {
    width: 100%;
    min-height: 580px;
    font-family: var(--font-mono);
    font-size: 13px;
    line-height: 1.6;
    tab-size: 2;
    padding: 18px 22px;
    border: none;
    border-radius: 0;
    background: var(--bg-surface);
    color: var(--text-primary);
    resize: vertical;
    box-shadow: none;
  }

  .raw-textarea:focus {
    outline: none;
    box-shadow: none;
  }

  @media (max-width: 800px) {
    .form-grid.cols-2,
    .form-grid.cols-3 {
      grid-template-columns: 1fr;
    }
    .col-span-2 {
      grid-column: span 1;
    }
    .card-header {
      flex-wrap: wrap;
    }
  }
</style>
