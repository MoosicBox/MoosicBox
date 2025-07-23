use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionStep {
    Click {
        selector: String,
    },
    DoubleClick {
        selector: String,
    },
    RightClick {
        selector: String,
    },
    Hover {
        selector: String,
    },
    Focus {
        selector: String,
    },
    Blur {
        selector: String,
    },
    KeyPress {
        key: Key,
    },
    KeySequence {
        keys: Vec<Key>,
    },
    Scroll {
        direction: ScrollDirection,
        amount: i32,
    },
    DragAndDrop {
        from_selector: String,
        to_selector: String,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseDown {
        button: MouseButton,
    },
    MouseUp {
        button: MouseButton,
    },
}

impl InteractionStep {
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Click { selector } => format!("Click {selector}"),
            Self::DoubleClick { selector } => format!("Double-click {selector}"),
            Self::RightClick { selector } => format!("Right-click {selector}"),
            Self::Hover { selector } => format!("Hover over {selector}"),
            Self::Focus { selector } => format!("Focus {selector}"),
            Self::Blur { selector } => format!("Blur {selector}"),
            Self::KeyPress { key } => format!("Press key {key:?}"),
            Self::KeySequence { keys } => format!("Press key sequence: {keys:?}"),
            Self::Scroll { direction, amount } => format!("Scroll {direction:?} by {amount}"),
            Self::DragAndDrop {
                from_selector,
                to_selector,
            } => {
                format!("Drag from {from_selector} to {to_selector}")
            }
            Self::MouseMove { x, y } => format!("Move mouse to ({x}, {y})"),
            Self::MouseDown { button } => format!("Mouse down {button:?}"),
            Self::MouseUp { button } => format!("Mouse up {button:?}"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Key {
    // Letters
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,

    // Numbers
    Digit0,
    Digit1,
    Digit2,
    Digit3,
    Digit4,
    Digit5,
    Digit6,
    Digit7,
    Digit8,
    Digit9,

    // Function keys
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,

    // Special keys
    Enter,
    Escape,
    Space,
    Tab,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,

    // Arrow keys
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,

    // Modifier keys
    Shift,
    Control,
    Alt,
    Meta,

    // Other keys
    CapsLock,
    NumLock,
    ScrollLock,
    PrintScreen,
    Pause,

    // Punctuation
    Semicolon,
    Equal,
    Comma,
    Minus,
    Period,
    Slash,
    Backquote,
    BracketLeft,
    Backslash,
    BracketRight,
    Quote,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScrollDirection {
    Up,
    Down,
    Left,
    Right,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlStep {
    Loop {
        count: u32,
        steps: Vec<crate::TestStep>,
    },
    ForEach {
        data: Vec<serde_json::Value>,
        steps: Vec<crate::TestStep>,
    },
    Parallel {
        branches: BTreeMap<String, Vec<crate::TestStep>>,
    },
    Try {
        steps: Vec<crate::TestStep>,
        catch_steps: Option<Vec<crate::TestStep>>,
        finally_steps: Option<Vec<crate::TestStep>>,
    },
    Retry {
        steps: Vec<crate::TestStep>,
        max_attempts: u32,
        delay: Option<Duration>,
    },
}

impl ControlStep {
    #[must_use]
    pub const fn loop_count(count: u32, steps: Vec<crate::TestStep>) -> Self {
        Self::Loop { count, steps }
    }

    #[must_use]
    pub const fn for_each(data: Vec<serde_json::Value>, steps: Vec<crate::TestStep>) -> Self {
        Self::ForEach { data, steps }
    }

    #[must_use]
    pub const fn parallel(branches: BTreeMap<String, Vec<crate::TestStep>>) -> Self {
        Self::Parallel { branches }
    }

    #[must_use]
    pub const fn try_catch(
        steps: Vec<crate::TestStep>,
        catch_steps: Option<Vec<crate::TestStep>>,
        finally_steps: Option<Vec<crate::TestStep>>,
    ) -> Self {
        Self::Try {
            steps,
            catch_steps,
            finally_steps,
        }
    }

    #[must_use]
    pub const fn retry(
        steps: Vec<crate::TestStep>,
        max_attempts: u32,
        delay: Option<Duration>,
    ) -> Self {
        Self::Retry {
            steps,
            max_attempts,
            delay,
        }
    }

    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Loop { count, steps } => {
                format!("Loop {} times with {} steps", count, steps.len())
            }
            Self::ForEach { data, steps } => {
                format!(
                    "For each of {} items with {} steps",
                    data.len(),
                    steps.len()
                )
            }
            Self::Parallel { branches } => {
                format!("Parallel execution with {} branches", branches.len())
            }
            Self::Try {
                steps,
                catch_steps,
                finally_steps,
            } => {
                let catch_part = if catch_steps.is_some() {
                    " with catch"
                } else {
                    ""
                };
                let finally_part = if finally_steps.is_some() {
                    " with finally"
                } else {
                    ""
                };
                format!("Try {} steps{}{}", steps.len(), catch_part, finally_part)
            }
            Self::Retry {
                steps,
                max_attempts,
                delay,
            } => {
                let delay_part = delay
                    .as_ref()
                    .map_or_else(String::new, |d| format!(" with {d:?} delay"));
                format!(
                    "Retry {} steps up to {} times{}",
                    steps.len(),
                    max_attempts,
                    delay_part
                )
            }
        }
    }
}

/// Test fragments for reusable test scenarios
pub mod fragments {
    use super::Key;
    use crate::{FormData, TestPlan};

    /// Common login flow
    #[must_use]
    pub fn login_flow(username: &str, password: &str) -> TestPlan {
        TestPlan::new()
            .navigate_to("/login")
            .wait_for_element("#login-form")
            .fill_form(
                FormData::new()
                    .text("username", username)
                    .text("password", password),
            )
            .click("#login-button")
            .wait_for_url("/dashboard")
    }

    /// Common logout flow
    #[must_use]
    pub fn logout_flow() -> TestPlan {
        TestPlan::new()
            .click("#user-menu")
            .wait_for_element("#logout-button")
            .click("#logout-button")
            .wait_for_url("/login")
    }

    /// Navigation test across main sections
    #[must_use]
    pub fn navigation_test() -> TestPlan {
        TestPlan::new()
            .navigate_to("/")
            .wait_for_element("nav")
            .click("nav a[href='/about']")
            .wait_for_url("/about")
            .click("nav a[href='/contact']")
            .wait_for_url("/contact")
            .go_back()
            .wait_for_url("/about")
    }

    /// Form validation test
    #[must_use]
    pub fn form_validation_test(form_selector: &str) -> TestPlan {
        TestPlan::new()
            .click(format!("{form_selector} input[type='submit']"))
            .wait_for_element(".error-message")
            .fill_form(FormData::new().text("email", "invalid-email"))
            .click(format!("{form_selector} input[type='submit']"))
            .wait_for_element(".error-message")
            .fill_form(FormData::new().text("email", "valid@example.com"))
            .click(format!("{form_selector} input[type='submit']"))
    }

    /// Accessibility test
    #[must_use]
    pub fn accessibility_test() -> TestPlan {
        TestPlan::new()
            .navigate_to("/")
            .wait_for_element("h1")
            .wait_for_element("main")
            .wait_for_element("nav")
            // Test keyboard navigation
            .key_press(Key::Tab)
            .wait_for_element(":focus")
    }
}
