//! Utility functions for the admin HTMX API.
//!
//! This module provides JavaScript code generation helpers for HTMX interactions.

/// Generates JavaScript code to clear the value of an input field.
///
/// Returns a JavaScript expression that selects an element using the provided CSS selector
/// and clears its value property. This is typically used with HTMX's `hx-on` attributes
/// to reset form inputs after requests.
///
/// # Examples
///
/// ```
/// use moosicbox_admin_htmx::api::util::clear_input;
///
/// let js = clear_input("#my-input");
/// assert_eq!(js, "document.querySelector('#my-input').value = ''");
/// ```
#[must_use]
#[allow(unused)]
pub fn clear_input(selector: &str) -> String {
    format!("document.querySelector('{selector}').value = ''")
}
