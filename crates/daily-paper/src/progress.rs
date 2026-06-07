use daily_paper_core::models::run::{PipelineEvent, StageName};
use indicatif::{ProgressBar, ProgressStyle};

/// Per-stage progress indicator that drives both CLI progress bars
/// and web SSE `PipelineEvent::Progress` events.
#[derive(Clone)]
pub struct StageProgress {
    pb: ProgressBar,
    event_tx: Option<tokio::sync::broadcast::Sender<PipelineEvent>>,
    run_id: String,
    stage: StageName,
}

impl StageProgress {
    /// Create a new progress bar for a pipeline stage.
    ///
    /// `total` is the number of items to process. Pass 0 for indeterminate stages.
    pub fn new(
        stage: StageName,
        total: usize,
        event_tx: Option<&tokio::sync::broadcast::Sender<PipelineEvent>>,
        run_id: &str,
    ) -> Self {
        let pb = if total > 0 {
            ProgressBar::new(total as u64)
        } else {
            ProgressBar::new_spinner()
        };

        let style = if total > 0 {
            ProgressStyle::with_template(
                "  {spinner:.green} [{bar:30.cyan/dim}] {pos}/{len} ({eta}) {msg}",
            )
            .expect("valid template")
            .progress_chars("█░")
        } else {
            ProgressStyle::with_template("  {spinner:.green} {msg}").expect("valid template")
        };
        pb.set_style(style);
        pb.enable_steady_tick(std::time::Duration::from_millis(120));

        Self {
            pb,
            event_tx: event_tx.cloned(),
            run_id: run_id.to_string(),
            stage,
        }
    }

    /// Advance by one item and update the message.
    pub fn inc(&self, message: &str) {
        self.pb.inc(1);
        self.pb.set_message(message.to_string());

        if let Some(tx) = &self.event_tx {
            let _ = tx.send(PipelineEvent::Progress {
                run_id: self.run_id.clone(),
                stage: self.stage.clone(),
                current: self.pb.position() as usize,
                total: self.pb.length().unwrap_or(0) as usize,
                message: message.to_string(),
            });
        }
    }

    /// Mark the progress bar as finished.
    pub fn finish(&self, message: &str) {
        self.pb.finish_with_message(message.to_string());
    }
}
