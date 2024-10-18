use std::{
    collections::HashMap,
    sync::{Arc, LazyLock, RwLock},
};

use bytes::Bytes;

pub static BYTES: LazyLock<RwLock<HashMap<String, Arc<Bytes>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[macro_export]
macro_rules! image {
    ($path:expr $(,)?) => {{
        let bytes = include_bytes!($path);
        moosicbox_app_native_image::BYTES.write().unwrap().insert(
            ($path).to_owned(),
            std::sync::Arc::new(bytes.to_vec().into()),
        );
        $path
    }};
}

pub fn get_image(path: &str) -> Option<Arc<Bytes>> {
    BYTES.read().unwrap().get(path).cloned()
}
