use hyperchad_actions::logic::Value;
use hyperchad_renderer::{View, transformer::models::Selector};
use thiserror::Error;

use crate::{
    client::{
        actions::{ActionEffects, ActionRuntime, parse_selector_str},
        core::event_types,
        event::CustomEvent,
        form::FormSubmission,
        http_events::{HttpEventKind, HttpEventPayload},
        routing::RouteTable,
        sse::SseFrame,
    },
    renderer::TestingRenderer,
    snapshots,
};

/// Errors returned by harness operations.
#[derive(Debug, Error)]
pub enum HarnessError {
    #[error("Invalid selector: {selector}")]
    InvalidSelector { selector: String },
    #[error("Selector not found: {selector}")]
    SelectorNotFound { selector: String },
    #[error("Route not found: {path}")]
    RouteNotFound { path: String },
    #[error("Assertion failed: {message}")]
    AssertionFailed { message: String },
}

/// High-level deterministic UI testing harness.
pub struct Harness {
    renderer: TestingRenderer,
    routes: RouteTable,
    action_runtime: ActionRuntime,
    navigation_history: Vec<String>,
    effects_log: Vec<ActionEffects>,
}

impl Harness {
    /// Creates a harness from an existing testing renderer.
    #[must_use]
    pub fn new(renderer: TestingRenderer) -> Self {
        let action_runtime = ActionRuntime::new(renderer.snapshot.clone());
        Self {
            renderer,
            routes: RouteTable::default(),
            action_runtime,
            navigation_history: vec![],
            effects_log: vec![],
        }
    }

    /// Creates a harness with a fresh testing renderer.
    #[must_use]
    pub fn with_default_renderer() -> Self {
        Self::new(TestingRenderer::new())
    }

    /// Returns the renderer used by this harness.
    #[must_use]
    pub const fn renderer(&self) -> &TestingRenderer {
        &self.renderer
    }

    /// Returns navigation history.
    #[must_use]
    pub fn navigation_history(&self) -> &[String] {
        &self.navigation_history
    }

    /// Returns all captured action effect batches.
    #[must_use]
    pub fn effects_log(&self) -> &[ActionEffects] {
        &self.effects_log
    }

    /// Registers a route with full view only.
    pub fn route_full(&mut self, path: impl Into<String>, view: View) -> &mut Self {
        self.routes.insert_full(path, view);
        self
    }

    /// Registers a route with full and partial variants.
    pub fn route_full_and_partial(
        &mut self,
        path: impl Into<String>,
        full: View,
        partial: View,
    ) -> &mut Self {
        self.routes.insert_full_and_partial(path, full, partial);
        self
    }

    /// Applies a view directly.
    pub fn apply_view(&mut self, view: View) {
        self.renderer.apply_view(view);
        let effects = self.action_runtime.run_immediate_actions();
        if !effects.navigation.is_empty()
            || !effects.custom_actions.is_empty()
            || !effects.logs.is_empty()
            || effects.repaint_requests > 0
        {
            self.effects_log.push(effects);
        }
    }

    /// Navigates to a full route response.
    pub fn navigate_to(&mut self, path: &str) -> Result<(), HarnessError> {
        let Some(view) = self.routes.resolve(path, false) else {
            return Err(HarnessError::RouteNotFound {
                path: path.to_string(),
            });
        };
        self.navigation_history.push(path.to_string());
        self.apply_view(view);
        Ok(())
    }

    /// Navigates to a partial (`hx-request`) route response when available.
    pub fn navigate_hx(&mut self, path: &str) -> Result<(), HarnessError> {
        let Some(view) = self.routes.resolve(path, true) else {
            return Err(HarnessError::RouteNotFound {
                path: path.to_string(),
            });
        };
        self.navigation_history.push(path.to_string());
        self.apply_view(view);
        Ok(())
    }

    /// Executes form semantics in deterministic order: `hx-*` first, then action.
    pub fn submit_form(&mut self, submission: &FormSubmission) -> Result<(), HarnessError> {
        if let Some(hx_route) = &submission.hx_route {
            self.navigate_hx(hx_route)?;
        }
        if let Some(action_route) = &submission.action_route {
            self.navigate_to(action_route)?;
        }
        Ok(())
    }

    /// Dispatches a click event.
    pub fn click(&mut self, selector: &str) -> Result<ActionEffects, HarnessError> {
        self.dispatch(selector, event_types::CLICK, None, None, None)
    }

    /// Dispatches an arbitrary event.
    pub fn dispatch(
        &mut self,
        selector: &str,
        event_type: &str,
        event_name: Option<&str>,
        event_value: Option<&str>,
        value: Option<&Value>,
    ) -> Result<ActionEffects, HarnessError> {
        let selector = parse_selector_str(selector)?;
        if !self.renderer.snapshot().dom.contains_selector(&selector) {
            return Err(HarnessError::SelectorNotFound {
                selector: selector_to_string(&selector),
            });
        }
        let effects = self.action_runtime.dispatch_event(
            &selector,
            event_type,
            event_name,
            event_value,
            value,
        );
        self.follow_navigation_effects(&effects)?;
        self.effects_log.push(effects.clone());
        Ok(effects)
    }

    /// Dispatches a custom named event.
    pub fn dispatch_custom_event(
        &mut self,
        selector: &str,
        event: CustomEvent,
    ) -> Result<ActionEffects, HarnessError> {
        self.renderer
            .record_event(event.name.clone(), event.value.clone());
        let value_model = event.value.as_ref().map(|x| Value::String(x.clone()));
        self.dispatch(
            selector,
            "event",
            Some(&event.name),
            event.value.as_deref(),
            value_model.as_ref(),
        )
    }

    /// Dispatches an HTTP lifecycle event.
    pub fn dispatch_http_event(
        &mut self,
        selector: &str,
        kind: HttpEventKind,
        payload: &HttpEventPayload,
    ) -> Result<ActionEffects, HarnessError> {
        let json = payload.to_json_string();
        let value = Value::String(json.clone());
        self.dispatch(selector, kind.event_type(), None, Some(&json), Some(&value))
    }

    /// Applies a simulated SSE frame.
    pub fn consume_sse_frame(&mut self, frame: SseFrame) -> Result<(), HarnessError> {
        match frame {
            SseFrame::View(view) | SseFrame::PartialView(view) => self.apply_view(view),
            SseFrame::Event { name, value } => {
                self.renderer.record_event(name, value);
            }
            #[cfg(feature = "canvas")]
            SseFrame::CanvasUpdate(update) => {
                self.renderer
                    .snapshot
                    .write()
                    .unwrap()
                    .transcript
                    .push_canvas_update(update);
            }
        }
        Ok(())
    }

    /// Asserts selector existence.
    pub fn assert_selector_exists(&self, selector: &str) -> Result<(), HarnessError> {
        let selector = parse_selector_str(selector)?;
        let snapshot = self.renderer.snapshot();
        let exists = snapshot.dom.contains_selector(&selector);
        if exists {
            Ok(())
        } else {
            Err(HarnessError::SelectorNotFound {
                selector: selector_to_string(&selector),
            })
        }
    }

    /// Asserts stream kinds in exact order.
    pub fn assert_stream_kinds(&self, expected: &[&str]) -> Result<(), HarnessError> {
        let snapshot = self.renderer.snapshot();
        let actual = snapshot.transcript.kinds();
        if actual == expected {
            Ok(())
        } else {
            Err(HarnessError::AssertionFailed {
                message: format!("expected stream kinds {expected:?}, got {actual:?}"),
            })
        }
    }

    /// Snapshot-friendly normalized DOM string.
    #[must_use]
    pub fn dom_snapshot(&self) -> String {
        snapshots::dom_snapshot(&self.renderer.snapshot().dom)
    }

    /// Snapshot-friendly normalized stream transcript.
    #[must_use]
    pub fn stream_snapshot(&self) -> String {
        snapshots::transcript_snapshot(&self.renderer.snapshot().transcript)
    }

    fn follow_navigation_effects(&mut self, effects: &ActionEffects) -> Result<(), HarnessError> {
        for url in &effects.navigation {
            if self.routes.contains(url) {
                self.navigate_to(url)?;
            }
        }
        Ok(())
    }
}

fn selector_to_string(selector: &Selector) -> String {
    match selector {
        Selector::Id(id) => format!("#{id}"),
        Selector::Class(class) => format!(".{class}"),
        Selector::ChildClass(class) => format!("> .{class}"),
        Selector::SelfTarget => "self".to_string(),
    }
}
