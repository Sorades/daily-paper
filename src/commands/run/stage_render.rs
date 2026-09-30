use chrono::Utc;
use tracing::info;

use crate::models::candidate::CandidatePaper;
use crate::models::common::sha256_hex;
use crate::models::read::ReadResult;
use crate::models::report::RenderedReport;
use crate::models::run::*;
use crate::render::html::{render_html, ReportPaper};
use crate::render::text::render_text;
use crate::state::keys::StorageKey;

use super::context::{record_stage, ExecutionContext};

pub(crate) struct RenderInput<'a> {
    pub(crate) selected_ids: &'a [String],
    pub(crate) candidates: &'a [CandidatePaper],
    pub(crate) read_results: &'a [ReadResult],
    pub(crate) scores: &'a [(String, f32)],
}

pub(crate) async fn stage_render(
    cx: &mut ExecutionContext<'_>,
    input: RenderInput<'_>,
) -> anyhow::Result<(String, Option<String>)> {
    let date = cx.date().to_string();
    let run_id = cx.run_id.to_string();
    let stage_start = Utc::now();
    let mut record = StageRecord {
        stage: StageName::Render,
        status: StageStatus::Running,
        started_at: stage_start,
        finished_at: None,
        cache_hit: false,
        input_hash: None,
        output_ref: None,
        error: None,
    };

    let report_papers: Vec<ReportPaper> = input
        .selected_ids
        .iter()
        .enumerate()
        .filter_map(|(i, paper_id)| {
            let candidate = input.candidates.iter().find(|c| &c.paper_id == paper_id)?;
            let read_result = input
                .read_results
                .iter()
                .find(|r| &r.paper_id == paper_id)
                .cloned();

            Some(ReportPaper {
                paper_id: paper_id.clone(),
                rank: i + 1,
                title: candidate.title.clone(),
                authors: candidate.authors.clone(),
                abstract_text: candidate.abstract_text.clone(),
                landing_url: candidate.landing_url.clone(),
                pdf_url: candidate.pdf_url.clone(),
                read_result,
                score: input
                    .scores
                    .iter()
                    .find(|(id, _)| id == paper_id)
                    .map(|(_, s)| *s)
                    .unwrap_or(0.0),
            })
        })
        .collect();

    let title = format!("Report - {}", cx.manifest.date_window.label);
    let html_body = render_html(
        &title,
        &report_papers,
        &run_id,
        cx.config.report_template_path.as_deref(),
    )?;
    let text_body = render_text(&title, &report_papers, &run_id);

    cx.store.put_string(&StorageKey::RunReportHtml { run_id: &run_id }, &html_body)?;
    cx.store.put_string(&StorageKey::RunReportText { run_id: &run_id }, &text_body)?;

    cx.store.put_string(&StorageKey::ArchiveReportHtml { date: &date }, &html_body)?;
    cx.store.put_string(&StorageKey::ArchiveReportText { date: &date }, &text_body)?;

    let report_hash = sha256_hex(html_body.as_bytes());
    let report_instance_id = format!("{}-{}", run_id, &report_hash[..8]);
    let generated_at = Utc::now();

    let cache_html_path = cx.store.resolve_key(&StorageKey::RunReportHtml { run_id: &run_id })?;
    let cache_text_path = cx.store.resolve_key(&StorageKey::RunReportText { run_id: &run_id })?;

    let rendered = RenderedReport {
        report_hash: report_hash.clone(),
        report_instance_id: report_instance_id.clone(),
        run_id: run_id.to_string(),
        generated_at,
        title,
        html_path: cache_html_path.to_string_lossy().to_string(),
        text_path: Some(cache_text_path.to_string_lossy().to_string()),
        ranked_paper_ids: input.selected_ids.to_vec(),
        read_paper_ids: input
            .read_results
            .iter()
            .map(|r| r.paper_id.clone())
            .collect(),
    };

    cx.store.put_json(&StorageKey::RunReportMeta { run_id: &run_id }, &rendered)?;

    let archive_html_path = StorageKey::ArchiveReportHtml { date: &date }
        .to_state_path()?
        .resolve(cx.store.root());
    let archive_text_path = StorageKey::ArchiveReportText { date: &date }
        .to_state_path()?
        .resolve(cx.store.root());

    info!(
        papers = report_papers.len(),
        html = %archive_html_path.display(),
        "render complete"
    );

    record.status = StageStatus::Succeeded;
    record.finished_at = Some(Utc::now());
    record_stage(cx.manifest, record);

    Ok((
        archive_html_path.to_string_lossy().to_string(),
        Some(archive_text_path.to_string_lossy().to_string()),
    ))
}
