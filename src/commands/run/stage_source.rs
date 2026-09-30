use std::collections::{HashMap, HashSet};
use anyhow::Context;
use chrono::Utc;
use tracing::{info, warn};

use crate::models::candidate::{normalize_arxiv_id, normalize_doi, CandidatePaper};
use crate::models::dedup::{DedupResult, DuplicateReason, DuplicateRecord, ExistingLibraryMatch};
use crate::models::run::*;
use crate::models::zotero::ZoteroSnapshot;
use crate::source::arxiv::client::{ArxivBackendKind, ArxivClient};
use crate::state::keys::StorageKey;

use super::context::{default_arxiv_announcement_date, record_stage, ExecutionContext};

pub(crate) async fn stage_source_fetch(
    cx: &mut ExecutionContext<'_>,
    date_explicit: bool,
    snapshot: &ZoteroSnapshot,
    max_candidates: Option<usize>,
) -> anyhow::Result<DedupResult> {
    let date = cx.date().to_string();
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::SourceFetch,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let mut all_candidates = Vec::new();

    let today = default_arxiv_announcement_date().to_string();
    let use_rss = !date_explicit || date == today;

    for source in &cx.config.sources {
        let source_impl: Box<dyn crate::source::PaperSource> = if source.kind == "arxiv" {
            let backend = if use_rss {
                ArxivBackendKind::Rss
            } else {
                ArxivBackendKind::Export
            };
            info!(
                categories = ?source.categories,
                include_cross_list = source.include_cross_list,
                backend = ?backend,
                "fetching from arXiv via PaperSource trait"
            );
            Box::new(ArxivClient::with_backend(
                backend,
                source.categories.clone(),
                source.include_cross_list,
                source.max_results_per_page,
                source.max_pages,
            ))
        } else {
            warn!(kind = %source.kind, "unsupported source kind, skipping");
            continue;
        };

        let fetch_ctx = crate::source::FetchContext {
            start: cx.date_window.start,
            end: cx.date_window.end,
            report_date: date.clone(),
            cursor: None,
        };

        let output = source_impl
            .fetch(&fetch_ctx)
            .await
            .context(format!("failed to fetch from source '{}'", source_impl.id()))?;

        all_candidates.extend(output.candidates);
    }

    let before_filter = all_candidates.len();
    let cutoff = cx.date_window.start - chrono::Duration::days(2);
    all_candidates.retain(|c| c.published_at.map(|p| p >= cutoff).unwrap_or(false));
    if all_candidates.len() < before_filter {
        info!(
            before = before_filter,
            after = all_candidates.len(),
            "filtered candidates by published_at (last 2 days)"
        );
    }

    all_candidates.sort_by(|a, b| a.paper_id.cmp(&b.paper_id));
    all_candidates.dedup_by(|a, b| a.paper_id == b.paper_id);

    info!(count = all_candidates.len(), "fetched candidate papers");

    cx.store.put_json(&StorageKey::ArchiveCandidates { date: &date }, &all_candidates)?;

    let mut dedup = deduplicate_candidates(&all_candidates, snapshot, cx.run_id);

    if let Some(max) = max_candidates {
        if dedup.candidates.len() > max {
            info!(
                limit = max,
                total = dedup.candidates.len(),
                "limiting candidates before caching dedup"
            );
            dedup.candidates.truncate(max);
        }
    }

    info!(
        input = all_candidates.len(),
        kept = dedup.candidates.len(),
        library_matches = dedup.skipped_existing.len(),
        within_dupes = dedup.duplicates.len(),
        "deduplication complete"
    );

    cx.store.put_json(&StorageKey::ArchiveDedup { date: &date }, &dedup)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record_stage(cx.manifest, record);

    Ok(dedup)
}

pub(crate) fn deduplicate_candidates(
    candidates: &[CandidatePaper],
    snapshot: &ZoteroSnapshot,
    run_id: &str,
) -> DedupResult {
    let mut kept: Vec<CandidatePaper> = Vec::new();
    let mut skipped_existing = Vec::new();
    let mut seen_dois: HashMap<String, usize> = HashMap::new();
    let mut seen_arxiv: HashMap<String, usize> = HashMap::new();
    let mut duplicates = Vec::new();

    let lib_dois: HashSet<String> = snapshot
        .items
        .iter()
        .filter_map(|p| p.doi.as_ref().map(|d| normalize_doi(d)))
        .collect();
    let lib_arxiv: HashSet<String> = snapshot
        .items
        .iter()
        .filter_map(|p| p.arxiv_id.as_ref().map(|a| normalize_arxiv_id(a)))
        .collect();

    for candidate in candidates {
        if let Some(ref doi) = candidate.doi {
            let norm = normalize_doi(doi);
            if lib_dois.contains(&norm) {
                if let Some(lib_paper) = snapshot.items.iter().find(|p| {
                    p.doi.as_ref().map(|d| normalize_doi(d)) == Some(norm.clone())
                }) {
                    skipped_existing.push(ExistingLibraryMatch {
                        candidate_paper_id: candidate.paper_id.clone(),
                        library_id: lib_paper.library_id.clone(),
                        reason: DuplicateReason::SameDoi,
                    });
                    continue;
                }
            }
        }

        if let Some(ref arxiv_id) = candidate.arxiv_id {
            let norm = normalize_arxiv_id(arxiv_id);
            if lib_arxiv.contains(&norm) {
                if let Some(lib_paper) = snapshot.items.iter().find(|p| {
                    p.arxiv_id.as_ref().map(|a| normalize_arxiv_id(a)) == Some(norm.clone())
                }) {
                    skipped_existing.push(ExistingLibraryMatch {
                        candidate_paper_id: candidate.paper_id.clone(),
                        library_id: lib_paper.library_id.clone(),
                        reason: DuplicateReason::SameArxivId,
                    });
                    continue;
                }
            }
        }

        let mut is_dup = false;
        if let Some(ref doi) = candidate.doi {
            let norm = normalize_doi(doi);
            if let Some(&existing_idx) = seen_dois.get(&norm) {
                duplicates.push(DuplicateRecord {
                    kept_paper_id: kept[existing_idx].paper_id.clone(),
                    dropped_paper_id: candidate.paper_id.clone(),
                    reason: DuplicateReason::SameDoi,
                });
                is_dup = true;
            } else {
                seen_dois.insert(norm, kept.len());
            }
        }
        if !is_dup {
            if let Some(ref arxiv_id) = candidate.arxiv_id {
                let norm = normalize_arxiv_id(arxiv_id);
                if let Some(&existing_idx) = seen_arxiv.get(&norm) {
                    duplicates.push(DuplicateRecord {
                        kept_paper_id: kept[existing_idx].paper_id.clone(),
                        dropped_paper_id: candidate.paper_id.clone(),
                        reason: DuplicateReason::SameArxivId,
                    });
                    is_dup = true;
                } else {
                    seen_arxiv.insert(norm, kept.len());
                }
            }
        }

        if !is_dup {
            kept.push(candidate.clone());
        }
    }

    DedupResult {
        run_id: run_id.to_string(),
        candidates: kept,
        duplicates,
        skipped_existing,
    }
}
