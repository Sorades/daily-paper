use std::path::Path;
use anyhow::Context;
use chrono::Utc;
use tracing::info;

use crate::deliver::receipt::{deliver_email, EmailDelivery};
use crate::models::report::{compute_delivery_key, RenderedReport};
use crate::models::run::*;
use crate::state::keys::StorageKey;

use super::context::{record_stage, ExecutionContext};

pub(crate) async fn stage_send(
    cx: &mut ExecutionContext<'_>,
    report_run_id: &str,
    html_path: &str,
    text_path: Option<&str>,
    force: bool,
) -> anyhow::Result<()> {
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Send,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let rendered: RenderedReport = cx
        .store
        .get_json(&StorageKey::RunReportMeta { run_id: report_run_id })?
        .context("report metadata not found")?;

    let sink_config_hash = {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        hasher.update(cx.config.email.smtp_server.as_bytes());
        hasher.update(cx.config.email.smtp_port.to_le_bytes());
        hasher.update(cx.config.email.sender.as_bytes());
        hasher.update(cx.config.email.receiver.as_bytes());
        hex::encode(hasher.finalize())
    };

    let delivery_key = compute_delivery_key(
        &rendered.report_hash,
        "smtp",
        &sink_config_hash,
        &cx.config.email.receiver,
    );

    if !force {
        let receipt_key = StorageKey::DeliveryReceipt { delivery_key: &delivery_key };
        if cx.store.key_exists(&receipt_key)? {
            info!("report already sent, skipping (use --force-send to override)");
            record.status = StageStatus::Succeeded;
            record.cache_hit = true;
            record.finished_at = Some(Utc::now());
            record_stage(cx.manifest, record);
            return Ok(());
        }
    }

    let subject = format!("Daily Paper - {}", cx.manifest.date_window.label);
    let now = Utc::now();

    let report_hash = rendered.report_hash.clone();
    let report_instance_id = rendered.report_instance_id.clone();
    let run_id_owned = report_run_id.to_string();
    let html_path_buf = Path::new(html_path).to_path_buf();
    let text_path_buf = text_path.map(|p| Path::new(p).to_path_buf());
    let smtp_server = cx.config.email.smtp_server.clone();
    let smtp_port = cx.config.email.smtp_port;
    let sender = cx.config.email.sender.clone();
    let receiver = cx.config.email.receiver.clone();
    let password = cx.config.email.password.clone();

    let receipt = tokio::task::spawn_blocking(move || {
        deliver_email(&EmailDelivery {
            report_hash: &report_hash,
            report_instance_id: &report_instance_id,
            run_id: &run_id_owned,
            html_path: &html_path_buf,
            text_path: text_path_buf.as_deref(),
            smtp_server: &smtp_server,
            smtp_port,
            sender: &sender,
            receiver: &receiver,
            password: &password,
            subject: &subject,
            now,
        })
    })
    .await
    .map_err(|e| anyhow::anyhow!("delivery task panicked or was cancelled: {}", e))?
    .context("failed to send email")?;

    cx.store.put_json(
        &StorageKey::DeliveryReceipt { delivery_key: &delivery_key },
        &receipt,
    )?;

    info!(delivery_key = %delivery_key, "email sent successfully");

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record_stage(cx.manifest, record);

    Ok(())
}
