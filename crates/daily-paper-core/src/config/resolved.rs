use super::raw::RawConfig;
use crate::error::Result;

#[derive(Debug, Clone)]
pub struct ResolvedConfig {
    pub zotero: ResolvedZoteroConfig,
    pub sources: Vec<ResolvedSourceConfig>,
    pub embedding: ResolvedEmbeddingConfig,
    pub reranker: ResolvedRerankerConfig,
    pub reader: ResolvedReaderConfig,
    pub pdf: ResolvedPdfConfig,
    pub email: ResolvedEmailConfig,
    pub web: ResolvedWebConfig,
}

#[derive(Debug, Clone)]
pub struct ResolvedZoteroConfig {
    pub user_id: String,
    pub api_key: String,
    pub max_snapshot_age_hours: u64,
    pub filters: Vec<ResolvedZoteroFilter>,
}

#[derive(Debug, Clone)]
pub struct ResolvedZoteroFilter {
    pub path: String,
    pub weight: f32,
    pub exclude: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedSourceConfig {
    pub kind: String,
    pub categories: Vec<String>,
    pub include_cross_list: bool,
}

#[derive(Debug, Clone)]
pub struct ResolvedEmbeddingConfig {
    pub kind: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    pub model: String,
    pub batch_size: usize,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub max_concurrency: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedRerankerConfig {
    pub kind: String,
    pub top_k_library_matches: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedReaderConfig {
    pub kind: String,
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub top_n: usize,
    pub language: String,
    pub require_full_text: bool,
    pub on_read_failure: String,
    pub timeout_secs: u64,
    pub max_retries: u32,
    pub max_concurrency: usize,
    pub max_input_tokens: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedPdfConfig {
    pub extractor: String,
    pub timeout_secs: u64,
    pub max_pdf_mb: u64,
    pub max_text_chars: usize,
}

#[derive(Debug, Clone)]
pub struct ResolvedEmailConfig {
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender: String,
    pub receiver: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedWebConfig {
    pub port: u16,
    pub ui_path: String,
}

impl ResolvedConfig {
    pub fn from_raw(raw: &RawConfig) -> Result<Self> {
        // Validate required fields
        super::validation::validate(raw)?;

        Ok(Self {
            zotero: ResolvedZoteroConfig {
                user_id: raw.zotero.user_id.clone(),
                api_key: raw.zotero.api_key.clone(),
                max_snapshot_age_hours: raw.zotero.max_snapshot_age_hours.unwrap_or(168),
                filters: raw
                    .zotero
                    .filters
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .map(|f| ResolvedZoteroFilter {
                        path: f.path.clone(),
                        weight: f.weight.unwrap_or(1.0),
                        exclude: f.exclude.unwrap_or(false),
                    })
                    .collect(),
            },
            sources: raw
                .sources
                .iter()
                .map(|s| ResolvedSourceConfig {
                    kind: s.kind.clone(),
                    categories: s.categories.clone(),
                    include_cross_list: s.include_cross_list.unwrap_or(false),
                })
                .collect(),
            embedding: ResolvedEmbeddingConfig {
                kind: raw.embedding.kind.clone(),
                base_url: raw.embedding.base_url.clone(),
                api_key: raw.embedding.api_key.clone(),
                model: raw.embedding.model.clone().unwrap_or_else(|| {
                    if raw.embedding.kind == "fastembed" {
                        "BAAI/bge-small-en-v1.5".to_string()
                    } else {
                        "text-embedding-3-small".to_string()
                    }
                }),
                batch_size: raw.embedding.batch_size.unwrap_or(64),
                timeout_secs: raw.embedding.timeout_secs.unwrap_or(60),
                max_retries: raw.embedding.max_retries.unwrap_or(5),
                max_concurrency: raw.embedding.max_concurrency.unwrap_or(4),
            },
            reranker: ResolvedRerankerConfig {
                kind: raw.reranker.kind.clone(),
                top_k_library_matches: raw.reranker.top_k_library_matches.unwrap_or(20),
            },
            reader: ResolvedReaderConfig {
                kind: raw.reader.kind.clone(),
                base_url: raw.reader.base_url.clone(),
                api_key: raw.reader.api_key.clone(),
                model: raw.reader.model.clone(),
                top_n: raw.reader.top_n.unwrap_or(10),
                language: raw
                    .reader
                    .language
                    .clone()
                    .unwrap_or_else(|| "zh-CN".into()),
                require_full_text: raw.reader.require_full_text.unwrap_or(true),
                on_read_failure: raw
                    .reader
                    .on_read_failure
                    .clone()
                    .unwrap_or_else(|| "retry".into()),
                timeout_secs: raw.reader.timeout_secs.unwrap_or(120),
                max_retries: raw.reader.max_retries.unwrap_or(5),
                max_concurrency: raw.reader.max_concurrency.unwrap_or(2),
                max_input_tokens: raw.reader.max_input_tokens.unwrap_or(60000),
            },
            pdf: ResolvedPdfConfig {
                extractor: raw.pdf.extractor.clone(),
                timeout_secs: raw.pdf.timeout_secs.unwrap_or(60),
                max_pdf_mb: raw.pdf.max_pdf_mb.unwrap_or(50),
                max_text_chars: raw.pdf.max_text_chars.unwrap_or(300000),
            },
            email: ResolvedEmailConfig {
                smtp_server: raw.email.smtp_server.clone(),
                smtp_port: raw.email.smtp_port,
                sender: raw.email.sender.clone(),
                receiver: raw.email.receiver.clone(),
                password: raw.email.password.clone(),
            },
            web: ResolvedWebConfig {
                port: raw.web.as_ref().and_then(|w| w.port).unwrap_or(8991),
                ui_path: raw
                    .web
                    .as_ref()
                    .and_then(|w| w.ui_path.clone())
                    .unwrap_or_else(|| "ui".to_string()),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::raw::*;

    fn minimal_raw() -> RawConfig {
        RawConfig {
            zotero: ZoteroConfig {
                user_id: "12345".into(),
                api_key: "key".into(),
                max_snapshot_age_hours: None,
                filters: None,
            },
            sources: vec![SourceConfig {
                kind: "arxiv".into(),
                categories: vec!["cs.AI".into()],
                include_cross_list: None,
            }],
            embedding: EmbeddingConfig {
                kind: "fastembed".into(),
                base_url: None,
                api_key: None,
                model: None,
                batch_size: None,
                timeout_secs: None,
                max_retries: None,
                max_concurrency: None,
            },
            reranker: RerankerConfig {
                kind: "cosine".into(),
                top_k_library_matches: None,
            },
            reader: ReaderConfig {
                kind: "openai".into(),
                base_url: "http://localhost:8080/v1".into(),
                api_key: "key".into(),
                model: "gpt-4o-mini".into(),
                top_n: None,
                language: None,
                require_full_text: None,
                on_read_failure: None,
                timeout_secs: None,
                max_retries: None,
                max_concurrency: None,
                max_input_tokens: None,
            },
            pdf: PdfConfig {
                extractor: "pdftotext".into(),
                timeout_secs: None,
                max_pdf_mb: None,
                max_text_chars: None,
            },
            email: EmailConfig {
                smtp_server: "smtp.example.com".into(),
                smtp_port: 587,
                sender: "a@b.com".into(),
                receiver: "c@d.com".into(),
                password: "pass".into(),
            },
            web: None,
        }
    }

    #[test]
    fn fastembed_default_model() {
        let cfg = ResolvedConfig::from_raw(&minimal_raw()).unwrap();
        assert_eq!(cfg.embedding.model, "BAAI/bge-small-en-v1.5");
    }

    #[test]
    fn openai_default_model() {
        let mut raw = minimal_raw();
        raw.embedding.kind = "openai".into();
        raw.embedding.base_url = Some("http://localhost".into());
        raw.embedding.api_key = Some("key".into());
        let cfg = ResolvedConfig::from_raw(&raw).unwrap();
        assert_eq!(cfg.embedding.model, "text-embedding-3-small");
    }

    #[test]
    fn on_read_failure_default_is_retry() {
        let cfg = ResolvedConfig::from_raw(&minimal_raw()).unwrap();
        assert_eq!(cfg.reader.on_read_failure, "retry");
    }

    #[test]
    fn reader_defaults() {
        let cfg = ResolvedConfig::from_raw(&minimal_raw()).unwrap();
        assert_eq!(cfg.reader.top_n, 10);
        assert_eq!(cfg.reader.language, "zh-CN");
        assert_eq!(cfg.reader.require_full_text, true);
        assert_eq!(cfg.reader.timeout_secs, 120);
        assert_eq!(cfg.reader.max_retries, 5);
        assert_eq!(cfg.reader.max_concurrency, 2);
        assert_eq!(cfg.reader.max_input_tokens, 60000);
    }

    #[test]
    fn web_defaults() {
        let cfg = ResolvedConfig::from_raw(&minimal_raw()).unwrap();
        assert_eq!(cfg.web.port, 8991);
        assert_eq!(cfg.web.ui_path, "ui");
    }
}
