use super::raw::RawConfig;
use crate::error::{Error, Result};

/// Validate required fields in the raw config.
pub fn validate(raw: &RawConfig) -> Result<()> {
    if raw.zotero.user_id.is_empty() {
        return Err(Error::Config("zotero.user_id is required".into()));
    }
    if raw.zotero.api_key.is_empty() {
        return Err(Error::Config("zotero.api_key is required".into()));
    }
    if raw.sources.is_empty() {
        return Err(Error::Config("at least one source is required".into()));
    }
    for source in &raw.sources {
        if source.kind.trim().is_empty() {
            return Err(Error::Config("source.kind is required".into()));
        }
        if source.kind == "arxiv" {
            let backend = source.backend.as_deref().unwrap_or("").trim();
            if backend.is_empty() {
                return Err(Error::Config(
                    "sources.backend is required for arxiv sources; use \"rss\" or \"export\""
                        .into(),
                ));
            }
            if backend != "rss" && backend != "export" {
                return Err(Error::Config(format!(
                    "unsupported arxiv backend '{}'; expected \"rss\" or \"export\"",
                    backend
                )));
            }
            if source.categories.is_empty() {
                return Err(Error::Config(
                    "sources.categories must not be empty for arxiv sources".into(),
                ));
            }
            if let Some(max_results_per_page) = source.max_results_per_page {
                if max_results_per_page == 0 || max_results_per_page > 2000 {
                    return Err(Error::Config(
                        "sources.max_results_per_page must be between 1 and 2000".into(),
                    ));
                }
            }
            if let Some(max_pages) = source.max_pages {
                if max_pages == 0 {
                    return Err(Error::Config("sources.max_pages must be at least 1".into()));
                }
            }
        }
    }
    if raw.embedding.kind != "fastembed" {
        if raw.embedding.base_url.as_deref().unwrap_or("").is_empty() {
            return Err(Error::Config(
                "embedding.base_url is required for non-local embedding".into(),
            ));
        }
        if raw.embedding.api_key.as_deref().unwrap_or("").is_empty() {
            return Err(Error::Config(
                "embedding.api_key is required for non-local embedding".into(),
            ));
        }
    }
    if raw.reader.base_url.is_empty() {
        return Err(Error::Config("reader.base_url is required".into()));
    }
    if raw.reader.api_key.is_empty() {
        return Err(Error::Config("reader.api_key is required".into()));
    }
    if raw.reader.model.is_empty() {
        return Err(Error::Config("reader.model is required".into()));
    }
    if raw.email.smtp_server.is_empty() {
        return Err(Error::Config("email.smtp_server is required".into()));
    }
    if raw.email.sender.is_empty() {
        return Err(Error::Config("email.sender is required".into()));
    }
    if raw.email.receiver.is_empty() {
        return Err(Error::Config("email.receiver is required".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::raw::*;

    fn valid_raw() -> RawConfig {
        RawConfig {
            zotero: ZoteroConfig {
                user_id: "12345".into(),
                api_key: "key".into(),
                max_snapshot_age_hours: None,
                filters: None,
            },
            sources: vec![SourceConfig {
                kind: "arxiv".into(),
                backend: Some("rss".into()),
                categories: vec!["cs.AI".into()],
                include_cross_list: None,
                max_results_per_page: None,
                max_pages: None,
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
            schedule: None,
        }
    }

    #[test]
    fn valid_config_passes() {
        assert!(validate(&valid_raw()).is_ok());
    }

    #[test]
    fn empty_user_id_fails() {
        let mut raw = valid_raw();
        raw.zotero.user_id = String::new();
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn empty_sources_fails() {
        let mut raw = valid_raw();
        raw.sources = vec![];
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn arxiv_source_requires_backend() {
        let mut raw = valid_raw();
        raw.sources[0].backend = None;
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn arxiv_source_rejects_unknown_backend() {
        let mut raw = valid_raw();
        raw.sources[0].backend = Some("unknown".into());
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn arxiv_export_accepts_page_defaults() {
        let mut raw = valid_raw();
        raw.sources[0].backend = Some("export".into());
        assert!(validate(&raw).is_ok());
    }

    #[test]
    fn non_fastembed_missing_base_url_fails() {
        let mut raw = valid_raw();
        raw.embedding.kind = "openai".into();
        raw.embedding.base_url = None;
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn non_fastembed_missing_api_key_fails() {
        let mut raw = valid_raw();
        raw.embedding.kind = "openai".into();
        raw.embedding.base_url = Some("http://localhost".into());
        raw.embedding.api_key = None;
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn empty_reader_base_url_fails() {
        let mut raw = valid_raw();
        raw.reader.base_url = String::new();
        assert!(validate(&raw).is_err());
    }

    #[test]
    fn empty_smtp_server_fails() {
        let mut raw = valid_raw();
        raw.email.smtp_server = String::new();
        assert!(validate(&raw).is_err());
    }
}
