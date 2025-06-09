use const_hex::{Buffer, const_encode};
use sha2_const_stable::Sha256;

#[cfg(feature = "plugin-idiomorph")]
const PLUGIN_IDIOMORPH_HASH: &str = "-idiomorph";
#[cfg(not(feature = "plugin-idiomorph"))]
const PLUGIN_IDIOMORPH_HASH: &str = "";

#[cfg(feature = "plugin-nav")]
const PLUGIN_NAV_HASH: &str = "-nav";
#[cfg(not(feature = "plugin-nav"))]
const PLUGIN_NAV_HASH: &str = "";

#[cfg(feature = "plugin-sse")]
const PLUGIN_SSE_HASH: &str = "-sse";
#[cfg(not(feature = "plugin-sse"))]
const PLUGIN_SSE_HASH: &str = "";

#[cfg(feature = "plugin-tauri-event")]
const PLUGIN_TAURI_EVENT_HASH: &str = "-tauri-event";
#[cfg(not(feature = "plugin-tauri-event"))]
const PLUGIN_TAURI_EVENT_HASH: &str = "";

#[cfg(all(not(feature = "plugin-uuid-insecure"), feature = "plugin-uuid"))]
const PLUGIN_UUID_HASH: &str = "-uuid";
#[cfg(not(all(not(feature = "plugin-uuid-insecure"), feature = "plugin-uuid")))]
const PLUGIN_UUID_HASH: &str = "";

#[cfg(feature = "plugin-uuid-insecure")]
const PLUGIN_UUID_INSECURE_HASH: &str = "-uuid-insecure";
#[cfg(not(feature = "plugin-uuid-insecure"))]
const PLUGIN_UUID_INSECURE_HASH: &str = "";

#[cfg(feature = "plugin-routing")]
const PLUGIN_ROUTING_HASH: &str = "-routing";
#[cfg(not(feature = "plugin-routing"))]
const PLUGIN_ROUTING_HASH: &str = "";

#[cfg(feature = "plugin-event")]
const PLUGIN_EVENT_HASH: &str = "-event";
#[cfg(not(feature = "plugin-event"))]
const PLUGIN_EVENT_HASH: &str = "";

#[cfg(feature = "plugin-canvas")]
const PLUGIN_CANVAS_HASH: &str = "-canvas";
#[cfg(not(feature = "plugin-canvas"))]
const PLUGIN_CANVAS_HASH: &str = "";

#[cfg(feature = "plugin-form")]
const PLUGIN_FORM_HASH: &str = "-form";
#[cfg(not(feature = "plugin-form"))]
const PLUGIN_FORM_HASH: &str = "";

#[cfg(feature = "plugin-actions-change")]
const PLUGIN_ACTIONS_CHANGE_HASH: &str = "-actions-change";
#[cfg(not(feature = "plugin-actions-change"))]
const PLUGIN_ACTIONS_CHANGE_HASH: &str = "";

#[cfg(feature = "plugin-actions-click")]
const PLUGIN_ACTIONS_CLICK_HASH: &str = "-actions-click";
#[cfg(not(feature = "plugin-actions-click"))]
const PLUGIN_ACTIONS_CLICK_HASH: &str = "";

#[cfg(feature = "plugin-actions-click-outside")]
const PLUGIN_ACTIONS_CLICK_OUTSIDE_HASH: &str = "-actions-click-outside";
#[cfg(not(feature = "plugin-actions-click-outside"))]
const PLUGIN_ACTIONS_CLICK_OUTSIDE_HASH: &str = "";

#[cfg(feature = "plugin-actions-event")]
const PLUGIN_ACTIONS_EVENT_HASH: &str = "-actions-event";
#[cfg(not(feature = "plugin-actions-event"))]
const PLUGIN_ACTIONS_EVENT_HASH: &str = "";

#[cfg(feature = "plugin-actions-immediate")]
const PLUGIN_ACTIONS_IMMEDIATE_HASH: &str = "-actions-immediate";
#[cfg(not(feature = "plugin-actions-immediate"))]
const PLUGIN_ACTIONS_IMMEDIATE_HASH: &str = "";

#[cfg(feature = "plugin-actions-mouse-down")]
const PLUGIN_ACTIONS_MOUSE_DOWN_HASH: &str = "-actions-mouse-down";
#[cfg(not(feature = "plugin-actions-mouse-down"))]
const PLUGIN_ACTIONS_MOUSE_DOWN_HASH: &str = "";

#[cfg(feature = "plugin-actions-mouse-over")]
const PLUGIN_ACTIONS_MOUSE_OVER_HASH: &str = "-actions-mouse-over";
#[cfg(not(feature = "plugin-actions-mouse-over"))]
const PLUGIN_ACTIONS_MOUSE_OVER_HASH: &str = "";

#[cfg(feature = "plugin-actions-resize")]
const PLUGIN_ACTIONS_RESIZE_HASH: &str = "-actions-resize";
#[cfg(not(feature = "plugin-actions-resize"))]
const PLUGIN_ACTIONS_RESIZE_HASH: &str = "";

pub const PLUGIN_HASH: &str = const_format::concatcp!(
    "plugins",
    PLUGIN_IDIOMORPH_HASH,
    PLUGIN_NAV_HASH,
    PLUGIN_SSE_HASH,
    PLUGIN_TAURI_EVENT_HASH,
    PLUGIN_UUID_HASH,
    PLUGIN_UUID_INSECURE_HASH,
    PLUGIN_ROUTING_HASH,
    PLUGIN_EVENT_HASH,
    PLUGIN_ACTIONS_CHANGE_HASH,
    PLUGIN_ACTIONS_CLICK_HASH,
    PLUGIN_ACTIONS_CLICK_OUTSIDE_HASH,
    PLUGIN_ACTIONS_EVENT_HASH,
    PLUGIN_ACTIONS_IMMEDIATE_HASH,
    PLUGIN_ACTIONS_MOUSE_DOWN_HASH,
    PLUGIN_ACTIONS_MOUSE_OVER_HASH,
    PLUGIN_ACTIONS_RESIZE_HASH,
    PLUGIN_CANVAS_HASH,
    PLUGIN_FORM_HASH,
);

pub const RAW_HASH: [u8; Sha256::DIGEST_SIZE] =
    Sha256::new().update(PLUGIN_HASH.as_bytes()).finalize();

pub const HEX_BUF: Buffer<{ Sha256::DIGEST_SIZE }> = const_encode(&RAW_HASH);
pub const PLUGIN_HASH_HEX: &str = HEX_BUF.as_str();
