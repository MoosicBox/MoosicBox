use std::collections::HashMap;
use std::error::Error;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use enum_as_inner::EnumAsInner;
use futures::Future;
use serde::{Deserialize, Serialize};

use std::sync::{Mutex, OnceLock};

use crate::menu::Album;
use crate::sqlite::menu::FullAlbum;

#[derive(Debug, Serialize, Deserialize, Clone)]
struct CacheItem {
    expiration: u128,
    data: CacheItemType,
}

#[derive(Debug, Serialize, Deserialize, Clone, EnumAsInner)]
#[serde(untagged)]
pub enum CacheItemType {
    Albums(Vec<Album>),
    Album(Album),
    FullAlbum(FullAlbum),
}

pub fn current_time_nanos() -> u128 {
    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    since_the_epoch.as_nanos()
}

pub struct CacheRequest {
    pub key: String,
    pub expiration: Duration,
}

pub async fn get_or_set_to_cache<Fut>(
    request: CacheRequest,
    compute: impl Fn() -> Fut,
) -> Result<CacheItemType, Box<dyn Error>>
where
    Fut: Future<Output = Result<CacheItemType, Box<dyn Error>>>,
{
    let info: HashMap<String, CacheItem> = HashMap::new();

    static CACHE_MAP: OnceLock<Mutex<HashMap<String, CacheItem>>> = OnceLock::new();
    let cache = CACHE_MAP.get_or_init(|| Mutex::new(info));

    if let Some(entry) = cache.lock().unwrap().get(&request.key) {
        if entry.expiration > current_time_nanos() {
            return Ok(entry.data.clone());
        }
    }

    let value = match compute().await {
        Ok(x) => x,
        Err(error) => return Err(error),
    };

    cache.lock().unwrap().insert(
        request.key,
        CacheItem {
            expiration: current_time_nanos() + request.expiration.as_nanos(),
            data: value.clone(),
        },
    );

    Ok(value)
}
