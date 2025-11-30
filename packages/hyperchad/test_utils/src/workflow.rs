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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::time::Duration;

    // InteractionStep description tests
    #[test_log::test]
    fn interaction_step_description_click() {
        let step = InteractionStep::Click {
            selector: "#submit-btn".to_string(),
        };
        assert_eq!(step.description(), "Click #submit-btn");
    }

    #[test_log::test]
    fn interaction_step_description_double_click() {
        let step = InteractionStep::DoubleClick {
            selector: ".item".to_string(),
        };
        assert_eq!(step.description(), "Double-click .item");
    }

    #[test_log::test]
    fn interaction_step_description_right_click() {
        let step = InteractionStep::RightClick {
            selector: "#context-menu-target".to_string(),
        };
        assert_eq!(step.description(), "Right-click #context-menu-target");
    }

    #[test_log::test]
    fn interaction_step_description_hover() {
        let step = InteractionStep::Hover {
            selector: ".tooltip-trigger".to_string(),
        };
        assert_eq!(step.description(), "Hover over .tooltip-trigger");
    }

    #[test_log::test]
    fn interaction_step_description_focus() {
        let step = InteractionStep::Focus {
            selector: "#email-input".to_string(),
        };
        assert_eq!(step.description(), "Focus #email-input");
    }

    #[test_log::test]
    fn interaction_step_description_blur() {
        let step = InteractionStep::Blur {
            selector: "#email-input".to_string(),
        };
        assert_eq!(step.description(), "Blur #email-input");
    }

    #[test_log::test]
    fn interaction_step_description_key_press() {
        let step = InteractionStep::KeyPress { key: Key::Enter };
        assert_eq!(step.description(), "Press key Enter");
    }

    #[test_log::test]
    fn interaction_step_description_key_sequence() {
        let step = InteractionStep::KeySequence {
            keys: vec![Key::Control, Key::C],
        };
        assert_eq!(step.description(), "Press key sequence: [Control, C]");
    }

    #[test_log::test]
    fn interaction_step_description_scroll() {
        let step = InteractionStep::Scroll {
            direction: ScrollDirection::Down,
            amount: 100,
        };
        assert_eq!(step.description(), "Scroll Down by 100");
    }

    #[test_log::test]
    fn interaction_step_description_drag_and_drop() {
        let step = InteractionStep::DragAndDrop {
            from_selector: "#source".to_string(),
            to_selector: "#target".to_string(),
        };
        assert_eq!(step.description(), "Drag from #source to #target");
    }

    #[test_log::test]
    fn interaction_step_description_mouse_move() {
        let step = InteractionStep::MouseMove { x: 100, y: 200 };
        assert_eq!(step.description(), "Move mouse to (100, 200)");
    }

    #[test_log::test]
    fn interaction_step_description_mouse_down() {
        let step = InteractionStep::MouseDown {
            button: MouseButton::Left,
        };
        assert_eq!(step.description(), "Mouse down Left");
    }

    #[test_log::test]
    fn interaction_step_description_mouse_up() {
        let step = InteractionStep::MouseUp {
            button: MouseButton::Right,
        };
        assert_eq!(step.description(), "Mouse up Right");
    }

    // ControlStep description tests
    #[test_log::test]
    fn control_step_description_loop() {
        let step = ControlStep::Loop {
            count: 5,
            steps: vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
        };
        assert_eq!(step.description(), "Loop 5 times with 1 steps");
    }

    #[test_log::test]
    fn control_step_description_for_each() {
        let step = ControlStep::ForEach {
            data: vec![
                serde_json::json!({"id": 1}),
                serde_json::json!({"id": 2}),
                serde_json::json!({"id": 3}),
            ],
            steps: vec![
                crate::TestStep::Navigation(crate::NavigationStep::GoBack),
                crate::TestStep::Navigation(crate::NavigationStep::GoForward),
            ],
        };
        assert_eq!(step.description(), "For each of 3 items with 2 steps");
    }

    #[test_log::test]
    fn control_step_description_parallel() {
        let mut branches = BTreeMap::new();
        branches.insert(
            "branch1".to_string(),
            vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
        );
        branches.insert(
            "branch2".to_string(),
            vec![crate::TestStep::Navigation(
                crate::NavigationStep::GoForward,
            )],
        );
        let step = ControlStep::Parallel { branches };
        assert_eq!(step.description(), "Parallel execution with 2 branches");
    }

    #[test_log::test]
    fn control_step_description_try_without_catch_or_finally() {
        let step = ControlStep::Try {
            steps: vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
            catch_steps: None,
            finally_steps: None,
        };
        assert_eq!(step.description(), "Try 1 steps");
    }

    #[test_log::test]
    fn control_step_description_try_with_catch() {
        let step = ControlStep::Try {
            steps: vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
            catch_steps: Some(vec![crate::TestStep::Navigation(
                crate::NavigationStep::Reload,
            )]),
            finally_steps: None,
        };
        assert_eq!(step.description(), "Try 1 steps with catch");
    }

    #[test_log::test]
    fn control_step_description_try_with_finally() {
        let step = ControlStep::Try {
            steps: vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
            catch_steps: None,
            finally_steps: Some(vec![crate::TestStep::Navigation(
                crate::NavigationStep::Reload,
            )]),
        };
        assert_eq!(step.description(), "Try 1 steps with finally");
    }

    #[test_log::test]
    fn control_step_description_try_with_catch_and_finally() {
        let step = ControlStep::Try {
            steps: vec![
                crate::TestStep::Navigation(crate::NavigationStep::GoBack),
                crate::TestStep::Navigation(crate::NavigationStep::GoForward),
            ],
            catch_steps: Some(vec![crate::TestStep::Navigation(
                crate::NavigationStep::Reload,
            )]),
            finally_steps: Some(vec![crate::TestStep::Navigation(
                crate::NavigationStep::GoBack,
            )]),
        };
        assert_eq!(step.description(), "Try 2 steps with catch with finally");
    }

    #[test_log::test]
    fn control_step_description_retry_without_delay() {
        let step = ControlStep::Retry {
            steps: vec![
                crate::TestStep::Navigation(crate::NavigationStep::GoBack),
                crate::TestStep::Navigation(crate::NavigationStep::GoForward),
                crate::TestStep::Navigation(crate::NavigationStep::Reload),
            ],
            max_attempts: 3,
            delay: None,
        };
        assert_eq!(step.description(), "Retry 3 steps up to 3 times");
    }

    #[test_log::test]
    fn control_step_description_retry_with_delay() {
        let step = ControlStep::Retry {
            steps: vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
            max_attempts: 5,
            delay: Some(Duration::from_secs(2)),
        };
        assert_eq!(
            step.description(),
            "Retry 1 steps up to 5 times with 2s delay"
        );
    }

    // ControlStep constructor tests
    #[test_log::test]
    fn control_step_loop_count_constructor() {
        let step = ControlStep::loop_count(
            10,
            vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
        );
        if let ControlStep::Loop { count, steps } = step {
            assert_eq!(count, 10);
            assert_eq!(steps.len(), 1);
        } else {
            panic!("Expected Loop variant");
        }
    }

    #[test_log::test]
    fn control_step_for_each_constructor() {
        let step = ControlStep::for_each(
            vec![serde_json::json!(1), serde_json::json!(2)],
            vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
        );
        if let ControlStep::ForEach { data, steps } = step {
            assert_eq!(data.len(), 2);
            assert_eq!(steps.len(), 1);
        } else {
            panic!("Expected ForEach variant");
        }
    }

    #[test_log::test]
    fn control_step_parallel_constructor() {
        let mut branches = BTreeMap::new();
        branches.insert("b1".to_string(), vec![]);
        let step = ControlStep::parallel(branches);
        if let ControlStep::Parallel { branches } = step {
            assert_eq!(branches.len(), 1);
        } else {
            panic!("Expected Parallel variant");
        }
    }

    #[test_log::test]
    fn control_step_try_catch_constructor() {
        let step = ControlStep::try_catch(
            vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
            Some(vec![crate::TestStep::Navigation(
                crate::NavigationStep::Reload,
            )]),
            Some(vec![crate::TestStep::Navigation(
                crate::NavigationStep::GoForward,
            )]),
        );
        if let ControlStep::Try {
            steps,
            catch_steps,
            finally_steps,
        } = step
        {
            assert_eq!(steps.len(), 1);
            assert!(catch_steps.is_some());
            assert!(finally_steps.is_some());
        } else {
            panic!("Expected Try variant");
        }
    }

    #[test_log::test]
    fn control_step_retry_constructor() {
        let step = ControlStep::retry(
            vec![crate::TestStep::Navigation(crate::NavigationStep::GoBack)],
            3,
            Some(Duration::from_millis(500)),
        );
        if let ControlStep::Retry {
            steps,
            max_attempts,
            delay,
        } = step
        {
            assert_eq!(steps.len(), 1);
            assert_eq!(max_attempts, 3);
            assert_eq!(delay, Some(Duration::from_millis(500)));
        } else {
            panic!("Expected Retry variant");
        }
    }

    // Fragment tests
    #[test_log::test]
    fn fragments_login_flow_creates_correct_steps() {
        let plan = fragments::login_flow("testuser", "testpass");
        assert_eq!(plan.steps.len(), 5);
    }

    #[test_log::test]
    fn fragments_logout_flow_creates_correct_steps() {
        let plan = fragments::logout_flow();
        assert_eq!(plan.steps.len(), 4);
    }

    #[test_log::test]
    fn fragments_navigation_test_creates_correct_steps() {
        let plan = fragments::navigation_test();
        assert_eq!(plan.steps.len(), 8);
    }

    #[test_log::test]
    fn fragments_form_validation_test_creates_correct_steps() {
        let plan = fragments::form_validation_test("#my-form");
        assert_eq!(plan.steps.len(), 7);
    }

    #[test_log::test]
    fn fragments_accessibility_test_creates_correct_steps() {
        let plan = fragments::accessibility_test();
        assert_eq!(plan.steps.len(), 6);
    }
}
