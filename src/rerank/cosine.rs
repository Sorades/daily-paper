/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

/// Compute weighted average of top-k similarities.
///
/// For each candidate, computes cosine similarity against all library items,
/// takes the top-k by similarity, and returns the weighted average score.
pub fn weighted_top_k_score(
    candidate: &[f32],
    library_items: &[(Vec<f32>, f32)], // (embedding, weight)
    top_k: usize,
) -> f32 {
    if library_items.is_empty() || top_k == 0 {
        return 0.0;
    }

    let mut sims: Vec<(f32, f32)> = library_items
        .iter()
        .map(|(embedding, weight)| {
            let sim = cosine_similarity(candidate, embedding);
            (sim, *weight)
        })
        .collect();

    // Sort by similarity descending
    sims.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    // Take top-k
    let top: Vec<(f32, f32)> = sims.into_iter().take(top_k).collect();

    if top.is_empty() {
        return 0.0;
    }

    // Weighted average: sum(sim * weight) / sum(weight)
    let weighted_sum: f32 = top.iter().map(|(sim, weight)| sim * weight).sum();
    let weight_sum: f32 = top.iter().map(|(_, weight)| weight).sum();

    if weight_sum == 0.0 {
        0.0
    } else {
        weighted_sum / weight_sum
    }
}

/// Ranked paper with score and matched library items.
#[derive(Debug, Clone)]
pub struct RankedPaper {
    pub paper_id: String,
    pub rank: usize,
    pub score: f32,
    pub matched_library_items: Vec<MatchedLibraryItem>,
}

#[derive(Debug, Clone)]
pub struct MatchedLibraryItem {
    pub library_id: String,
    pub similarity: f32,
    pub weight: f32,
}

/// Rerank candidate papers against the library.
pub fn rerank(
    candidates: &[(String, Vec<f32>)],         // (paper_id, embedding)
    library_items: &[(String, Vec<f32>, f32)], // (library_id, embedding, weight)
    top_k: usize,
) -> Vec<RankedPaper> {
    let lib_refs: Vec<(Vec<f32>, f32)> = library_items
        .iter()
        .map(|(_, emb, weight)| (emb.clone(), *weight))
        .collect();

    let mut ranked: Vec<RankedPaper> = candidates
        .iter()
        .map(|(paper_id, candidate_emb)| {
            let score = weighted_top_k_score(candidate_emb, &lib_refs, top_k);

            // Compute individual similarities for matched items
            let mut matches: Vec<MatchedLibraryItem> = library_items
                .iter()
                .map(|(lib_id, lib_emb, weight)| MatchedLibraryItem {
                    library_id: lib_id.clone(),
                    similarity: cosine_similarity(candidate_emb, lib_emb),
                    weight: *weight,
                })
                .collect();

            matches.sort_by(|a, b| {
                b.similarity
                    .partial_cmp(&a.similarity)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            matches.truncate(top_k);

            RankedPaper {
                paper_id: paper_id.clone(),
                rank: 0,
                score,
                matched_library_items: matches,
            }
        })
        .collect();

    // Sort by score descending and assign ranks
    ranked.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    for (i, paper) in ranked.iter_mut().enumerate() {
        paper.rank = i + 1;
    }

    ranked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        assert!((cosine_similarity(&a, &b) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!((cosine_similarity(&a, &b) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![-1.0, 0.0];
        assert!((cosine_similarity(&a, &b) - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_empty() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
    }

    #[test]
    fn cosine_similarity_different_lengths() {
        assert_eq!(cosine_similarity(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn weighted_top_k_basic() {
        let library = vec![
            (vec![1.0, 0.0], 1.0), // identical
            (vec![0.0, 1.0], 2.0), // orthogonal
        ];
        let candidate = vec![1.0, 0.0];
        let score = weighted_top_k_score(&candidate, &library, 2);
        // sim=[1.0, 0.0], weights=[1.0, 2.0]
        // weighted_avg = (1.0*1.0 + 0.0*2.0) / (1.0+2.0) = 0.333
        assert!((score - 0.333).abs() < 0.01);
    }

    #[test]
    fn weighted_top_k_respects_k() {
        let library = vec![
            (vec![1.0, 0.0], 1.0), // sim=1.0
            (vec![0.7, 0.7], 1.0), // sim≈0.707
            (vec![0.0, 1.0], 1.0), // sim=0.0
        ];
        let candidate = vec![1.0, 0.0];

        let score_k1 = weighted_top_k_score(&candidate, &library, 1);
        let score_k2 = weighted_top_k_score(&candidate, &library, 2);
        let score_k3 = weighted_top_k_score(&candidate, &library, 3);

        assert!((score_k1 - 1.0).abs() < 0.01);
        // k=2 includes 0.707 sim, lowering the weighted average
        assert!(score_k2 < score_k1);
        // k=3 adds 0.0 sim, further lowering
        assert!(score_k3 < score_k2);
    }

    #[test]
    fn rerank_basic() {
        let candidates = vec![("p1".into(), vec![1.0, 0.0]), ("p2".into(), vec![0.0, 1.0])];
        let library = vec![("lib1".into(), vec![1.0, 0.0], 1.0)];

        let ranked = rerank(&candidates, &library, 1);
        assert_eq!(ranked[0].paper_id, "p1");
        assert_eq!(ranked[0].rank, 1);
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn rerank_assigns_ranks() {
        let candidates = vec![
            ("low".into(), vec![0.0, 1.0]),
            ("high".into(), vec![1.0, 0.0]),
            ("mid".into(), vec![0.7, 0.7]),
        ];
        let library = vec![("lib1".into(), vec![1.0, 0.0], 1.0)];

        let ranked = rerank(&candidates, &library, 1);
        assert_eq!(ranked[0].paper_id, "high");
        assert_eq!(ranked[0].rank, 1);
        assert_eq!(ranked[1].paper_id, "mid");
        assert_eq!(ranked[1].rank, 2);
        assert_eq!(ranked[2].paper_id, "low");
        assert_eq!(ranked[2].rank, 3);
    }
}
