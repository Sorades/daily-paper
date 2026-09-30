use std::path::Path;
use std::sync::Arc;
use anyhow::Context;
use chrono::Utc;
use futures_util::stream::{self, StreamExt};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::config::ResolvedConfig;
use crate::metadata::fetcher::extract_from_source;
use crate::models::candidate::CandidatePaper;
use crate::models::read::{
    compute_read_cache_key, AuthorAffiliation, PaperMetadataSummary, ReadResult, TokenUsage,
};
use crate::models::run::*;
use crate::pdf::download::download_pdf;
use crate::pdf::extract::extract_text;
use crate::pdf::section::{parse_sections, select_sections_for_reading};
use crate::reader::openai::ReaderClient;
use crate::reader::template::{
    build_system_prompt, build_tldr_system_prompt, build_tldr_user_prompt, build_user_prompt,
    compute_template_hash, is_valid_structured_summary, parse_llm_output, parse_tldr_output,
    trim_to_token_budget,
};
use crate::state::keys::StorageKey;
use crate::state::path::StatePath;
use crate::state::store::FileStateStore;

use crate::progress::StageProgress;

use super::context::{mark_stage_blocked, record_stage, ExecutionContext};

pub(crate) async fn stage_deep_read(
    cx: &mut ExecutionContext<'_>,
    selected_ids: &[String],
    candidates: &[CandidatePaper],
    force_read: bool,
) -> anyhow::Result<Vec<ReadResult>> {
    let date = cx.date().to_string();
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::DeepRead,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let reader = Arc::new(ReaderClient::new(
        cx.config.reader.base_url.clone(),
        cx.config.reader.api_key.clone(),
        cx.config.reader.model.clone(),
        cx.config.reader.timeout_secs,
        cx.config.reader.max_retries,
        cx.config.reader.max_concurrency,
        cx.config.reader.max_input_tokens,
    ));

    let template_hash = Arc::new(compute_template_hash(&build_system_prompt(
        cx.config.reader.system_prompt_path.as_deref(),
    )?));

    let paper_dir_path = StorageKey::PapersDir.to_state_path()?;
    let paper_dir = paper_dir_path.resolve(cx.store.root());
    std::fs::create_dir_all(&paper_dir)?;

    let progress = Arc::new(StageProgress::new(
        StageName::DeepRead,
        selected_ids.len(),
        cx.event_tx,
        cx.run_id,
    ));

    let max_attempts = if cx.config.reader.on_read_failure == "retry" {
        3
    } else {
        1
    };

    let max_concurrency = cx.config.reader.max_concurrency.max(1);
    info!(concurrency = max_concurrency, papers = selected_ids.len(), "running deep read with controlled concurrency");

    let blocked_messages = Arc::new(Mutex::new(Vec::<String>::new()));
    let store_root = cx.store.root().to_path_buf();
    let config_arc = Arc::new(cx.config.clone());
    let date_owned = date.clone();

    let items: Vec<(usize, String, Option<CandidatePaper>)> = selected_ids
        .iter()
        .enumerate()
        .map(|(idx, id)| {
            let candidate = candidates.iter().find(|c| &c.paper_id == id).cloned();
            (idx, id.clone(), candidate)
        })
        .collect();

    let processed = stream::iter(items)
        .map(|(idx, paper_id, candidate)| {
            let store = FileStateStore::new(store_root.clone());
            let config = Arc::clone(&config_arc);
            let reader = Arc::clone(&reader);
            let template_hash = Arc::clone(&template_hash);
            let progress = Arc::clone(&progress);
            let blocked_messages = Arc::clone(&blocked_messages);
            let date = date_owned.clone();
            let paper_dir = paper_dir.clone();

            async move {
                let candidate = match candidate {
                    Some(c) => c,
                    None => {
                        warn!(paper_id = %paper_id, "selected paper not in candidates");
                        return (idx, None);
                    }
                };

                let res = process_single_paper(
                    &store,
                    &config,
                    &reader,
                    &template_hash,
                    &paper_id,
                    &candidate,
                    force_read,
                    &date,
                    &paper_dir,
                    max_attempts,
                    &progress,
                    &blocked_messages,
                )
                .await;

                (idx, res)
            }
        })
        .buffer_unordered(max_concurrency)
        .collect::<Vec<_>>()
        .await;

    // Collect blocked warnings into manifest
    for msg in blocked_messages.lock().await.iter() {
        mark_stage_blocked(cx.manifest, msg);
    }

    // Preserve the original ranking order of selected_ids
    let mut ordered = processed;
    ordered.sort_by_key(|(idx, _)| *idx);

    let mut read_results = Vec::new();
    let mut any_failure = false;
    for (_, res) in ordered {
        match res {
            Some(r) => read_results.push(r),
            None => any_failure = true,
        }
    }

    if any_failure && cx.config.reader.on_read_failure == "block" {
        warn!("some papers failed deep read; pipeline will be blocked");
    }

    info!(
        total = selected_ids.len(),
        succeeded = read_results.len(),
        "deep read complete"
    );
    progress.finish(&format!(
        "{}/{} succeeded",
        read_results.len(),
        selected_ids.len()
    ));

    let paper_ids_str = read_results
        .iter()
        .map(|r| r.paper_id.as_str())
        .collect::<Vec<_>>()
        .join(",");

    record.status = if read_results.len() == selected_ids.len() {
        StageStatus::Succeeded
    } else {
        StageStatus::Failed
    };
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(paper_ids_str);
    record_stage(cx.manifest, record);

    Ok(read_results)
}

#[allow(clippy::too_many_arguments)]
async fn process_single_paper(
    store: &FileStateStore,
    config: &ResolvedConfig,
    reader: &ReaderClient,
    template_hash: &str,
    paper_id: &str,
    candidate: &CandidatePaper,
    force_read: bool,
    date: &str,
    paper_dir: &Path,
    max_attempts: usize,
    progress: &StageProgress,
    blocked_messages: &Mutex<Vec<String>>,
) -> Option<ReadResult> {
    let pdf_dir = paper_dir.join(paper_id);
    let read_cache_path = find_cached_read_result(
        store,
        paper_id,
        template_hash,
        &config.reader.model,
        &config.reader.language,
    )
    .ok()?;

    if !force_read {
        if let Some(ref path) = read_cache_path {
            if let Ok(Some(result)) = store.read_json::<ReadResult>(&StatePath::new(path).ok()?) {
                info!(paper_id = %paper_id, "using cached read result");
                progress.inc(&format!("{} (cached)", paper_id));
                return Some(result);
            }
        }
    }

    let pdf_url = match candidate.pdf_url.as_deref() {
        Some(url) => url,
        None => {
            warn!(paper_id = %paper_id, "no PDF URL for paper");
            if config.reader.require_full_text {
                blocked_messages
                    .lock()
                    .await
                    .push(format!("no PDF URL for {} and require_full_text is enabled", paper_id));
                return None;
            }
            // If require_full_text is false, generate fallback TLDR directly from title/abstract
            return generate_abstract_only_read_result(
                store,
                config,
                reader,
                template_hash,
                paper_id,
                candidate,
                date,
                progress,
                blocked_messages,
            )
            .await;
        }
    };

    for attempt in 1..=max_attempts {
        if attempt > 1 {
            warn!(
                paper_id = %paper_id,
                attempt,
                max = max_attempts,
                "retrying paper"
            );
            progress.inc(&format!(
                "{} (retry {}/{})",
                paper_id, attempt, max_attempts
            ));
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        } else {
            info!(paper_id = %paper_id, title = %candidate.title, "deep reading");
            progress.inc(&format!("{} - {}", paper_id, &candidate.title));
        }

        let pdf_asset = match download_pdf(
            paper_id,
            pdf_url,
            &pdf_dir,
            config.pdf.timeout_secs,
            config.pdf.max_pdf_mb,
        )
        .await
        {
            Ok(a) => a,
            Err(e) => {
                warn!(paper_id = %paper_id, attempt, error = %e, "PDF download failed");
                if attempt == max_attempts {
                    blocked_messages
                        .lock()
                        .await
                        .push(format!("PDF download failed for {}: {}", paper_id, e));
                }
                continue;
            }
        };

        let extracted = match extract_text(
            paper_id,
            Path::new(&pdf_asset.file_path),
            &pdf_asset.sha256,
            &pdf_dir,
            config.pdf.timeout_secs,
            config.pdf.max_text_chars,
        )
        .await
        {
            Ok(e) => e,
            Err(e) => {
                warn!(paper_id = %paper_id, attempt, error = %e, "PDF extract failed");
                if attempt == max_attempts {
                    blocked_messages
                        .lock()
                        .await
                        .push(format!("PDF extract failed for {}: {}", paper_id, e));
                }
                continue;
            }
        };

        let metadata = extract_from_source(paper_id, &candidate.source_metadata);

        let full_text =
            std::fs::read_to_string(Path::new(&extracted.text_path)).unwrap_or_default();
        let sections = parse_sections(&full_text);
        let selected_text = select_sections_for_reading(
            &full_text,
            &sections,
            config.reader.max_input_tokens * 3,
        );

        let authors_str = candidate
            .authors
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        let user_prompt = build_user_prompt(
            &candidate.title,
            &candidate.abstract_text,
            &authors_str,
            &selected_text,
            &config.reader.language,
        );
        let system_prompt = match build_system_prompt(config.reader.system_prompt_path.as_deref()) {
            Ok(s) => s,
            Err(e) => {
                warn!(paper_id = %paper_id, error = %e, "failed to build system prompt");
                return None;
            }
        };

        let trimmed_prompt =
            trim_to_token_budget(&user_prompt, config.reader.max_input_tokens * 4);

        let structured_result = match reader.complete(&system_prompt, &trimmed_prompt).await {
            Ok((raw_response, token_usage)) => {
                match parse_llm_output(&raw_response) {
                    Ok(parsed) => Some((parsed, token_usage)),
                    Err(e) => {
                        warn!(
                            paper_id = %paper_id,
                            attempt,
                            error = %e,
                            "LLM read returned invalid report format"
                        );
                        if attempt < max_attempts {
                            continue;
                        }
                        None
                    }
                }
            }
            Err(e) => {
                warn!(paper_id = %paper_id, attempt, error = %e, "LLM read failed");
                if attempt < max_attempts {
                    continue;
                }
                None
            }
        };

        let (
            summary,
            author_affiliations,
            llm_project_url,
            llm_code_url,
            token_usage,
            warnings,
        ) = if let Some((parsed, token_usage)) = structured_result {
            let author_affiliations = parsed
                .author_affiliations
                .iter()
                .map(|aa| AuthorAffiliation {
                    name: aa.name.clone(),
                    affiliation: aa.affiliation.clone(),
                })
                .collect();
            (
                parsed.summary.clone(),
                author_affiliations,
                parsed.project_url.clone(),
                parsed.code_url.clone(),
                token_usage,
                Vec::new(),
            )
        } else {
            match generate_tldr_fallback(
                reader,
                candidate,
                &authors_str,
                &selected_text,
                &config.reader.language,
                config.reader.max_input_tokens,
                paper_id,
            )
            .await
            {
                Ok((summary, token_usage)) => {
                    warn!(paper_id = %paper_id, "using TLDR fallback for deep read report");
                    (
                        summary,
                        Vec::new(),
                        None,
                        None,
                        token_usage,
                        vec!["structured_read_failed; used_tldr_fallback".to_string()],
                    )
                }
                Err(e) => {
                    blocked_messages.lock().await.push(format!(
                        "LLM read failed for {}; TLDR fallback also failed: {}",
                        paper_id, e
                    ));
                    continue;
                }
            }
        };

        let read_cache_key = compute_read_cache_key(
            paper_id,
            &pdf_asset.sha256,
            &extracted.text_extract_key,
            &metadata.metadata_key,
            template_hash,
            &config.reader.model,
            &config.reader.language,
        );

        let read_result = ReadResult {
            paper_id: paper_id.to_string(),
            cache_key: read_cache_key.clone(),
            generated_at: Utc::now(),
            model_id: config.reader.model.clone(),
            reader_template_hash: template_hash.to_string(),
            language: config.reader.language.clone(),
            summary,
            metadata: PaperMetadataSummary {
                institutions: metadata.institutions.clone(),
                notable_authors: metadata.notable_authors.clone(),
                project_url: llm_project_url.or_else(|| metadata.project_url.clone()),
                code_url: llm_code_url.or_else(|| metadata.code_url.clone()),
            },
            author_affiliations,
            token_usage,
            warnings,
        };

        let _ = store.put_json(
            &StorageKey::PaperReadCache {
                paper_id,
                cache_key: &read_cache_key,
            },
            &read_result,
        );
        let _ = store.put_json(&StorageKey::ArchiveReadResult { date, paper_id }, &read_result);

        return Some(read_result);
    }

    None
}

async fn generate_tldr_fallback(
    reader: &ReaderClient,
    candidate: &CandidatePaper,
    authors: &str,
    selected_text: &str,
    language: &str,
    max_input_tokens: usize,
    paper_id: &str,
) -> anyhow::Result<(String, Option<TokenUsage>)> {
    let system_prompt = build_tldr_system_prompt();
    let user_prompt = build_tldr_user_prompt(
        &candidate.title,
        &candidate.abstract_text,
        authors,
        selected_text,
        language,
    );
    let trimmed_prompt = trim_to_token_budget(&user_prompt, max_input_tokens * 4);
    let (raw_response, token_usage) = reader
        .complete(&system_prompt, &trimmed_prompt)
        .await
        .with_context(|| format!("TLDR fallback LLM call failed for {}", paper_id))?;
    let summary = parse_tldr_output(&raw_response)
        .map_err(|e| anyhow::anyhow!("TLDR fallback returned invalid format: {}", e))?;
    Ok((summary, token_usage))
}

fn find_cached_read_result(
    store: &FileStateStore,
    paper_id: &str,
    template_hash: &str,
    model_id: &str,
    language: &str,
) -> anyhow::Result<Option<String>> {
    let dir_key = StorageKey::PaperReadCacheDir { paper_id };
    let full_dir = store.resolve_key(&dir_key)?;
    if !full_dir.exists() {
        return Ok(None);
    }

    for entry in std::fs::read_dir(&full_dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().map(|e| e == "json").unwrap_or(false) {
            let Some(file_name) = path.file_name() else {
                continue;
            };
            let cache_file = file_name.to_string_lossy();
            let cache_key = cache_file.trim_end_matches(".json");
            let key = StorageKey::PaperReadCache { paper_id, cache_key };
            let state_path = key.to_state_path()?;
            if let Some(result) = store.read_json::<ReadResult>(&state_path)? {
                if result.reader_template_hash == template_hash
                    && result.model_id == model_id
                    && result.language == language
                {
                    if is_valid_structured_summary(&result.summary) {
                        return Ok(Some(state_path.as_path().to_string_lossy().to_string()));
                    }
                    warn!(
                        paper_id = %paper_id,
                        path = %state_path.as_path().display(),
                        "ignoring cached read result with invalid summary format"
                    );
                }
            }
        }
    }
    Ok(None)
}

async fn generate_abstract_only_read_result(
    store: &FileStateStore,
    config: &ResolvedConfig,
    reader: &ReaderClient,
    template_hash: &str,
    paper_id: &str,
    candidate: &CandidatePaper,
    date: &str,
    progress: &StageProgress,
    blocked_messages: &Mutex<Vec<String>>,
) -> Option<ReadResult> {
    info!(paper_id = %paper_id, title = %candidate.title, "generating abstract-only read result");
    progress.inc(&format!("{} - {} (abstract only)", paper_id, &candidate.title));

    let authors_str = candidate
        .authors
        .iter()
        .map(|a| a.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    match generate_tldr_fallback(
        reader,
        candidate,
        &authors_str,
        "",
        &config.reader.language,
        config.reader.max_input_tokens,
        paper_id,
    )
    .await
    {
        Ok((summary, token_usage)) => {
            let metadata = extract_from_source(paper_id, &candidate.source_metadata);
            let read_cache_key = compute_read_cache_key(
                paper_id,
                "none",
                "none",
                &metadata.metadata_key,
                template_hash,
                &config.reader.model,
                &config.reader.language,
            );

            let read_result = ReadResult {
                paper_id: paper_id.to_string(),
                cache_key: read_cache_key.clone(),
                generated_at: Utc::now(),
                model_id: config.reader.model.clone(),
                reader_template_hash: template_hash.to_string(),
                language: config.reader.language.clone(),
                summary,
                metadata: PaperMetadataSummary {
                    institutions: metadata.institutions.clone(),
                    notable_authors: metadata.notable_authors.clone(),
                    project_url: metadata.project_url.clone(),
                    code_url: metadata.code_url.clone(),
                },
                author_affiliations: Vec::new(),
                token_usage,
                warnings: vec!["abstract_only_read; require_full_text_disabled".to_string()],
            };

            let _ = store.put_json(
                &StorageKey::PaperReadCache {
                    paper_id,
                    cache_key: &read_cache_key,
                },
                &read_result,
            );
            let _ = store.put_json(&StorageKey::ArchiveReadResult { date, paper_id }, &read_result);

            Some(read_result)
        }
        Err(e) => {
            blocked_messages.lock().await.push(format!(
                "abstract-only LLM read failed for {}: {}",
                paper_id, e
            ));
            None
        }
    }
}
