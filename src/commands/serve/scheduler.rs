use std::sync::atomic::Ordering;
use chrono::{Duration, FixedOffset, Local, NaiveTime, Offset, TimeZone, Utc};
use tracing::{info, warn};

use super::runner::spawn_pipeline;
use super::state::AppState;
use super::types::RunRequest;

/// Parse a timezone offset string like "+08:00", "+08", "-05:00", "UTC", "Z", "Asia/Shanghai" etc.
/// Returns a FixedOffset.
pub fn parse_tz_offset(tz_str: Option<&str>) -> FixedOffset {
    match tz_str {
        Some(s) if s.eq_ignore_ascii_case("utc") || s.eq_ignore_ascii_case("z") => {
            FixedOffset::east_opt(0).unwrap()
        }
        Some(s) if s.contains("Shanghai") || s.contains("Beijing") || s.contains("China") => {
            FixedOffset::east_opt(8 * 3600).unwrap()
        }
        Some(s) => {
            let clean = s.trim();
            if let Ok(offset) = clean.parse::<FixedOffset>() {
                offset
            } else if clean.starts_with('+') || clean.starts_with('-') {
                let sign = if clean.starts_with('-') { -1 } else { 1 };
                let num_part = clean.trim_start_matches(['+', '-']);
                let parts: Vec<&str> = num_part.split(':').collect();
                let hours: i32 = parts[0].parse().unwrap_or(0);
                let mins: i32 = if parts.len() > 1 {
                    parts[1].parse().unwrap_or(0)
                } else {
                    0
                };
                let total_secs = sign * (hours * 3600 + mins * 60);
                FixedOffset::east_opt(total_secs).unwrap_or_else(|| FixedOffset::east_opt(0).unwrap())
            } else {
                Local::now().offset().fix()
            }
        }
        None => Local::now().offset().fix(),
    }
}

/// Calculate the next instant at the given hour:minute in the specified timezone offset.
/// If that time has already passed today, returns tomorrow.
pub(crate) fn next_run_instant(hour: u32, minute: u32, offset: FixedOffset) -> (tokio::time::Instant, chrono::DateTime<FixedOffset>) {
    let now_utc = Utc::now();
    let now_tz = now_utc.with_timezone(&offset);

    let target_time = NaiveTime::from_hms_opt(hour, minute, 0).unwrap_or_default();
    let today_target = now_tz.date_naive().and_time(target_time);

    let today_target_tz = offset.from_local_datetime(&today_target).single().unwrap_or(now_tz);

    let target = if today_target_tz > now_tz {
        today_target_tz
    } else {
        today_target_tz + Duration::days(1)
    };

    let duration_secs = (target.timestamp() - now_tz.timestamp()).max(0) as u64;
    (
        tokio::time::Instant::now() + std::time::Duration::from_secs(duration_secs),
        target,
    )
}

pub(crate) async fn scheduler_loop(state: AppState, hour: u32, minute: u32, timezone_str: Option<&str>) {
    let offset = parse_tz_offset(timezone_str);
    info!(
        hour = hour,
        minute = minute,
        offset = %offset,
        "scheduler enabled, will run pipeline daily according to target timezone"
    );

    loop {
        let (next_instant, next_time) = next_run_instant(hour, minute, offset);
        info!(
            next_run = %next_time.format("%Y-%m-%d %H:%M:%S %:z"),
            "next scheduled run"
        );

        tokio::time::sleep_until(next_instant).await;

        // If pipeline is currently running, retry up to 12 times (every 5 minutes, up to 1 hour)
        // to prevent missed daily digest due to colliding manual runs.
        let mut retry_count = 0;
        while state.pipeline_running.load(Ordering::SeqCst) && retry_count < 12 {
            warn!(
                attempt = retry_count + 1,
                "scheduled run delayed: pipeline already running, retrying in 5 minutes"
            );
            tokio::time::sleep(std::time::Duration::from_secs(300)).await;
            retry_count += 1;
        }

        if state.pipeline_running.load(Ordering::SeqCst) {
            warn!("scheduled run skipped: pipeline still busy after 1 hour of retries");
            continue;
        }

        info!("scheduled run starting");
        let req = RunRequest {
            stages: None,
            from_run: None,
            date: None,
            dry_run: Some(false),
            force_zotero_sync: None,
            force_embedding: None,
            force_rerank: None,
            force_read: None,
            force_send: None,
            max_candidates: None,
            no_email: Some(false),
            send_email: Some(true),
        };

        match spawn_pipeline(&state, &req).await {
            Ok(run_id) => info!(run_id = %run_id, "scheduled pipeline started"),
            Err((status, msg)) => {
                warn!(
                    status = %status,
                    message = %msg.message,
                    "scheduled run failed to start"
                );
            }
        }
    }
}
