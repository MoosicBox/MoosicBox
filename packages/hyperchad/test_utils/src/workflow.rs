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

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_interaction_step_click_description() {
        let step = InteractionStep::Click {
            selector: "#submit-button".to_string(),
        };
        assert_eq!(step.description(), "Click #submit-button");
    }

    #[test]
    fn test_interaction_step_double_click_description() {
        let step = InteractionStep::DoubleClick {
            selector: ".item".to_string(),
        };
        assert_eq!(step.description(), "Double-click .item");
    }

    #[test]
    fn test_interaction_step_right_click_description() {
        let step = InteractionStep::RightClick {
            selector: "#context-menu".to_string(),
        };
        assert_eq!(step.description(), "Right-click #context-menu");
    }

    #[test]
    fn test_interaction_step_hover_description() {
        let step = InteractionStep::Hover {
            selector: ".tooltip-trigger".to_string(),
        };
        assert_eq!(step.description(), "Hover over .tooltip-trigger");
    }

    #[test]
    fn test_interaction_step_focus_description() {
        let step = InteractionStep::Focus {
            selector: "input[name='email']".to_string(),
        };
        assert_eq!(step.description(), "Focus input[name='email']");
    }

    #[test]
    fn test_interaction_step_blur_description() {
        let step = InteractionStep::Blur {
            selector: "#search-field".to_string(),
        };
        assert_eq!(step.description(), "Blur #search-field");
    }

    #[test]
    fn test_interaction_step_key_press_description() {
        let step = InteractionStep::KeyPress { key: Key::Enter };
        assert_eq!(step.description(), "Press key Enter");
    }

    #[test]
    fn test_interaction_step_key_sequence_description() {
        let step = InteractionStep::KeySequence {
            keys: vec![Key::Control, Key::C],
        };
        let desc = step.description();
        assert!(desc.contains("Press key sequence"));
        assert!(desc.contains("Control"));
        assert!(desc.contains("C"));
    }

    #[test]
    fn test_interaction_step_scroll_description() {
        let step = InteractionStep::Scroll {
            direction: ScrollDirection::Down,
            amount: 500,
        };
        let desc = step.description();
        assert!(desc.contains("Scroll"));
        assert!(desc.contains("Down"));
        assert!(desc.contains("500"));
    }

    #[test]
    fn test_interaction_step_drag_and_drop_description() {
        let step = InteractionStep::DragAndDrop {
            from_selector: "#draggable".to_string(),
            to_selector: "#dropzone".to_string(),
        };
        assert_eq!(step.description(), "Drag from #draggable to #dropzone");
    }

    #[test]
    fn test_interaction_step_mouse_move_description() {
        let step = InteractionStep::MouseMove { x: 100, y: 200 };
        assert_eq!(step.description(), "Move mouse to (100, 200)");
    }

    #[test]
    fn test_interaction_step_mouse_down_description() {
        let step = InteractionStep::MouseDown {
            button: MouseButton::Left,
        };
        let desc = step.description();
        assert!(desc.contains("Mouse down"));
        assert!(desc.contains("Left"));
    }

    #[test]
    fn test_interaction_step_mouse_up_description() {
        let step = InteractionStep::MouseUp {
            button: MouseButton::Right,
        };
        let desc = step.description();
        assert!(desc.contains("Mouse up"));
        assert!(desc.contains("Right"));
    }

    #[test]
    fn test_control_step_loop_description() {
        let step = ControlStep::Loop {
            count: 5,
            steps: vec![],
        };
        assert_eq!(step.description(), "Loop 5 times with 0 steps");
    }

    #[test]
    fn test_control_step_for_each_description() {
        let data = vec![serde_json::json!({"id": 1}), serde_json::json!({"id": 2})];
        let step = ControlStep::ForEach {
            data,
            steps: vec![],
        };
        assert_eq!(step.description(), "For each of 2 items with 0 steps");
    }

    #[test]
    fn test_control_step_parallel_description() {
        let mut branches = BTreeMap::new();
        branches.insert("task1".to_string(), vec![]);
        branches.insert("task2".to_string(), vec![]);
        let step = ControlStep::Parallel { branches };
        assert_eq!(step.description(), "Parallel execution with 2 branches");
    }

    #[test]
    fn test_control_step_try_description_full() {
        let step = ControlStep::Try {
            steps: vec![],
            catch_steps: Some(vec![]),
            finally_steps: Some(vec![]),
        };
        assert_eq!(step.description(), "Try 0 steps with catch with finally");
    }

    #[test]
    fn test_control_step_try_description_catch_only() {
        let step = ControlStep::Try {
            steps: vec![],
            catch_steps: Some(vec![]),
            finally_steps: None,
        };
        assert_eq!(step.description(), "Try 0 steps with catch");
    }

    #[test]
    fn test_control_step_try_description_finally_only() {
        let step = ControlStep::Try {
            steps: vec![],
            catch_steps: None,
            finally_steps: Some(vec![]),
        };
        assert_eq!(step.description(), "Try 0 steps with finally");
    }

    #[test]
    fn test_control_step_try_description_no_catch_or_finally() {
        let step = ControlStep::Try {
            steps: vec![],
            catch_steps: None,
            finally_steps: None,
        };
        assert_eq!(step.description(), "Try 0 steps");
    }

    #[test]
    fn test_control_step_retry_description_with_delay() {
        let step = ControlStep::Retry {
            steps: vec![],
            max_attempts: 3,
            delay: Some(Duration::from_secs(1)),
        };
        let desc = step.description();
        assert!(desc.contains("Retry 0 steps up to 3 times"));
        assert!(desc.contains("1s"));
    }

    #[test]
    fn test_control_step_retry_description_no_delay() {
        let step = ControlStep::Retry {
            steps: vec![],
            max_attempts: 5,
            delay: None,
        };
        assert_eq!(step.description(), "Retry 0 steps up to 5 times");
    }

    #[test]
    fn test_control_step_constructors() {
        let loop_step = ControlStep::loop_count(10, vec![]);
        match loop_step {
            ControlStep::Loop { count, .. } => assert_eq!(count, 10),
            _ => panic!("Expected Loop variant"),
        }

        let for_each_step = ControlStep::for_each(vec![serde_json::json!(1)], vec![]);
        match for_each_step {
            ControlStep::ForEach { data, .. } => assert_eq!(data.len(), 1),
            _ => panic!("Expected ForEach variant"),
        }

        let parallel_step = ControlStep::parallel(BTreeMap::new());
        match parallel_step {
            ControlStep::Parallel { branches } => assert_eq!(branches.len(), 0),
            _ => panic!("Expected Parallel variant"),
        }

        let try_step = ControlStep::try_catch(vec![], Some(vec![]), None);
        match try_step {
            ControlStep::Try { catch_steps, .. } => assert!(catch_steps.is_some()),
            _ => panic!("Expected Try variant"),
        }

        let retry_step = ControlStep::retry(vec![], 3, Some(Duration::from_millis(100)));
        match retry_step {
            ControlStep::Retry {
                max_attempts,
                delay,
                ..
            } => {
                assert_eq!(max_attempts, 3);
                assert_eq!(delay, Some(Duration::from_millis(100)));
            }
            _ => panic!("Expected Retry variant"),
        }
    }

    #[test]
    fn test_fragments_login_flow() {
        let plan = fragments::login_flow("testuser", "password123");
        assert!(plan.steps.len() >= 4); // Should have navigate, wait, fill_form, click, wait_for_url
    }

    #[test]
    fn test_fragments_logout_flow() {
        let plan = fragments::logout_flow();
        assert!(plan.steps.len() >= 3); // Should have click, wait, click, wait
    }

    #[test]
    fn test_fragments_navigation_test() {
        let plan = fragments::navigation_test();
        assert!(plan.steps.len() >= 5); // Should have multiple navigation steps
    }

    #[test]
    fn test_fragments_form_validation_test() {
        let plan = fragments::form_validation_test("#contact-form");
        assert!(plan.steps.len() >= 3); // Should have multiple validation steps
    }

    #[test]
    fn test_fragments_accessibility_test() {
        let plan = fragments::accessibility_test();
        assert!(plan.steps.len() >= 4); // Should have navigation and keyboard tests
    }

    #[test]
    fn test_interaction_step_serialization() {
        let step = InteractionStep::Click {
            selector: "#button".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: InteractionStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            InteractionStep::Click { selector } => assert_eq!(selector, "#button"),
            _ => panic!("Expected Click variant"),
        }
    }

    #[test]
    fn test_control_step_serialization() {
        let step = ControlStep::Loop {
            count: 3,
            steps: vec![],
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: ControlStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            ControlStep::Loop { count, .. } => assert_eq!(count, 3),
            _ => panic!("Expected Loop variant"),
        }
    }
}
