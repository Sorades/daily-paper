use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::{broadcast, watch, Mutex, RwLock};

use crate::config::ResolvedConfig;
use crate::models::run::PipelineEvent;
use crate::state::store::FileStateStore;

pub(crate) type LogBuffer = Arc<Mutex<VecDeque<String>>>;

pub(crate) const MAX_LOG_LINES: usize = 10_000;

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) store: Arc<FileStateStore>,
    pub(crate) config_path: PathBuf,
    pub(crate) config: Arc<RwLock<ResolvedConfig>>,
    pub(crate) pipeline_tx: broadcast::Sender<PipelineEvent>,
    pub(crate) log_buffer: LogBuffer,
    pub(crate) log_tx: broadcast::Sender<String>,
    pub(crate) pipeline_running: Arc<AtomicBool>,
    pub(crate) cancel_tx: Arc<Mutex<Option<watch::Sender<bool>>>>,
}

pub(crate) struct RunningGuard(pub(crate) Arc<AtomicBool>);

impl Drop for RunningGuard {
    fn drop(&mut self) {
        self.0.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
