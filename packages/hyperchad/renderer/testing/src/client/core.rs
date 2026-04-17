/// Normalized event names used by the action runtime.
pub mod event_types {
    pub const CLICK: &str = "click";
    pub const CLICK_OUTSIDE: &str = "click_outside";
    pub const HOVER: &str = "hover";
    pub const CHANGE: &str = "change";
    pub const RESIZE: &str = "resize";
    pub const MOUSE_DOWN: &str = "mouse_down";
    pub const KEY_DOWN: &str = "key_down";
    pub const IMMEDIATE: &str = "immediate";

    pub const HTTP_BEFORE_REQUEST: &str = "http_before_request";
    pub const HTTP_AFTER_REQUEST: &str = "http_after_request";
    pub const HTTP_REQUEST_SUCCESS: &str = "http_request_success";
    pub const HTTP_REQUEST_ERROR: &str = "http_request_error";
    pub const HTTP_REQUEST_ABORT: &str = "http_request_abort";
    pub const HTTP_REQUEST_TIMEOUT: &str = "http_request_timeout";
}
