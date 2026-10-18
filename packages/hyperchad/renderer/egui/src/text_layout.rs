//! Explicit native faces and shared shaping.
//!
//! The compatibility proportional chain is Ubuntu-Light-based (300), not an
//! assertion that a regular/400 face exists.

use std::{collections::BTreeMap, fmt, sync::Arc};

use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, text::LayoutJob};
use hyperchad_transformer::{
    layout::font::{TextFontFamily, TextFontWeight, TextMeasureError, TextMeasureRequest},
    models::WhiteSpace,
};

/// Invalid or conflicting caller-supplied face metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontRegistrationError(pub String);

impl fmt::Display for FontRegistrationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for FontRegistrationError {}

/// Immutable faces shared by native measurement and paint.
///
/// Face weights are caller assertions about supplied bytes. No synthetic weight,
/// filename inference, system discovery or nearest-weight substitution is used.
#[derive(Clone)]
pub struct FontRegistry {
    definitions: FontDefinitions,
    faces: BTreeMap<(String, u16), FontFamily>,
    defaults: BTreeMap<String, u16>,
    aliases: BTreeMap<String, String>,
    compatibility_face: Option<(String, u16)>,
}

/// Mutable registration phase; install only the completed immutable registry.
pub struct FontRegistryBuilder {
    registry: FontRegistry,
    fallbacks: BTreeMap<(String, u16), Vec<(String, u16)>>,
}

impl FontRegistry {
    /// Preserves egui's default proportional/monospace glyph fallback chains.
    ///
    /// # Panics
    /// * Bundled face metadata or the internally constructed registry is invalid.
    #[must_use]
    pub fn compatibility() -> Self {
        let mut builder = Self::builder();
        let bundled = builder.registry.definitions.font_data.clone();
        for (family, weight, name) in [("Ubuntu Light", 300, "Ubuntu-Light"), ("Hack", 400, "Hack")]
        {
            if let Some(data) = bundled.get(name) {
                // Known metadata for egui's bundled assets, not name inference.
                builder
                    .register_face(family, weight, data.as_ref().clone())
                    .expect("bundled face metadata");
                builder.set_default_face(family, weight);
            }
        }
        builder.alias("sans-serif", "Ubuntu Light");
        builder.alias("monospace", "Hack");
        let mut registry = builder.build().expect("bundled registry");
        // These default families already exist in an unmodified egui context,
        // so the compatibility registry works without a queued font install.
        // The asserted primary faces are the exact bundled 300/400 assets.
        registry
            .faces
            .insert(("Ubuntu Light".to_owned(), 300), FontFamily::Proportional);
        registry
            .faces
            .insert(("Hack".to_owned(), 400), FontFamily::Monospace);
        registry
    }

    /// Starts with unchanged egui compatibility chains and no authored faces.
    #[must_use]
    pub fn builder() -> FontRegistryBuilder {
        FontRegistryBuilder {
            fallbacks: BTreeMap::new(),
            registry: Self {
                definitions: FontDefinitions::default(),
                faces: BTreeMap::new(),
                defaults: BTreeMap::new(),
                aliases: BTreeMap::new(),
                compatibility_face: None,
            },
        }
    }

    /// Actual asserted primary weight of the compatibility chain.
    #[must_use]
    pub fn compatibility_weight(&self) -> Option<u16> {
        self.compatibility_face
            .as_ref()
            .map(|(_, weight)| *weight)
            .or_else(|| {
                self.definitions
                    .font_data
                    .contains_key("Ubuntu-Light")
                    .then_some(300)
            })
    }

    /// Installs definitions and the registry before the first egui pass.
    ///
    /// `set_fonts` takes effect at pass initialization. Do not replace fonts or
    /// this registry independently after initialization.
    pub fn install(self: &Arc<Self>, ctx: &egui::Context) {
        ctx.set_fonts(self.definitions.clone());
        ctx.data_mut(|data| data.insert_temp(registry_id(), Arc::clone(self)));
    }

    /// Retrieves the installed registry, or the bundled compatibility policy.
    #[must_use]
    pub fn for_context(ctx: &egui::Context) -> Arc<Self> {
        ctx.data(|data| data.get_temp::<Arc<Self>>(registry_id()))
            .unwrap_or_else(|| Arc::new(Self::compatibility()))
    }

    /// Resolves exact faces; `None` retains unspecified-style provenance.
    ///
    /// # Errors
    /// * No requested family has a registered face at the exact requested weight.
    /// * An authored family has no explicitly declared default face.
    /// * Size is nonpositive/nonfinite, or weight is outside 100–900.
    pub fn resolve(
        &self,
        families: Option<&[String]>,
        weight: Option<u16>,
        size: f32,
    ) -> Result<FontId, String> {
        if !size.is_finite() || size <= 0.0 {
            return Err("native font size must be finite and positive".to_owned());
        }
        if weight.is_some_and(|weight| !(100..=900).contains(&weight)) {
            return Err("native font weight must be in 100..=900".to_owned());
        }
        if families.is_none() && weight.is_none() {
            let family = self
                .compatibility_face
                .as_ref()
                .map_or(FontFamily::Proportional, |key| self.faces[key].clone());
            return Ok(FontId::new(size, family));
        }
        let generic = [self
            .compatibility_face
            .as_ref()
            .map_or_else(|| "sans-serif".to_owned(), |(family, _)| family.clone())];
        let families = families.unwrap_or(&generic);
        for requested in families {
            let family = self.aliases.get(requested).unwrap_or(requested);
            if let Some(weight) = weight.or_else(|| self.defaults.get(family).copied())
                && let Some(face) = self.faces.get(&(family.clone(), weight))
            {
                return Ok(FontId::new(size, face.clone()));
            }
        }
        Err(format!(
            "no registered native face for families {families:?}, weight {weight:?}; unspecified family defaults must be explicitly registered"
        ))
    }
}

impl FontRegistryBuilder {
    /// Registers genuine bytes and asserted metadata, without parsing heuristics.
    ///
    /// # Errors
    /// * Family is empty, weight is outside 100–900, bytes are empty, or the face
    ///   is already registered.
    pub fn register_face(
        &mut self,
        family: &str,
        weight: u16,
        data: FontData,
    ) -> Result<(), FontRegistrationError> {
        let key = (family.to_owned(), weight);
        if family.trim().is_empty()
            || !(100..=900).contains(&weight)
            || data.font.is_empty()
            || self.registry.faces.contains_key(&key)
        {
            return Err(FontRegistrationError(format!(
                "invalid or duplicate face {family:?}/{weight}"
            )));
        }
        let name = format!("hyperchad-face-{}", self.registry.faces.len());
        self.registry
            .definitions
            .font_data
            .insert(name.clone(), Arc::new(data));
        let face = FontFamily::Name(name.clone().into());
        // Preserve egui's emoji fallback data/tweaks; never append a different
        // Latin weight behind an explicitly requested primary face.
        let mut chain = vec![name];
        for fallback in ["NotoEmoji-Regular", "emoji-icon-font"] {
            if self.registry.definitions.font_data.contains_key(fallback) {
                chain.push(fallback.to_owned());
            }
        }
        self.registry
            .definitions
            .families
            .insert(face.clone(), chain);
        self.registry.faces.insert(key, face);
        Ok(())
    }

    /// Selects an actual registered face for unspecified compatibility styles.
    /// The selected bytes and weight are validated together by `build`.
    pub fn set_compatibility_face(&mut self, family: &str, weight: u16) {
        self.registry.compatibility_face = Some((family.to_owned(), weight));
    }

    /// Declares a family's default face; validated by `build`.
    pub fn set_default_face(&mut self, family: &str, weight: u16) {
        self.registry.defaults.insert(family.to_owned(), weight);
    }

    /// Declares a direct generic or authored alias; alias chains are unsupported.
    pub fn alias(&mut self, alias: &str, family: &str) {
        self.registry
            .aliases
            .insert(alias.to_owned(), family.to_owned());
    }

    /// Declares ordered glyph fallback faces with caller-asserted metadata.
    /// These are used only for missing glyphs, never to resolve a missing weight.
    pub fn set_fallback_faces(&mut self, family: &str, weight: u16, faces: Vec<(String, u16)>) {
        self.fallbacks.insert((family.to_owned(), weight), faces);
    }

    /// Freezes registration.
    ///
    /// # Errors
    /// * A declared default or alias does not reference a registered face/family.
    /// * An alias shadows a registered family.
    ///
    /// # Panics
    /// * Internal registered-face and font-definition mappings are inconsistent.
    pub fn build(mut self) -> Result<FontRegistry, FontRegistrationError> {
        if let Some(key) = &self.registry.compatibility_face
            && !self.registry.faces.contains_key(key)
        {
            return Err(FontRegistrationError(format!(
                "missing compatibility face {key:?}"
            )));
        }
        for (primary, fallbacks) in &self.fallbacks {
            let face = self
                .registry
                .faces
                .get(primary)
                .ok_or_else(|| {
                    FontRegistrationError(format!("missing fallback primary {primary:?}"))
                })?
                .clone();
            let mut names = Vec::new();
            for fallback in fallbacks {
                let fallback_face = self.registry.faces.get(fallback).ok_or_else(|| {
                    FontRegistrationError(format!("missing fallback face {fallback:?}"))
                })?;
                let name = self.registry.definitions.families[fallback_face][0].clone();
                names.push(name);
            }
            let chain = self
                .registry
                .definitions
                .families
                .get_mut(&face)
                .expect("registered family");
            chain.splice(1..1, names);
        }
        for (family, weight) in &self.registry.defaults {
            if !self.registry.faces.contains_key(&(family.clone(), *weight)) {
                return Err(FontRegistrationError(format!(
                    "missing default face {family}/{weight}"
                )));
            }
        }
        for (alias, target) in &self.registry.aliases {
            if alias.trim().is_empty()
                || !self
                    .registry
                    .faces
                    .keys()
                    .any(|(family, _)| family == target)
                || self
                    .registry
                    .faces
                    .keys()
                    .any(|(family, _)| family == alias)
            {
                return Err(FontRegistrationError(format!(
                    "invalid font alias {alias:?} -> {target:?}"
                )));
            }
        }
        Ok(self.registry)
    }
}

fn registry_id() -> egui::Id {
    egui::Id::new("hyperchad-native-font-registry")
}

/// Shared ASCII whitespace policy. NBSP and other Unicode spaces are retained.
#[must_use]
pub fn normalize_whitespace(text: &str, policy: WhiteSpace) -> String {
    match policy {
        WhiteSpace::Preserve | WhiteSpace::PreserveWrap => text.to_owned(),
        WhiteSpace::Normal => {
            let mut result = String::new();
            let mut previous_space = false;
            for character in text.chars() {
                let space = matches!(character, ' ' | '\t' | '\n' | '\r' | '\u{000c}');
                if !space || !previous_space {
                    result.push(if space { ' ' } else { character });
                }
                previous_space = space;
            }
            result
        }
    }
}

/// Constructs the same intrinsic shaping job for measurement and paint.
///
/// # Errors
/// * Wrap width is negative, NaN or negative infinity.
/// * The exact authored face cannot be resolved.
pub fn layout_job(
    registry: &FontRegistry,
    text: &str,
    families: Option<&[String]>,
    weight: Option<u16>,
    size: f32,
    wrap_width: f32,
    white_space: WhiteSpace,
) -> Result<LayoutJob, String> {
    if wrap_width.is_nan() || wrap_width < 0.0 {
        return Err("native wrap width must be nonnegative or positive infinity".to_owned());
    }
    let font = registry.resolve(families, weight, size)?;
    let width = if white_space == WhiteSpace::Preserve {
        f32::INFINITY
    } else {
        wrap_width
    };
    let mut job = LayoutJob::simple(
        normalize_whitespace(text, white_space),
        font,
        Color32::WHITE,
        width,
    );
    job.round_output_to_gui = false;
    Ok(job)
}

/// Builds an intrinsic job from the shared checked measurement contract.
///
/// # Errors
/// * Invalid logical dimensions/weight or unavailable exact authored faces.
pub fn request_job(
    registry: &FontRegistry,
    request: &TextMeasureRequest<'_>,
) -> Result<LayoutJob, TextMeasureError> {
    if !request.size.is_finite()
        || request.size <= 0.0
        || request.wrap_width.is_nan()
        || request.wrap_width < 0.0
        || matches!(request.weight, TextFontWeight::Absolute(weight) if !(100..=900).contains(&weight))
    {
        return Err(TextMeasureError::InvalidRequest {
            message: "invalid native text size, wrap width or weight".to_owned(),
        });
    }
    let families = match request.family {
        TextFontFamily::CompatibilityDefault => None,
        TextFontFamily::Authored(families) => Some(families),
    };
    let weight = match request.weight {
        TextFontWeight::CompatibilityDefault => None,
        TextFontWeight::Absolute(weight) => Some(weight),
    };
    layout_job(
        registry,
        request.text,
        families,
        weight,
        request.size,
        request.wrap_width,
        request.whitespace.unwrap_or_default(),
    )
    .map_err(|message| TextMeasureError::UnsupportedStyle { message })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_is_not_an_authored_regular_face() {
        let registry = FontRegistry::compatibility();
        assert_eq!(
            registry.resolve(None, None, 14.0).unwrap().family,
            FontFamily::Proportional
        );
        assert_eq!(
            registry.resolve(None, Some(400), 14.0).unwrap_err(),
            "no registered native face for families [\"sans-serif\"], weight Some(400); unspecified family defaults must be explicitly registered"
        );
        assert!(registry.resolve(None, Some(700), 14.0).is_err());
        assert!(
            registry
                .resolve(Some(&["Ubuntu Light".to_owned()]), Some(300), 14.0)
                .is_ok()
        );
        assert!(
            registry
                .resolve(Some(&["missing".to_owned()]), None, 14.0)
                .is_err()
        );
    }

    #[test]
    fn custom_compatibility_face_reports_its_actual_weight() {
        let bundled = FontDefinitions::default();
        let mut builder = FontRegistry::builder();
        builder
            .register_face(
                "Custom Mono",
                400,
                bundled.font_data["Hack"].as_ref().clone(),
            )
            .unwrap();
        builder.set_compatibility_face("Custom Mono", 400);
        let registry = builder.build().unwrap();
        assert_eq!(registry.compatibility_weight(), Some(400));
        assert_eq!(
            registry.resolve(None, None, 14.0).unwrap().family,
            registry
                .resolve(Some(&["Custom Mono".to_owned()]), Some(400), 14.0)
                .unwrap()
                .family
        );
        assert_eq!(
            FontRegistry::compatibility().compatibility_weight(),
            Some(300)
        );
    }

    #[test]
    fn whitespace_policy_and_logical_rounding_are_shared() {
        let registry = FontRegistry::compatibility();
        for policy in [
            WhiteSpace::Normal,
            WhiteSpace::Preserve,
            WhiteSpace::PreserveWrap,
        ] {
            let job =
                layout_job(&registry, " a\n b\u{a0}c ", None, None, 14.0, 20.0, policy).unwrap();
            assert!(!job.round_output_to_gui);
            if policy == WhiteSpace::Normal {
                assert_eq!(job.text, " a b\u{a0}c ");
            } else {
                assert_eq!(job.text, " a\n b\u{a0}c ");
            }
            assert_eq!(
                job.wrap.max_width.is_infinite(),
                policy == WhiteSpace::Preserve
            );
        }
    }
}
