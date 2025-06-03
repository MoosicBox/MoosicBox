use std::sync::{Arc, atomic::AtomicBool};

#[derive(Debug, Clone)]
pub struct ApiAuth {
    logged_in: Arc<AtomicBool>,
}

impl Default for ApiAuth {
    fn default() -> Self {
        Self::new(false)
    }
}

impl ApiAuth {
    #[must_use]
    pub fn new(logged_in: bool) -> Self {
        Self {
            logged_in: Arc::new(AtomicBool::new(logged_in)),
        }
    }

    #[must_use]
    pub fn is_logged_in(&self) -> bool {
        self.logged_in.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_logged_in(&self, logged_in: bool) {
        self.logged_in
            .store(logged_in, std::sync::atomic::Ordering::SeqCst);
    }
}
