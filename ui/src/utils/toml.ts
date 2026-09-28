// Zero-dependency robust TOML parser and serializer tailored for daily-paper config

export interface ParsedConfig {
  schedule?: {
    enabled?: boolean
    time?: string
  }
  zotero?: {
    user_id?: string
    api_key?: string
    max_snapshot_age_hours?: number
    filters?: Array<{ path: string; weight?: number; exclude?: boolean }>
  }
  sources?: Array<{
    kind: string
    categories: string[]
    include_cross_list?: boolean
    max_results_per_page?: number
    max_pages?: number
  }>
  embedding?: {
    kind?: string
    base_url?: string
    api_key?: string
    model?: string
    batch_size?: number
    timeout_secs?: number
    max_retries?: number
    max_concurrency?: number
  }
  reranker?: {
    kind?: string
    top_k_library_matches?: number
  }
  reader?: {
    kind?: string
    base_url?: string
    api_key?: string
    model?: string
    top_n?: number
    language?: string
    require_full_text?: boolean
    on_read_failure?: string
    timeout_secs?: number
    max_retries?: number
    max_concurrency?: number
    max_input_tokens?: number
    system_prompt_path?: string
  }
  pdf?: {
    extractor?: string
    timeout_secs?: number
    max_pdf_mb?: number
    max_text_chars?: number
  }
  email?: {
    smtp_server?: string
    smtp_port?: number
    sender?: string
    receiver?: string
    password?: string
  }
  web?: {
    port?: number
    ui_path?: string
  }
  report_template_path?: string
  [key: string]: any
}

function parseValue(valStr: string): any {
  valStr = valStr.trim()
  if (valStr === 'true') return true
  if (valStr === 'false') return false
  if (/^-?\d+$/.test(valStr)) return parseInt(valStr, 10)
  if (/^-?\d+\.\d+$/.test(valStr)) return parseFloat(valStr)

  // String
  if ((valStr.startsWith('"') && valStr.endsWith('"')) || (valStr.startsWith("'") && valStr.endsWith("'"))) {
    return valStr.slice(1, -1)
  }

  // Array: e.g. ["cs.AI", "cs.CL"]
  if (valStr.startsWith('[') && valStr.endsWith(']')) {
    const inner = valStr.slice(1, -1).trim()
    if (!inner) return []
    // Split by comma outside quotes
    const items: string[] = []
    let current = ''
    let inQuotes = false
    let quoteChar = ''
    for (let i = 0; i < inner.length; i++) {
      const c = inner[i]
      if ((c === '"' || c === "'") && inner[i - 1] !== '\\') {
        if (!inQuotes) {
          inQuotes = true
          quoteChar = c
        } else if (c === quoteChar) {
          inQuotes = false
        }
      }
      if (c === ',' && !inQuotes) {
        items.push(current.trim())
        current = ''
      } else {
        current += c
      }
    }
    if (current.trim()) items.push(current.trim())
    return items.map((it) => parseValue(it))
  }

  return valStr
}

export function parseToml(tomlStr: string): ParsedConfig {
  const result: any = {
    schedule: { enabled: true, time: '07:30' },
    zotero: { user_id: '', api_key: '', max_snapshot_age_hours: 168 },
    sources: [],
    embedding: { kind: 'openai-compatible', model: '', base_url: '', api_key: '' },
    reranker: { kind: 'embedding_similarity', top_k_library_matches: 20 },
    reader: {
      kind: 'openai-compatible',
      base_url: '',
      api_key: '',
      model: '',
      top_n: 10,
      language: 'zh-CN',
      require_full_text: true,
      on_read_failure: 'block',
    },
    pdf: { extractor: 'pdftotext', timeout_secs: 60, max_pdf_mb: 50, max_text_chars: 300000 },
    email: { smtp_server: '', smtp_port: 465, sender: '', receiver: '', password: '' },
    web: { port: 8991 },
  }

  let currentSection: any = result
  let currentSectionName = ''

  const lines = tomlStr.split('\n')
  for (let line of lines) {
    line = line.trim()
    if (!line || line.startsWith('#')) continue

    // Array of tables: [[section]]
    const arrayMatch = line.match(/^\[\[([^\]]+)\]\]$/)
    if (arrayMatch) {
      currentSectionName = arrayMatch[1].trim()
      if (!Array.isArray(result[currentSectionName])) {
        result[currentSectionName] = []
      }
      const newObj = {}
      result[currentSectionName].push(newObj)
      currentSection = newObj
      continue
    }

    // Single Table: [section]
    const tableMatch = line.match(/^\[([^\]]+)\]$/)
    if (tableMatch) {
      currentSectionName = tableMatch[1].trim()
      if (!result[currentSectionName] || typeof result[currentSectionName] !== 'object') {
        result[currentSectionName] = {}
      }
      currentSection = result[currentSectionName]
      continue
    }

    // Key = Value
    const eqIdx = line.indexOf('=')
    if (eqIdx !== -1) {
      const key = line.slice(0, eqIdx).trim()
      let valStr = line.slice(eqIdx + 1).trim()
      // strip trailing inline comment if outside quotes
      if (!valStr.startsWith('"') && !valStr.startsWith("'")) {
        const hashIdx = valStr.indexOf('#')
        if (hashIdx !== -1) valStr = valStr.slice(0, hashIdx).trim()
      }
      const val = parseValue(valStr)
      currentSection[key] = val
    }
  }

  // Ensure default arxiv source exists if empty
  if (!result.sources || result.sources.length === 0) {
    result.sources = [{ kind: 'arxiv', categories: ['cs.AI', 'cs.CL', 'cs.LG'], include_cross_list: false }]
  }

  return result
}

function formatValue(val: any): string {
  if (typeof val === 'string') return `"${val.replace(/"/g, '\\"')}"`
  if (typeof val === 'boolean') return val ? 'true' : 'false'
  if (typeof val === 'number') return String(val)
  if (Array.isArray(val)) {
    return `[${val.map((v) => formatValue(v)).join(', ')}]`
  }
  return `"${String(val)}"`
}

export function serializeToml(config: ParsedConfig): string {
  let out = `# Daily Paper Configuration\n# Generated by Web UI\n\n`

  // 1. [schedule]
  if (config.schedule) {
    out += `[schedule]\n`
    out += `enabled = ${config.schedule.enabled ? 'true' : 'false'}\n`
    if (config.schedule.time) out += `time = ${formatValue(config.schedule.time)}\n`
    out += `\n`
  }

  // 2. [zotero]
  if (config.zotero) {
    out += `[zotero]\n`
    out += `user_id = ${formatValue(config.zotero.user_id || '')}\n`
    out += `api_key = ${formatValue(config.zotero.api_key || '')}\n`
    if (config.zotero.max_snapshot_age_hours !== undefined) {
      out += `max_snapshot_age_hours = ${config.zotero.max_snapshot_age_hours}\n`
    }
    out += `\n`

    if (config.zotero.filters && config.zotero.filters.length > 0) {
      for (const f of config.zotero.filters) {
        out += `[[zotero.filters]]\n`
        out += `path = ${formatValue(f.path)}\n`
        if (f.weight !== undefined) out += `weight = ${f.weight}\n`
        if (f.exclude !== undefined) out += `exclude = ${f.exclude ? 'true' : 'false'}\n`
        out += `\n`
      }
    }
  }

  // 3. [[sources]]
  if (config.sources && config.sources.length > 0) {
    for (const s of config.sources) {
      out += `[[sources]]\n`
      out += `kind = ${formatValue(s.kind || 'arxiv')}\n`
      out += `categories = ${formatValue(s.categories || [])}\n`
      if (s.include_cross_list !== undefined) {
        out += `include_cross_list = ${s.include_cross_list ? 'true' : 'false'}\n`
      }
      if (s.max_results_per_page !== undefined) {
        out += `max_results_per_page = ${s.max_results_per_page}\n`
      }
      if (s.max_pages !== undefined) {
        out += `max_pages = ${s.max_pages}\n`
      }
      out += `\n`
    }
  }

  // 4. [embedding]
  if (config.embedding) {
    out += `[embedding]\n`
    out += `kind = ${formatValue(config.embedding.kind || 'openai-compatible')}\n`
    if (config.embedding.base_url) out += `base_url = ${formatValue(config.embedding.base_url)}\n`
    if (config.embedding.api_key) out += `api_key = ${formatValue(config.embedding.api_key)}\n`
    if (config.embedding.model) out += `model = ${formatValue(config.embedding.model)}\n`
    if (config.embedding.batch_size) out += `batch_size = ${config.embedding.batch_size}\n`
    if (config.embedding.timeout_secs) out += `timeout_secs = ${config.embedding.timeout_secs}\n`
    if (config.embedding.max_retries) out += `max_retries = ${config.embedding.max_retries}\n`
    if (config.embedding.max_concurrency) out += `max_concurrency = ${config.embedding.max_concurrency}\n`
    out += `\n`
  }

  // 5. [reranker]
  if (config.reranker) {
    out += `[reranker]\n`
    out += `kind = ${formatValue(config.reranker.kind || 'embedding_similarity')}\n`
    if (config.reranker.top_k_library_matches) {
      out += `top_k_library_matches = ${config.reranker.top_k_library_matches}\n`
    }
    out += `\n`
  }

  // 6. [reader]
  if (config.reader) {
    out += `[reader]\n`
    out += `kind = ${formatValue(config.reader.kind || 'openai-compatible')}\n`
    out += `base_url = ${formatValue(config.reader.base_url || '')}\n`
    out += `api_key = ${formatValue(config.reader.api_key || '')}\n`
    out += `model = ${formatValue(config.reader.model || '')}\n`
    if (config.reader.top_n) out += `top_n = ${config.reader.top_n}\n`
    if (config.reader.language) out += `language = ${formatValue(config.reader.language)}\n`
    if (config.reader.require_full_text !== undefined) {
      out += `require_full_text = ${config.reader.require_full_text ? 'true' : 'false'}\n`
    }
    if (config.reader.on_read_failure) {
      out += `on_read_failure = ${formatValue(config.reader.on_read_failure)}\n`
    }
    if (config.reader.timeout_secs) out += `timeout_secs = ${config.reader.timeout_secs}\n`
    if (config.reader.max_retries) out += `max_retries = ${config.reader.max_retries}\n`
    if (config.reader.max_concurrency) out += `max_concurrency = ${config.reader.max_concurrency}\n`
    if (config.reader.max_input_tokens) out += `max_input_tokens = ${config.reader.max_input_tokens}\n`
    out += `\n`
  }

  // 7. [pdf]
  if (config.pdf) {
    out += `[pdf]\n`
    out += `extractor = ${formatValue(config.pdf.extractor || 'pdftotext')}\n`
    if (config.pdf.timeout_secs) out += `timeout_secs = ${config.pdf.timeout_secs}\n`
    if (config.pdf.max_pdf_mb) out += `max_pdf_mb = ${config.pdf.max_pdf_mb}\n`
    if (config.pdf.max_text_chars) out += `max_text_chars = ${config.pdf.max_text_chars}\n`
    out += `\n`
  }

  // 8. [email]
  if (config.email) {
    out += `[email]\n`
    out += `smtp_server = ${formatValue(config.email.smtp_server || '')}\n`
    out += `smtp_port = ${config.email.smtp_port || 465}\n`
    out += `sender = ${formatValue(config.email.sender || '')}\n`
    out += `receiver = ${formatValue(config.email.receiver || '')}\n`
    out += `password = ${formatValue(config.email.password || '')}\n`
    out += `\n`
  }

  // 9. [web]
  if (config.web) {
    out += `[web]\n`
    if (config.web.port) out += `port = ${config.web.port}\n`
    if (config.web.ui_path) out += `ui_path = ${formatValue(config.web.ui_path)}\n`
    out += `\n`
  }

  return out.trim() + '\n'
}
