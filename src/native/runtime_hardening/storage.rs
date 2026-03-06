use super::*;

mod ingestor;
mod store;

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub(super) struct NativeRuntimeStateStore {
    pub(super) db_path: PathBuf,
    busy_timeout_ms: u64,
}

#[derive(Debug, Clone)]
pub(super) struct RuntimeLogRecord {
    pub(super) ts_ms: i64,
    pub(super) component: String,
    pub(super) level: String,
    pub(super) message: String,
    pub(super) context_json: Option<String>,
}

enum LogIngestorMessage {
    Log(RuntimeLogRecord),
    Flush(Sender<Result<()>>),
    Shutdown(Sender<Result<()>>),
}

#[derive(Debug)]
pub(super) struct RuntimeLogIngestor {
    sender: Sender<LogIngestorMessage>,
    handle: Option<thread::JoinHandle<()>>,
}
