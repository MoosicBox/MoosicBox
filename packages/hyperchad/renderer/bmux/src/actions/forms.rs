//! Semantic form serialization. File capabilities are supplied by the host, never opened here.
use super::{ActionError, ActionLimits, Effect, EventKind, RequestEffect, Source, Work, selectors};
use hyperchad_transformer::{Container, Element, Input};
use hyperchad_transformer_models::{Route, Selector, SwapStrategy};
use std::collections::BTreeMap;

/// Opaque authorization to read a selected file. Only the host may mint/resolve `token`.
/// Deliberately has no `Debug` implementation: diagnostics must not expose capabilities.
#[derive(Clone, PartialEq, Eq)]
pub struct AuthorizedFile {
    /// Host-issued capability, not a pathname.
    pub token: String,
    /// Basename included in multipart metadata.
    pub name: String,
    /// MIME type validated before request creation.
    pub content_type: String,
    /// Authorized byte length; host must verify it when executing the effect.
    pub size: u64,
}
impl std::fmt::Debug for AuthorizedFile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthorizedFile")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}
/// Current semantic control value. Keys are canonical `layout::node_key`s.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ControlValue {
    /// Text, textarea, password, or hidden value.
    Text(String),
    /// Checkbox checked state.
    Checked(bool),
    /// Selected option values, preserving selection order and duplicate values.
    Selected(Vec<String>),
    /// Explicitly host-authorized selected files.
    Files(Vec<AuthorizedFile>),
}
/// Current controls supplied by the controls lane; missing entries use declared defaults.
pub type FormValues = BTreeMap<String, ControlValue>;
/// One successful form control. Duplicate names are intentionally preserved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormPart {
    /// UTF-8 text field.
    Text { name: String, value: String },
    /// Host-authorized file field.
    File { name: String, file: AuthorizedFile },
}
/// Deferred multipart stream. Host writes literals and resolves file capabilities in order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultipartSegment {
    /// Literal bytes; no IO has occurred.
    Bytes(Vec<u8>),
    /// Read exactly the authorized file bytes, or fail the request.
    File(AuthorizedFile),
}
/// Request data. Multipart boundaries are chosen by the runtime when executing the effect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RequestBody {
    Empty,
    UrlEncoded(String),
    Multipart(Vec<FormPart>),
}
impl RequestBody {
    /// Produce multipart framing without opening files. Set the matching content-type header
    /// only after this succeeds. Host must choose a high-entropy boundary not present in data.
    ///
    /// # Errors
    /// * Rejects invalid boundary/header metadata and oversized encoded streams.
    pub fn multipart_segments(
        &self,
        boundary: &str,
        max_bytes: u64,
    ) -> Result<Vec<MultipartSegment>, ActionError> {
        if boundary.is_empty()
            || boundary.len() > 70
            || !boundary
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        {
            return Err(ActionError::Invalid("multipart boundary".into()));
        }
        let Self::Multipart(parts) = self else {
            return Err(ActionError::Invalid("not multipart".into()));
        };
        let mut segments = Vec::new();
        let mut total = 0_u64;
        for part in parts {
            let (name, tail) = match part {
                FormPart::Text { name, .. } => (name, String::new()),
                FormPart::File { name, file } => {
                    validate_file(file)?;
                    (
                        name,
                        format!(
                            "; filename=\"{}\"\r\nContent-Type: {}",
                            quoted(&file.name),
                            file.content_type
                        ),
                    )
                }
            };
            let header = format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"{tail}\r\n\r\n",
                quoted(name)
            );
            total = total
                .checked_add(header.len() as u64)
                .ok_or(ActionError::Limit("multipart bytes"))?;
            segments.push(MultipartSegment::Bytes(header.into_bytes()));
            match part {
                FormPart::Text { value, .. } => {
                    total = total
                        .checked_add(value.len() as u64)
                        .ok_or(ActionError::Limit("multipart bytes"))?;
                    segments.push(MultipartSegment::Bytes(value.as_bytes().to_vec()));
                }
                FormPart::File { file, .. } => {
                    total = total
                        .checked_add(file.size)
                        .ok_or(ActionError::Limit("multipart bytes"))?;
                    segments.push(MultipartSegment::File(file.clone()));
                }
            }
            total = total
                .checked_add(2)
                .ok_or(ActionError::Limit("multipart bytes"))?;
            if total > max_bytes {
                return Err(ActionError::Limit("multipart bytes"));
            }
            segments.push(MultipartSegment::Bytes(b"\r\n".to_vec()));
        }
        let end = format!("--{boundary}--\r\n");
        if total.saturating_add(end.len() as u64) > max_bytes {
            return Err(ActionError::Limit("multipart bytes"));
        }
        segments.push(MultipartSegment::Bytes(end.into_bytes()));
        Ok(segments)
    }
}
fn quoted(value: &str) -> String {
    value
        .replace('\r', "%0D")
        .replace('\n', "%0A")
        .replace('"', "%22")
}
fn validate_file(file: &AuthorizedFile) -> Result<(), ActionError> {
    if file.token.is_empty()
        || file.name.contains(['/', '\\', '\0'])
        || file.content_type.is_empty()
        || file.content_type.bytes().any(|b| b <= 32 || b >= 127)
    {
        return Err(ActionError::Invalid("file authorization metadata".into()));
    }
    Ok(())
}
pub(super) fn validate_values(
    values: &FormValues,
    limits: &ActionLimits,
) -> Result<(), ActionError> {
    if values.len() > limits.max_entries {
        return Err(ActionError::Limit("control values"));
    }
    let mut bytes = 0_usize;
    let mut entries = 0_usize;
    let mut files = 0_u64;
    for (key, value) in values {
        bytes = bytes.saturating_add(key.len());
        match value {
            ControlValue::Text(text) => bytes = bytes.saturating_add(text.len()),
            ControlValue::Checked(_) => {}
            ControlValue::Selected(values) => {
                entries = entries.saturating_add(values.len());
                for value in values {
                    bytes = bytes.saturating_add(value.len());
                }
            }
            ControlValue::Files(values) => {
                entries = entries.saturating_add(values.len());
                for file in values {
                    validate_file(file)?;
                    bytes = bytes
                        .saturating_add(file.name.len())
                        .saturating_add(file.content_type.len())
                        .saturating_add(file.token.len());
                    files = files.saturating_add(file.size);
                }
            }
        }
    }
    if bytes > limits.max_value_bytes
        || entries > limits.max_entries
        || files > limits.max_request_bytes
    {
        return Err(ActionError::Limit("semantic input bytes/entries"));
    }
    Ok(())
}
pub(super) fn validate_url(url: &str, limits: &ActionLimits) -> Result<(), ActionError> {
    if url.len() > limits.max_value_bytes {
        return Err(ActionError::Limit("URL bytes"));
    }
    if url.chars().any(char::is_control)
        || url.trim() != url
        || url.starts_with("//")
        || url.contains('\\')
    {
        return Err(ActionError::Invalid("unsafe URL".into()));
    }
    if let Some((scheme, _)) = url.split_once(':')
        && !scheme.contains(['/', '?', '#'])
        && !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https")
    {
        return Err(ActionError::Invalid("unsupported URL scheme".into()));
    }
    Ok(())
}
/// HTML form URL encoding (UTF-8, spaces as `+`, duplicate keys retained).
#[must_use]
pub fn url_encode(fields: &[(String, String)]) -> String {
    fn component(value: &str) -> String {
        let mut result = String::new();
        for byte in value.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'*' | b'-' | b'.' | b'_' => {
                    result.push(char::from(byte));
                }
                b' ' => result.push('+'),
                _ => {
                    const HEX: &[u8] = b"0123456789ABCDEF";
                    result.push('%');
                    result.push(char::from(HEX[usize::from(byte >> 4)]));
                    result.push(char::from(HEX[usize::from(byte & 15)]));
                }
            }
        }
        result
    }
    fields
        .iter()
        .map(|(key, value)| format!("{}={}", component(key), component(value)))
        .collect::<Vec<_>>()
        .join("&")
}
fn append_query(url: &str, query: &str) -> String {
    if query.is_empty() {
        return url.to_owned();
    }
    let (base, fragment) = url
        .split_once('#')
        .map_or((url, None), |(b, f)| (b, Some(f)));
    let separator = if base.ends_with(['?', '&']) {
        ""
    } else if base.contains('?') {
        "&"
    } else {
        "?"
    };
    format!(
        "{base}{separator}{query}{}",
        fragment.map_or(String::new(), |f| format!("#{f}"))
    )
}
fn text_content(node: &Container) -> String {
    let mut result = String::new();
    let mut stack = vec![node];
    while let Some(node) = stack.pop() {
        if let Element::Text { value } | Element::Raw { value } = &node.element {
            result.push_str(value);
        }
        stack.extend(node.children.iter().rev());
    }
    result
}

impl Work<'_> {
    pub(super) fn defaults(&mut self, node: &Container) -> Result<(), ActionError> {
        if matches!(self.event.kind, EventKind::Reset) {
            return self.reset_form();
        }
        if matches!(self.event.kind, EventKind::Submit) {
            return self.submit_form();
        }
        let route = node.route.as_ref().filter(|route| {
            let trigger = match route {
                Route::Get { trigger, .. }
                | Route::Post { trigger, .. }
                | Route::Put { trigger, .. }
                | Route::Delete { trigger, .. }
                | Route::Patch { trigger, .. } => trigger.as_deref(),
            };
            match (trigger, &self.event.kind) {
                (None | Some("click"), EventKind::Click)
                | (Some("load"), EventKind::Immediate)
                | (Some("change"), EventKind::Change)
                | (Some("resize"), EventKind::Resize) => true,
                (Some(name), EventKind::Named(event)) => name == event,
                _ => false,
            }
        });
        if let Some(route) = route {
            // Automatic load is once per retained source and exact route definition.
            // Named events (including an explicit "load") remain repeatable.
            let automatic = matches!(self.event.kind, EventKind::Immediate);
            let key = &self.event.source.key;
            if !automatic || self.engine.loaded_routes.get(key) != Some(route) {
                if automatic
                    && !self.engine.loaded_routes.contains_key(key)
                    && self.engine.loaded_routes.len() >= self.engine.limits.max_entries
                {
                    return Err(ActionError::Limit("loaded routes"));
                }
                self.route(route, Vec::new(), self.event.source.clone())?;
                if automatic {
                    self.engine
                        .loaded_routes
                        .insert(self.event.source.key.clone(), route.clone());
                }
            }
        }
        if !matches!(self.event.kind, EventKind::Click) {
            return Ok(());
        }
        match &node.element {
            Element::Anchor {
                href: Some(url),
                target,
            } => {
                validate_url(url, &self.engine.limits)?;
                self.emit(Effect::Navigate {
                    url: url.clone(),
                    base_url: self.event.location.clone(),
                    target: target.clone(),
                    source: self.event.source.clone(),
                })
            }
            Element::Button { r#type } => match r#type.as_deref().unwrap_or("submit") {
                "reset" => self.reset_form(),
                "submit" => {
                    if self.form_path().is_some() {
                        self.submit_form()
                    } else {
                        Ok(())
                    }
                }
                "button" => Ok(()),
                other => Err(ActionError::Invalid(format!("button type {other}"))),
            },
            _ => Ok(()),
        }
    }
    fn form_path(&self) -> Option<Vec<usize>> {
        (0..=self.event.source.path.len()).rev().find_map(|len| {
            let path = &self.event.source.path[..len];
            selectors::at(self.root, path)
                .filter(|n| matches!(n.element, Element::Form { .. }))
                .map(|_| path.to_vec())
        })
    }
    // One exhaustive control match keeps defaults, successful controls and reset values aligned.
    #[allow(clippy::too_many_lines)]
    fn fields(
        &mut self,
        form_path: &[usize],
        defaults: bool,
    ) -> Result<(Vec<FormPart>, FormValues), ActionError> {
        let mut parts = Vec::new();
        let mut reset = FormValues::new();
        let mut paths: Vec<_> = self
            .index
            .values()
            .filter(|p| p.starts_with(form_path))
            .cloned()
            .collect();
        paths.sort();
        for path in paths {
            self.step(0)?;
            // Nested forms are invalid HTML; do not leak their controls into an ancestor form.
            if (form_path.len() + 1..path.len()).any(|len| {
                selectors::at(self.root, &path[..len])
                    .is_some_and(|n| matches!(n.element, Element::Form { .. }))
            }) {
                continue;
            }
            let node = selectors::at(self.root, &path).ok_or(ActionError::InvalidSource)?;
            let key = crate::layout::node_key(node, &path);
            let mut option_values = Vec::new();
            let (name, initial, disabled) = match &node.element {
                Element::Input { input, name, .. } => (
                    name,
                    match input {
                        Input::Text { value, .. }
                        | Input::Password { value, .. }
                        | Input::Hidden { value } => {
                            ControlValue::Text(value.clone().unwrap_or_default())
                        }
                        Input::Checkbox { checked } => {
                            ControlValue::Checked(checked.unwrap_or(false))
                        }
                        Input::File => ControlValue::Files(Vec::new()),
                    },
                    false,
                ),
                Element::Textarea { name, value, .. } => {
                    (name, ControlValue::Text(value.clone()), false)
                }
                Element::Select {
                    name,
                    selected,
                    multiple,
                    disabled,
                    ..
                } => {
                    let mut selected_values = Vec::new();
                    let mut first = None;
                    let mut stack: Vec<_> = node.children.iter().rev().collect();
                    while let Some(option) = stack.pop() {
                        self.step(0)?;
                        if let Element::Option {
                            value,
                            disabled: option_disabled,
                        } = &option.element
                        {
                            if option_disabled == &Some(true) {
                                continue;
                            }
                            let value = value.clone().unwrap_or_else(|| text_content(option));
                            self.text(&value)?;
                            option_values.push(value.clone());
                            if first.is_none() {
                                first = Some(value.clone());
                            }
                            if selected.as_ref() == Some(&value) {
                                selected_values.push(value);
                            }
                        } else {
                            stack.extend(option.children.iter().rev());
                        }
                    }
                    if multiple != &Some(true) {
                        selected_values.truncate(1);
                        if selected_values.is_empty() && selected.is_none() {
                            selected_values.extend(first);
                        }
                    }
                    (
                        name,
                        ControlValue::Selected(selected_values),
                        disabled == &Some(true),
                    )
                }
                _ => continue,
            };
            let initial_kind = std::mem::discriminant(&initial);
            let current = if defaults {
                initial.clone()
            } else {
                self.values
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| initial.clone())
            };
            if std::mem::discriminant(&current) != initial_kind {
                return Err(ActionError::Invalid(
                    "semantic control type mismatch".into(),
                ));
            }
            if reset.len() >= self.engine.limits.max_entries {
                return Err(ActionError::Limit("form controls"));
            }
            reset.insert(key, initial);
            if disabled {
                continue;
            }
            let Some(name) = name.as_ref().filter(|n| !n.is_empty()) else {
                continue;
            };
            self.text(name)?;
            match current {
                ControlValue::Text(value) => {
                    self.text(&value)?;
                    parts.push(FormPart::Text {
                        name: name.clone(),
                        value,
                    });
                }
                ControlValue::Checked(true) => parts.push(FormPart::Text {
                    name: name.clone(),
                    value: "on".into(),
                }),
                ControlValue::Checked(false) => {}
                ControlValue::Selected(values) => {
                    if values.iter().any(|v| !option_values.contains(v)) {
                        return Err(ActionError::Invalid(
                            "selected value is absent or disabled".into(),
                        ));
                    }
                    if let Element::Select { multiple, .. } = &node.element
                        && *multiple != Some(true)
                        && values.len() > 1
                    {
                        return Err(ActionError::Invalid(
                            "multiple values for single select".into(),
                        ));
                    }
                    for value in values {
                        self.text(&value)?;
                        parts.push(FormPart::Text {
                            name: name.clone(),
                            value,
                        });
                    }
                }
                ControlValue::Files(files) => {
                    for file in files {
                        validate_file(&file)?;
                        parts.push(FormPart::File {
                            name: name.clone(),
                            file,
                        });
                    }
                }
            }
            if parts.len() > self.engine.limits.max_entries {
                return Err(ActionError::Limit("form parts"));
            }
        }
        validate_values(&reset, &self.engine.limits)?;
        Ok((parts, reset))
    }
    fn reset_form(&mut self) -> Result<(), ActionError> {
        let path = self
            .form_path()
            .ok_or_else(|| ActionError::Missing("ancestor form".into()))?;
        let (_, values) = self.fields(&path, true)?;
        self.emit(Effect::Reset {
            source: self.source(&path)?,
            values,
        })
    }
    fn submit_form(&mut self) -> Result<(), ActionError> {
        let path = self
            .form_path()
            .ok_or_else(|| ActionError::Missing("ancestor form".into()))?;
        let node = selectors::at(self.root, &path).ok_or(ActionError::InvalidSource)?;
        let Element::Form {
            action,
            method,
            multipart,
        } = &node.element
        else {
            return Err(ActionError::Invalid("not a form".into()));
        };
        let (parts, _) = self.fields(&path, false)?;
        if let Some(route) = &node.route {
            self.route(route, parts.clone(), self.source(&path)?)?;
        }
        if let Some(action) = action {
            let request = self.request(
                action,
                method.as_deref().unwrap_or("GET"),
                parts,
                *multipart,
                Selector::SelfTarget,
                SwapStrategy::This,
                true,
                self.source(&path)?,
            )?;
            self.emit(Effect::Request(request))?;
        } else if node.route.is_none() {
            let request = self.request(
                &self.event.location.clone(),
                method.as_deref().unwrap_or("GET"),
                parts,
                *multipart,
                Selector::SelfTarget,
                SwapStrategy::This,
                true,
                self.source(&path)?,
            )?;
            self.emit(Effect::Request(request))?;
        }
        Ok(())
    }
    fn route(
        &mut self,
        route: &Route,
        parts: Vec<FormPart>,
        source: Source,
    ) -> Result<(), ActionError> {
        let (method, url, target, strategy) = match route {
            Route::Get {
                route,
                target,
                strategy,
                ..
            } => ("GET", route, target, strategy),
            Route::Post {
                route,
                target,
                strategy,
                ..
            } => ("POST", route, target, strategy),
            Route::Put {
                route,
                target,
                strategy,
                ..
            } => ("PUT", route, target, strategy),
            Route::Delete {
                route,
                target,
                strategy,
                ..
            } => ("DELETE", route, target, strategy),
            Route::Patch {
                route,
                target,
                strategy,
                ..
            } => ("PATCH", route, target, strategy),
        };
        let multipart = parts.iter().any(|p| matches!(p, FormPart::File { .. }));
        let request = self.request(
            url,
            method,
            parts,
            multipart,
            target.clone(),
            strategy.clone(),
            false,
            source,
        )?;
        self.emit(Effect::Request(request))
    }
    #[allow(clippy::too_many_arguments)]
    fn request(
        &self,
        url: &str,
        method: &str,
        parts: Vec<FormPart>,
        multipart: bool,
        target: Selector,
        strategy: SwapStrategy,
        navigation: bool,
        source: Source,
    ) -> Result<RequestEffect, ActionError> {
        validate_url(url, &self.engine.limits)?;
        let method = method.to_ascii_uppercase();
        if !matches!(
            method.as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD"
        ) {
            return Err(ActionError::Invalid("HTTP method".into()));
        }
        let mut headers = BTreeMap::new();
        let mut size = 0_u64;
        let mut text = Vec::new();
        for part in &parts {
            match part {
                FormPart::Text { name, value } => {
                    size = size
                        .saturating_add(name.len() as u64)
                        .saturating_add(value.len() as u64);
                    text.push((name.clone(), value.clone()));
                }
                FormPart::File { name, file } => {
                    size = size
                        .saturating_add(name.len() as u64)
                        .saturating_add(file.size);
                    if method == "GET" || method == "HEAD" || !multipart {
                        return Err(ActionError::Invalid("files require multipart body".into()));
                    }
                }
            }
        }
        if size > self.engine.limits.max_request_bytes {
            return Err(ActionError::Limit("request bytes"));
        }
        let (url, body) = if method == "GET" || method == "HEAD" {
            (append_query(url, &url_encode(&text)), RequestBody::Empty)
        } else if multipart {
            (url.to_owned(), RequestBody::Multipart(parts))
        } else {
            let body = url_encode(&text);
            if body.len() as u64 > self.engine.limits.max_request_bytes {
                return Err(ActionError::Limit("encoded request bytes"));
            }
            headers.insert(
                "content-type".into(),
                "application/x-www-form-urlencoded;charset=UTF-8".into(),
            );
            (url.to_owned(), RequestBody::UrlEncoded(body))
        };
        validate_url(&url, &self.engine.limits)?;
        Ok(RequestEffect {
            source,
            url,
            base_url: self.event.location.clone(),
            method,
            body,
            headers,
            target,
            strategy,
            request_id: self.event.request_id.clone(),
            navigation,
        })
    }
}
