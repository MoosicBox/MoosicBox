use crate::db::models::DownloadTask;

pub struct DownloadQueue {
    pub tasks: Vec<DownloadTask>,
}
