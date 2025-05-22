#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::{borrow::Cow, sync::Arc};

use bytes::Bytes;
use rust_embed::{Embed, EmbeddedFile};

#[derive(Embed)]
#[folder = "$CARGO_MANIFEST_DIR/../public/"]
#[prefix = "/public/"]
pub struct Asset;

fn cow_to_arc_bytes(cow: Cow<'_, [u8]>) -> Arc<Bytes> {
    Arc::new(match cow {
        Cow::Owned(vec) => Bytes::from(vec),
        Cow::Borrowed(slice) => Bytes::copy_from_slice(slice),
    })
}

#[must_use]
pub fn get_asset_arc_bytes(asset: EmbeddedFile) -> Arc<Bytes> {
    cow_to_arc_bytes(asset.data)
}
