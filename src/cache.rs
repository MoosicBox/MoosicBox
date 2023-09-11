use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use futures::Future;
use serde::{Deserialize, Serialize};

use std::sync::{Mutex, OnceLock};

use crate::player::Album;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CacheItem<T> {
    expiration: u128,
    data: T,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Cache {
    albums: Option<Vec<Album>>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum CacheItems {
    Albums(Vec<Album>),
}

pub fn current_time_nanos() -> u128 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_nanos()
}

pub async fn get_or_set_to_cache<Fut>(key: &str, compute: impl Fn() -> Fut) -> CacheItems
where
    Fut: Future<Output = CacheItems>,
{
    let info: HashMap<String, CacheItem<CacheItems>> = HashMap::new();

    static CACHE_MAP: OnceLock<Mutex<HashMap<String, CacheItem<CacheItems>>>> = OnceLock::new();
    let cache = CACHE_MAP.get_or_init(|| Mutex::new(info));

    if let Some(entry) = cache.lock().unwrap().get(key) {
        if entry.expiration > current_time_nanos() {
            return entry.data.clone();
        }
    }

    let value = compute().await;

    cache.lock().unwrap().insert(
        String::from(key),
        CacheItem {
            expiration: current_time_nanos() + 60 * 60 * 1000 * 1000 * 1000,
            data: value.clone(),
        },
    );

    value
}
