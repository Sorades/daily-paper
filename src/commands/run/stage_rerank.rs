use chrono::Utc;
use tracing::{debug, info};

use crate::models::common::sha256_hex;
use crate::models::run::*;
use crate::models::zotero::ZoteroSnapshot;
use crate::rerank::cosine::rerank;
use crate::rerank::selection::{
    compute_candidate_set_hash, compute_rerank_cache_key, select_top_n, ReadSelection,
};
use crate::state::keys::StorageKey;

use super::context::{record_stage, ExecutionContext};
use super::stage_embedding::build_interest_profile;

pub(crate) async fn stage_rerank(
    cx: &mut ExecutionContext<'_>,
    candidate_embs: &[(String, Vec<f32>)],
    library_embs: &[(String, Vec<f32>, f32)],
    snapshot: &ZoteroSnapshot,
) -> anyhow::Result<(ReadSelection, Vec<(String, f32)>)> {
    let date = cx.date().to_string();
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Rerank,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let ranked = rerank(
        candidate_embs,
        library_embs,
        cx.config.reranker.top_k_library_matches,
    );

    let ranked_ids: Vec<String> = ranked.iter().map(|r| r.paper_id.clone()).collect();
    let candidate_set_hash = compute_candidate_set_hash(&ranked_ids);

    let profile = build_interest_profile(snapshot, &cx.config.zotero.filters);

    let rerank_cache_key = compute_rerank_cache_key(
        &profile.profile_id,
        &candidate_set_hash,
        &cx.config.embedding.model,
        &sha256_hex(format!("top_k={}", cx.config.reranker.top_k_library_matches).as_bytes()),
    );

    let paper_scores: Vec<(String, f32)> = ranked
        .iter()
        .map(|r| (r.paper_id.clone(), r.score))
        .collect();

    let selection = select_top_n(
        &rerank_cache_key,
        &ranked_ids,
        cx.config.reader.top_n,
        paper_scores,
    );

    let scores = selection.paper_scores.clone();

    info!(
        ranked = ranked.len(),
        selected = selection.selected_paper_ids.len(),
        "rerank complete"
    );

    for (i, paper_id) in selection.selected_paper_ids.iter().enumerate() {
        if let Some(r) = ranked.iter().find(|r| &r.paper_id == paper_id) {
            debug!(rank = i + 1, paper_id = %paper_id, score = r.score, "selected paper");
        }
    }

    cx.store.put_json(&StorageKey::RerankSelection { selection_id: &selection.selection_id }, &selection)?;
    cx.store.put_json(&StorageKey::ArchiveRerank { date: &date }, &selection)?;

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(selection.selection_id.clone());
    record_stage(cx.manifest, record);

    Ok((selection, scores))
}
