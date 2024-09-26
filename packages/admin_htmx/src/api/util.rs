#[allow(unused)]
pub fn clear_input(selector: &str) -> String {
    format!("document.querySelector('{selector}').value = ''")
}
