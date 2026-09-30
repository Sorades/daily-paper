use chrono::{Duration, Utc};
use anyhow::Context;
use tracing::info;

use crate::config::ResolvedConfig;
use crate::models::run::*;
use crate::models::zotero::{LibraryPaper, ZoteroSnapshot, ZoteroSyncState};
use crate::state::keys::StorageKey;
use crate::state::store::FileStateStore;
use crate::zotero::client::ZoteroClient;
use crate::zotero::convert::zotero_item_to_library_paper;
use crate::zotero::profile::{build_collection_paths, compute_snapshot_id};

use super::context::{record_stage, ExecutionContext};

pub(crate) async fn stage_zotero_sync(
    cx: &mut ExecutionContext<'_>,
    force: bool,
) -> anyhow::Result<ZoteroSnapshot> {
    let date = cx.date().to_string();
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::ZoteroSync,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let sync_state_key = StorageKey::ZoteroSyncState;
    let sync_state: Option<ZoteroSyncState> = cx.store.get_json(&sync_state_key)?;

    if !force {
        if let Some(ref state) = sync_state {
            if let (Some(snapshot_id), Some(last_success)) =
                (&state.last_snapshot_id, state.last_success_at)
            {
                let age = Utc::now() - last_success;
                if age < Duration::hours(cx.config.zotero.max_snapshot_age_hours as i64) {
                    if let Some(snapshot) =
                        cx.store.get_json::<ZoteroSnapshot>(&StorageKey::ZoteroSnapshot { snapshot_id })?
                    {
                        info!(
                            snapshot_id = %snapshot_id,
                            age_hours = age.num_hours(),
                            "using cached Zotero snapshot"
                        );
                        record.status = StageStatus::Succeeded;
                        record.cache_hit = true;
                        record.finished_at = Some(Utc::now());
                        record.output_ref = Some(snapshot_id.clone());
                        record_stage(cx.manifest, record);
                        return Ok(snapshot);
                    }
                }
            }
        }
    }

    info!("syncing Zotero library...");
    let client = ZoteroClient::new(cx.config.zotero.user_id.clone(), cx.config.zotero.api_key.clone());

    let library_version = client
        .get_library_version()
        .await
        .context("failed to get Zotero library version")?;

    let since_version = if force {
        None
    } else {
        sync_state.as_ref().and_then(|s| s.library_version)
    };

    let items = client
        .fetch_items(since_version)
        .await
        .context("failed to fetch Zotero items")?;

    if since_version.is_some() && items.is_empty() {
        let versions_match = sync_state.as_ref().and_then(|s| s.library_version) == library_version;

        if versions_match {
            if let Some(ref state) = sync_state {
                if let Some(ref snapshot_id) = state.last_snapshot_id {
                    if let Some(snapshot) = cx.store.get_json::<ZoteroSnapshot>(&StorageKey::ZoteroSnapshot {
                        snapshot_id,
                    })? {
                        info!(
                            snapshot_id = %snapshot_id,
                            paper_count = snapshot.item_count,
                            library_version = ?library_version,
                            "Zotero library unchanged, reusing cached snapshot"
                        );
                        record.status = StageStatus::Succeeded;
                        record.cache_hit = true;
                        record.finished_at = Some(Utc::now());
                        record.output_ref = Some(snapshot_id.clone());
                        record_stage(cx.manifest, record);
                        return Ok(snapshot);
                    }
                }
            }
            info!("cached snapshot missing, falling back to full sync");
            let items = client
                .fetch_items(None)
                .await
                .context("failed to fetch Zotero items (full sync)")?;
            return stage_zotero_sync_inner(
                cx.store,
                cx.config,
                cx.manifest,
                ZoteroSyncArgs {
                    record,
                    client,
                    library_version,
                    items,
                    date: date.to_string(),
                },
            )
            .await;
        }
        info!(
            old_version = ?sync_state.as_ref().and_then(|s| s.library_version),
            new_version = ?library_version,
            "library version changed but no items returned, falling back to full sync"
        );
        let items = client
            .fetch_items(None)
            .await
            .context("failed to fetch Zotero items (full sync)")?;
        return stage_zotero_sync_inner(
            cx.store,
            cx.config,
            cx.manifest,
            ZoteroSyncArgs {
                record,
                client,
                library_version,
                items,
                date: date.to_string(),
            },
        )
        .await;
    }

    stage_zotero_sync_inner(
        cx.store,
        cx.config,
        cx.manifest,
        ZoteroSyncArgs {
            record,
            client,
            library_version,
            items,
            date: date.to_string(),
        },
    )
    .await
}

struct ZoteroSyncArgs {
    record: StageRecord,
    client: ZoteroClient,
    library_version: Option<u64>,
    items: Vec<serde_json::Value>,
    date: String,
}

async fn stage_zotero_sync_inner(
    store: &FileStateStore,
    config: &ResolvedConfig,
    manifest: &mut RunManifest,
    args: ZoteroSyncArgs,
) -> anyhow::Result<ZoteroSnapshot> {
    let ZoteroSyncArgs {
        mut record,
        client,
        library_version,
        items,
        date,
    } = args;
    let collections = client
        .fetch_collections()
        .await
        .context("failed to fetch Zotero collections")?;

    let collection_paths = build_collection_paths(&collections);

    let mut papers: Vec<LibraryPaper> = items
        .iter()
        .filter_map(|item| {
            let mut paper = zotero_item_to_library_paper(item)?;
            for ck in &paper.collection_keys {
                if let Some(path) = collection_paths.get(ck) {
                    paper
                        .collections
                        .push(crate::models::zotero::CollectionPath {
                            key: ck.clone(),
                            path: path.clone(),
                        });
                }
            }
            Some(paper)
        })
        .filter(|p| !p.is_trashed)
        .collect();

    papers.sort_by(|a, b| a.library_id.cmp(&b.library_id));

    let snapshot_id = compute_snapshot_id(&config.zotero.user_id, library_version, &papers);

    let snapshot = ZoteroSnapshot {
        snapshot_id: snapshot_id.clone(),
        user_id: config.zotero.user_id.clone(),
        library_version,
        created_at: Utc::now(),
        item_count: papers.len(),
        items: papers,
    };

    store.put_json(&StorageKey::ZoteroSnapshot { snapshot_id: &snapshot_id }, &snapshot)?;
    store.put_string(&StorageKey::ArchiveSnapshotId { date: &date }, &snapshot_id)?;

    let new_sync_state = ZoteroSyncState {
        user_id: config.zotero.user_id.clone(),
        library_version,
        last_success_at: Some(Utc::now()),
        last_snapshot_id: Some(snapshot_id.clone()),
        last_error: None,
    };
    store.put_json(&StorageKey::ZoteroSyncState, &new_sync_state)?;

    info!(
        snapshot_id = %snapshot_id,
        paper_count = snapshot.item_count,
        "Zotero sync complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record.output_ref = Some(snapshot_id);
    record_stage(manifest, record);

    Ok(snapshot)
}
