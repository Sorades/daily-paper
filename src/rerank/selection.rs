use serde::{Deserialize, Serialize};
use sha2::Digest;

/// Selection of top-N papers for deep reading.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadSelection {
    pub selection_id: String,
    pub rerank_cache_key: String,
    pub top_n: usize,
    pub selected_paper_ids: Vec<String>,
    /// Per-paper scores from reranking (paper_id, score). Optional for backward compat.
    #[serde(default)]
    pub paper_scores: Vec<(String, f32)>,
}

/// Compute the selection cache key.
pub fn compute_selection_id(rerank_cache_key: &str, top_n: usize) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(rerank_cache_key.as_bytes());
    hasher.update(top_n.to_le_bytes());
    hex::encode(hasher.finalize())
}

/// Select top-N papers from ranked results.
///
/// Papers are already sorted by rank (1 = best). This function takes the first N.
pub fn select_top_n(
    rerank_cache_key: &str,
    ranked_paper_ids: &[String],
    top_n: usize,
    paper_scores: Vec<(String, f32)>,
) -> ReadSelection {
    let selected: Vec<String> = ranked_paper_ids.iter().take(top_n).cloned().collect();
    let selection_id = compute_selection_id(rerank_cache_key, top_n);

    // Filter scores to only include selected papers
    let selected_scores: Vec<(String, f32)> = paper_scores
        .into_iter()
        .filter(|(id, _)| selected.contains(id))
        .collect();

    ReadSelection {
        selection_id,
        rerank_cache_key: rerank_cache_key.to_string(),
        top_n,
        selected_paper_ids: selected,
        paper_scores: selected_scores,
    }
}

/// Compute rerank cache key from its inputs.
pub fn compute_rerank_cache_key(
    interest_profile_id: &str,
    candidate_set_hash: &str,
    embedding_model_id: &str,
    reranker_config_hash: &str,
) -> String {
    let mut hasher = sha2::Sha256::new();
    hasher.update(interest_profile_id.as_bytes());
    hasher.update(candidate_set_hash.as_bytes());
    hasher.update(embedding_model_id.as_bytes());
    hasher.update(reranker_config_hash.as_bytes());
    hex::encode(hasher.finalize())
}

/// Compute the hash of the candidate paper ID set (order-independent).
pub fn compute_candidate_set_hash(paper_ids: &[String]) -> String {
    let mut hasher = sha2::Sha256::new();
    let mut sorted_ids: Vec<&str> = paper_ids.iter().map(|s| s.as_str()).collect();
    sorted_ids.sort();
    for id in sorted_ids {
        hasher.update(id.as_bytes());
    }
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_takes_top_n() {
        let ids = vec!["p1".into(), "p2".into(), "p3".into(), "p4".into()];
        let sel = select_top_n("rk1", &ids, 2, vec![]);
        assert_eq!(sel.selected_paper_ids, vec!["p1", "p2"]);
        assert_eq!(sel.top_n, 2);
    }

    #[test]
    fn selection_more_than_available() {
        let ids = vec!["p1".into()];
        let sel = select_top_n("rk1", &ids, 10, vec![]);
        assert_eq!(sel.selected_paper_ids, vec!["p1"]);
    }

    #[test]
    fn selection_deterministic() {
        let ids = vec!["p1".into(), "p2".into()];
        let sel1 = select_top_n("rk1", &ids, 2, vec![]);
        let sel2 = select_top_n("rk1", &ids, 2, vec![]);
        assert_eq!(sel1.selection_id, sel2.selection_id);
    }

    #[test]
    fn candidate_set_hash_order_independent() {
        let ids1 = vec!["a".into(), "b".into(), "c".into()];
        let ids2 = vec!["c".into(), "a".into(), "b".into()];
        assert_eq!(
            compute_candidate_set_hash(&ids1),
            compute_candidate_set_hash(&ids2)
        );
    }
}
