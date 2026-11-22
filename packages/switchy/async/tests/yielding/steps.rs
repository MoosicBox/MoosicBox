use std::sync::{Arc, Mutex};

/// Records `name` and awaits a ready future, `count` times.
pub async fn steps(log: Arc<Mutex<Vec<String>>>, name: &'static str, count: usize) {
    for i in 0..count {
        log.lock().unwrap().push(format!("{name}{i}"));
        std::future::ready(()).await;
    }
}
