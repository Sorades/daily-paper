use super::path::StatePath;
use crate::error::Result;

/// Type-safe storage key that maps to exact canonical relative paths within the state store.
/// Ensures 100% backward compatibility with existing disk storage layouts while eliminating
/// ad-hoc string formatting errors across the codebase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageKey<'a> {
    // ── Run Manifests ──
    RunManifest { run_id: &'a str },
    RunDir { run_id: &'a str },

    // ── Zotero State & Snapshots ──
    ZoteroSyncState,
    ZoteroSnapshot { snapshot_id: &'a str },

    // ── Embeddings ──
    EmbeddingVector { hash: &'a str },

    // ── Rerank Selections ──
    RerankSelection { selection_id: &'a str },

    // ── Papers & Read Results ──
    PapersDir,
    PaperDir { paper_id: &'a str },
    PaperReadCacheDir { paper_id: &'a str },
    PaperReadCache { paper_id: &'a str, cache_key: &'a str },
    PaperPdf { paper_id: &'a str },
    PaperExtractedText { paper_id: &'a str },

    // ── Runs & Snapshots Collections ──
    RunsDir,
    ZoteroSnapshotsDir,
    ArchiveReadDir { date: &'a str },

    // ── Reports ──
    RunReportHtml { run_id: &'a str },
    RunReportText { run_id: &'a str },
    RunReportMeta { run_id: &'a str },
    RunReportDir { run_id: &'a str },

    // ── Deliveries ──
    DeliveryReceipt { delivery_key: &'a str },

    // ── Archive (Date-Specific) ──
    ArchiveSnapshotId { date: &'a str },
    ArchiveCandidates { date: &'a str },
    ArchiveDedup { date: &'a str },
    ArchiveEmbeddings { date: &'a str },
    ArchiveRerank { date: &'a str },
    ArchiveReadResult { date: &'a str, paper_id: &'a str },
    ArchiveReportHtml { date: &'a str },
    ArchiveReportText { date: &'a str },
}

/// Sanitize an arbitrary identifier (such as arXiv ID or DOI) into a safe, single path component.
/// Eliminates path separators ('/', '\') and filesystem reserved characters (':' on Windows, etc.).
pub fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| match c {
            ':' | '/' | '\\' | '<' | '>' | '"' | '|' | '?' | '*' => '_',
            other => other,
        })
        .collect()
}

impl<'a> StorageKey<'a> {
    /// Convert this typed key into a validated `StatePath`.
    pub fn to_state_path(&self) -> Result<StatePath> {
        let relative = match self {
            Self::RunManifest { run_id } => format!("cache/runs/{}/manifest.json", run_id),
            Self::RunDir { run_id } => format!("cache/runs/{}", run_id),

            Self::ZoteroSyncState => "cache/zotero/sync-state.json".to_string(),
            Self::ZoteroSnapshot { snapshot_id } => {
                format!("cache/zotero/snapshots/{}.json", snapshot_id)
            }

            Self::EmbeddingVector { hash } => format!("cache/embeddings/{}.vec", hash),

            Self::RerankSelection { selection_id } => {
                format!("cache/rerank/{}.json", selection_id)
            }

            Self::PapersDir => "cache/papers".to_string(),
            Self::PaperDir { paper_id } => format!("cache/papers/{}", paper_id),
            Self::PaperReadCacheDir { paper_id } => format!("cache/papers/{}/read", paper_id),
            Self::PaperReadCache { paper_id, cache_key } => {
                format!("cache/papers/{}/read/{}.json", paper_id, cache_key)
            }
            Self::PaperPdf { paper_id } => {
                let safe_name = sanitize_id(paper_id);
                format!("cache/papers/{}/{}.pdf", paper_id, safe_name)
            }
            Self::PaperExtractedText { paper_id } => {
                let safe_name = sanitize_id(paper_id);
                format!("cache/papers/{}/{}.txt", paper_id, safe_name)
            }

            Self::RunsDir => "cache/runs".to_string(),
            Self::ZoteroSnapshotsDir => "cache/zotero/snapshots".to_string(),
            Self::ArchiveReadDir { date } => format!("archive/{}/read", date),

            Self::RunReportHtml { run_id } => format!("cache/reports/{}/report.html", run_id),
            Self::RunReportText { run_id } => format!("cache/reports/{}/report.txt", run_id),
            Self::RunReportMeta { run_id } => format!("cache/reports/{}/report.json", run_id),
            Self::RunReportDir { run_id } => format!("cache/reports/{}", run_id),

            Self::DeliveryReceipt { delivery_key } => {
                format!("cache/deliveries/history/{}.json", delivery_key)
            }

            Self::ArchiveSnapshotId { date } => format!("archive/{}/snapshot_id.txt", date),
            Self::ArchiveCandidates { date } => format!("archive/{}/candidates.json", date),
            Self::ArchiveDedup { date } => format!("archive/{}/dedup.json", date),
            Self::ArchiveEmbeddings { date } => format!("archive/{}/embeddings.json", date),
            Self::ArchiveRerank { date } => format!("archive/{}/rerank.json", date),
            Self::ArchiveReadResult { date, paper_id } => {
                let safe_id = sanitize_id(paper_id);
                format!("archive/{}/read/{}.json", date, safe_id)
            }
            Self::ArchiveReportHtml { date } => format!("archive/{}/report/report.html", date),
            Self::ArchiveReportText { date } => format!("archive/{}/report/report.txt", date),
        };
        StatePath::new(relative)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_key_paths() {
        assert_eq!(
            StorageKey::RunManifest { run_id: "20260101-120000-abcd" }
                .to_state_path()
                .unwrap()
                .as_path(),
            std::path::Path::new("cache/runs/20260101-120000-abcd/manifest.json")
        );
        assert_eq!(
            StorageKey::ArchiveDedup { date: "2026-01-01" }
                .to_state_path()
                .unwrap()
                .as_path(),
            std::path::Path::new("archive/2026-01-01/dedup.json")
        );
        assert_eq!(
            StorageKey::EmbeddingVector { hash: "deadbeef" }
                .to_state_path()
                .unwrap()
                .as_path(),
            std::path::Path::new("cache/embeddings/deadbeef.vec")
        );
        assert_eq!(
            StorageKey::ArchiveReadResult {
                date: "2026-03-30",
                paper_id: "doi:10.1000/182"
            }
            .to_state_path()
            .unwrap()
            .as_path(),
            std::path::Path::new("archive/2026-03-30/read/doi_10.1000_182.json")
        );
        assert_eq!(
            StorageKey::PaperPdf {
                paper_id: "arxiv:2301.12345"
            }
            .to_state_path()
            .unwrap()
            .as_path(),
            std::path::Path::new("cache/papers/arxiv:2301.12345/arxiv_2301.12345.pdf")
        );
    }
}
