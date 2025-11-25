//! Test workflow execution and control flow utilities.
//!
//! This module provides types for user interactions (clicks, keyboard input, mouse
//! operations), control flow operations (loops, parallel execution, try-catch, retry),
//! and reusable test fragments for common scenarios like login, logout, and accessibility testing.

use std::{collections::BTreeMap, time::Duration};

use serde::{Deserialize, Serialize};

/// A user interaction step with the page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InteractionStep {
    /// Click an element.
    Click {
        /// CSS selector for the element to click.
        selector: String,
    },
    /// Double-click an element.
    DoubleClick {
        /// CSS selector for the element to double-click.
        selector: String,
    },
    /// Right-click an element.
    RightClick {
        /// CSS selector for the element to right-click.
        selector: String,
    },
    /// Hover over an element.
    Hover {
        /// CSS selector for the element to hover over.
        selector: String,
    },
    /// Focus an element.
    Focus {
        /// CSS selector for the element to focus.
        selector: String,
    },
    /// Blur an element.
    Blur {
        /// CSS selector for the element to blur.
        selector: String,
    },
    /// Press a keyboard key.
    KeyPress {
        /// Key to press.
        key: Key,
    },
    /// Press a sequence of keyboard keys.
    KeySequence {
        /// Keys to press in sequence.
        keys: Vec<Key>,
    },
    /// Scroll the page.
    Scroll {
        /// Direction to scroll.
        direction: ScrollDirection,
        /// Amount to scroll in pixels.
        amount: i32,
    },
    /// Drag and drop from one element to another.
    DragAndDrop {
        /// CSS selector for the source element.
        from_selector: String,
        /// CSS selector for the target element.
        to_selector: String,
    },
    /// Move the mouse to coordinates.
    MouseMove {
        /// X coordinate.
        x: i32,
        /// Y coordinate.
        y: i32,
    },
    /// Press a mouse button.
    MouseDown {
        /// Button to press.
        button: MouseButton,
    },
    /// Release a mouse button.
    MouseUp {
        /// Button to release.
        button: MouseButton,
    },
}

impl InteractionStep {
    /// Returns a human-readable description of this interaction step.
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

/// Keyboard keys for interaction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Key {
    /// Letter A key.
    A,
    /// Letter B key.
    B,
    /// Letter C key.
    C,
    /// Letter D key.
    D,
    /// Letter E key.
    E,
    /// Letter F key.
    F,
    /// Letter G key.
    G,
    /// Letter H key.
    H,
    /// Letter I key.
    I,
    /// Letter J key.
    J,
    /// Letter K key.
    K,
    /// Letter L key.
    L,
    /// Letter M key.
    M,
    /// Letter N key.
    N,
    /// Letter O key.
    O,
    /// Letter P key.
    P,
    /// Letter Q key.
    Q,
    /// Letter R key.
    R,
    /// Letter S key.
    S,
    /// Letter T key.
    T,
    /// Letter U key.
    U,
    /// Letter V key.
    V,
    /// Letter W key.
    W,
    /// Letter X key.
    X,
    /// Letter Y key.
    Y,
    /// Letter Z key.
    Z,

    /// Digit 0 key.
    Digit0,
    /// Digit 1 key.
    Digit1,
    /// Digit 2 key.
    Digit2,
    /// Digit 3 key.
    Digit3,
    /// Digit 4 key.
    Digit4,
    /// Digit 5 key.
    Digit5,
    /// Digit 6 key.
    Digit6,
    /// Digit 7 key.
    Digit7,
    /// Digit 8 key.
    Digit8,
    /// Digit 9 key.
    Digit9,

    /// F1 function key.
    F1,
    /// F2 function key.
    F2,
    /// F3 function key.
    F3,
    /// F4 function key.
    F4,
    /// F5 function key.
    F5,
    /// F6 function key.
    F6,
    /// F7 function key.
    F7,
    /// F8 function key.
    F8,
    /// F9 function key.
    F9,
    /// F10 function key.
    F10,
    /// F11 function key.
    F11,
    /// F12 function key.
    F12,

    /// Enter key.
    Enter,
    /// Escape key.
    Escape,
    /// Space bar.
    Space,
    /// Tab key.
    Tab,
    /// Backspace key.
    Backspace,
    /// Delete key.
    Delete,
    /// Insert key.
    Insert,
    /// Home key.
    Home,
    /// End key.
    End,
    /// Page Up key.
    PageUp,
    /// Page Down key.
    PageDown,

    /// Up arrow key.
    ArrowUp,
    /// Down arrow key.
    ArrowDown,
    /// Left arrow key.
    ArrowLeft,
    /// Right arrow key.
    ArrowRight,

    /// Shift modifier key.
    Shift,
    /// Control modifier key.
    Control,
    /// Alt modifier key.
    Alt,
    /// Meta (Windows/Command) modifier key.
    Meta,

    /// Caps Lock key.
    CapsLock,
    /// Num Lock key.
    NumLock,
    /// Scroll Lock key.
    ScrollLock,
    /// Print Screen key.
    PrintScreen,
    /// Pause key.
    Pause,

    /// Semicolon (;) key.
    Semicolon,
    /// Equal (=) key.
    Equal,
    /// Comma (,) key.
    Comma,
    /// Minus (-) key.
    Minus,
    /// Period (.) key.
    Period,
    /// Slash (/) key.
    Slash,
    /// Backquote (\`) key.
    Backquote,
    /// Left bracket ([) key.
    BracketLeft,
    /// Backslash (\) key.
    Backslash,
    /// Right bracket (]) key.
    BracketRight,
    /// Quote (') key.
    Quote,
}

/// Scroll direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScrollDirection {
    /// Scroll up.
    Up,
    /// Scroll down.
    Down,
    /// Scroll left.
    Left,
    /// Scroll right.
    Right,
}

/// Mouse button.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MouseButton {
    /// Left mouse button.
    Left,
    /// Right mouse button.
    Right,
    /// Middle mouse button.
    Middle,
}

/// Control flow operations for test execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ControlStep {
    /// Repeat steps a fixed number of times.
    Loop {
        /// Number of times to repeat.
        count: u32,
        /// Steps to execute in each iteration.
        steps: Vec<crate::TestStep>,
    },
    /// Execute steps for each item in a data set.
    ForEach {
        /// Data items to iterate over.
        data: Vec<serde_json::Value>,
        /// Steps to execute for each item.
        steps: Vec<crate::TestStep>,
    },
    /// Execute multiple branches in parallel.
    Parallel {
        /// Map of branch names to their steps.
        branches: BTreeMap<String, Vec<crate::TestStep>>,
    },
    /// Try steps with optional catch and finally blocks.
    Try {
        /// Steps to execute in the try block.
        steps: Vec<crate::TestStep>,
        /// Steps to execute if an error occurs.
        catch_steps: Option<Vec<crate::TestStep>>,
        /// Steps to execute regardless of success or failure.
        finally_steps: Option<Vec<crate::TestStep>>,
    },
    /// Retry steps on failure with a maximum attempt count.
    Retry {
        /// Steps to retry on failure.
        steps: Vec<crate::TestStep>,
        /// Maximum number of attempts.
        max_attempts: u32,
        /// Optional delay between retry attempts.
        delay: Option<Duration>,
    },
}

impl ControlStep {
    /// Creates a loop control step.
    #[must_use]
    pub const fn loop_count(count: u32, steps: Vec<crate::TestStep>) -> Self {
        Self::Loop { count, steps }
    }

    /// Creates a for-each control step.
    #[must_use]
    pub const fn for_each(data: Vec<serde_json::Value>, steps: Vec<crate::TestStep>) -> Self {
        Self::ForEach { data, steps }
    }

    /// Creates a parallel execution control step.
    #[must_use]
    pub const fn parallel(branches: BTreeMap<String, Vec<crate::TestStep>>) -> Self {
        Self::Parallel { branches }
    }

    /// Creates a try-catch-finally control step.
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

    /// Creates a retry control step.
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

    /// Returns a human-readable description of this control step.
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

pub mod fragments {
    //! Reusable test fragments for common testing scenarios.
    //!
    //! This module provides pre-built test plans for common operations like user
    //! authentication, navigation testing, form validation, and accessibility
    //! testing. These fragments can be used directly or included in larger test
    //! plans using [`TestPlan::include`].
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
