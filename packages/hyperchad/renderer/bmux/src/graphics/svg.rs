//! Static SVG decoding. No resource resolver is permitted to access the host.

use resvg::{tiny_skia, usvg};

use super::{GraphicsError, GraphicsStore};

const SVG_NAMESPACE: &str = "http://www.w3.org/2000/svg";
// Independent ceilings keep parser/CSS/path allocations bounded even when callers
// deliberately grant large raster budgets. These are not process isolation.
const MAX_SVG_BYTES: usize = 256 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_ATTRIBUTE_BYTES: usize = 64 * 1024;

fn invalid(message: &str) -> GraphicsError {
    GraphicsError::Decode(format!("SVG: {message}"))
}

impl GraphicsStore {
    pub(super) fn decode_svg(&mut self, key: &str, encoded: &[u8]) -> Result<(), GraphicsError> {
        if encoded.len() > MAX_SVG_BYTES {
            return Err(GraphicsError::Limit);
        }
        let text = std::str::from_utf8(encoded).map_err(|_| invalid("expected UTF-8"))?;
        // Legacy app icons declare a public SVG DTD. roxmltree never fetches it;
        // reject internal subsets/entities before allowing that declaration.
        if text.contains("<!ENTITY")
            || text.contains("<!ATTLIST")
            || text.contains("<!ELEMENT")
            || text.contains("<!NOTATION")
            || text.split("<!DOCTYPE").skip(1).any(|s| {
                s.split('>')
                    .next()
                    .is_none_or(|declaration| declaration.contains('['))
            })
        {
            return Err(invalid("DTD subsets and entities are disabled"));
        }
        let node_limit =
            u32::try_from(self.limits.max_actions.min(4096)).map_err(|_| GraphicsError::Limit)?;
        let document = usvg::roxmltree::Document::parse_with_options(
            text,
            usvg::roxmltree::ParsingOptions {
                allow_dtd: true,
                nodes_limit: node_limit,
                ..Default::default()
            },
        )
        .map_err(|error| match error {
            usvg::roxmltree::Error::NodesLimitReached => GraphicsError::Limit,
            other => invalid(&other.to_string()),
        })?;
        let root = document.root_element();
        if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NAMESPACE) {
            return Err(invalid("expected an SVG root"));
        }
        let (elements, complexity) = preflight(&document, node_limit)?;
        let options = usvg::Options {
            resources_dir: None,
            default_size: usvg::Size::from_wh(100.0, 100.0).ok_or(GraphicsError::InvalidSize)?,
            image_href_resolver: usvg::ImageHrefResolver {
                resolve_data: Box::new(|_, _, _| None),
                resolve_string: Box::new(|_, _| None),
            },
            ..Default::default()
        };
        let tree = usvg::Tree::from_xmltree(&document, &options)
            .map_err(|error| invalid(&error.to_string()))?;
        let size = tree.size().to_int_size();
        let (width, height) = (size.width(), size.height());
        let output_bytes = self.size(width, height)?;
        let mut allocation = u64::try_from(output_bytes).map_err(|_| GraphicsError::Limit)?;
        check_layers(tree.root(), self.limits.max_dimension, &mut allocation)?;
        if allocation > self.limits.max_resource_bytes as u64
            || allocation > self.limits.max_total_bytes as u64
        {
            return Err(GraphicsError::Limit);
        }
        // Every element can visit the output; geometry flattening is charged by
        // token count times maximum dimension. No recursive/effect elements are
        // allowed, so this deliberately conservative estimate has no expansion.
        let work = u64::from(width)
            .saturating_mul(u64::from(height))
            .saturating_mul(elements)
            .saturating_add(complexity.saturating_mul(u64::from(width.max(height))));
        if work > self.limits.max_pixel_visits {
            return Err(GraphicsError::Limit);
        }
        let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or(GraphicsError::Limit)?;
        resvg::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut pixmap.as_mut(),
        );
        // tiny-skia stores premultiplied sRGB; bmux expects straight alpha.
        // Its maintained conversion also normalizes fully transparent pixels.
        self.register_rgba(key, width, height, pixmap.take_demultiplied())
    }
}

fn preflight(
    document: &usvg::roxmltree::Document<'_>,
    node_limit: u32,
) -> Result<(u64, u64), GraphicsError> {
    let root = document.root_element();
    let mut elements = 0_u64;
    let mut complexity = 0_u64;
    let mut attribute_bytes = 0_usize;
    for node in document
        .descendants()
        .filter(usvg::roxmltree::Node::is_element)
    {
        if node.ancestors().count() > MAX_DEPTH {
            return Err(GraphicsError::Limit);
        }
        let name = node.tag_name().name();
        // Explicit static vocabulary: no scripts, foreignObject, image,
        // animation, recursive use, offscreen effects, or nested viewports.
        if node.tag_name().namespace() != Some(SVG_NAMESPACE)
            || !matches!(
                name,
                "svg"
                    | "g"
                    | "defs"
                    | "path"
                    | "rect"
                    | "circle"
                    | "ellipse"
                    | "line"
                    | "polyline"
                    | "polygon"
                    | "style"
                    | "title"
                    | "desc"
                    | "metadata"
            )
            || (name == "svg" && node != root)
        {
            return Err(invalid("unsupported or active element"));
        }
        elements += 1;
        if node.attributes().len() > 64 {
            return Err(GraphicsError::Limit);
        }
        for attribute in node.attributes() {
            attribute_bytes = attribute_bytes.saturating_add(attribute.value().len());
            if attribute_bytes > MAX_ATTRIBUTE_BYTES {
                return Err(GraphicsError::Limit);
            }
            let name = attribute.name();
            if name.starts_with("on") || matches!(name, "href" | "base") {
                return Err(invalid(
                    "event handlers and resource references are disabled",
                ));
            }
            check_static_value(attribute.value())?;
            complexity = complexity.saturating_add(value_complexity(attribute.value()));
        }
        if name == "style" {
            let css = node.text().unwrap_or_default();
            check_static_value(css)?;
            // CSS nesting/selector explosion is not needed by static icons.
            if css.matches('{').count() > 64 || css.matches(',').count() > 64 {
                return Err(GraphicsError::Limit);
            }
            complexity = complexity.saturating_add(value_complexity(css));
        }
    }
    if complexity > u64::from(node_limit).saturating_mul(16) {
        return Err(GraphicsError::Limit);
    }
    Ok((elements, complexity))
}

fn check_layers(
    group: &usvg::Group,
    max_dimension: u32,
    allocation: &mut u64,
) -> Result<(), GraphicsError> {
    if group.should_isolate() {
        // resvg pads isolation layers by two pixels per edge. Charge every
        // layer, not just the simultaneously live subset; never permit its
        // larger (4x-canvas) emergency limit to override our resource budget.
        let bounds = group.abs_layer_bounding_box();
        let width = padded_layer_dimension(bounds.width(), max_dimension)?;
        let height = padded_layer_dimension(bounds.height(), max_dimension)?;
        *allocation = allocation.saturating_add(width.saturating_mul(height).saturating_mul(4));
    }
    for child in group.children() {
        if let usvg::Node::Group(group) = child {
            check_layers(group, max_dimension, allocation)?;
        }
    }
    Ok(())
}

fn padded_layer_dimension(value: f32, max_dimension: u32) -> Result<u64, GraphicsError> {
    // Compare in f64 so rounding u32::MAX to f32 cannot enlarge the budget.
    // Reject before the float-to-integer cast, which otherwise saturates huge
    // finite inputs to u64::MAX and makes even small padding overflow.
    let rounded = f64::from(value).ceil();
    if !rounded.is_finite() || rounded <= 0.0 || rounded > f64::from(max_dimension) {
        return Err(GraphicsError::Limit);
    }
    let padded = (rounded as u64)
        .checked_add(4)
        .ok_or(GraphicsError::Limit)?;
    if padded > u64::from(max_dimension) {
        return Err(GraphicsError::Limit);
    }
    Ok(padded)
}

#[cfg(test)]
#[path = "svg_tests.rs"]
mod tests;

fn check_static_value(value: &str) -> Result<(), GraphicsError> {
    // Disallow CSS escapes/comments as well as all URL functions, including
    // fragment URLs: effects/paint-server expansion is intentionally unsupported.
    let lower = value.to_ascii_lowercase();
    if lower.contains("url") || lower.contains('@') || lower.contains('\\') || lower.contains("/*")
    {
        return Err(invalid("CSS resources, imports, and escapes are disabled"));
    }
    Ok(())
}

fn value_complexity(value: &str) -> u64 {
    // Count numeric runs and commands, including implicit repeated path commands.
    let mut previous_numeric = false;
    let mut count = 0;
    for byte in value.bytes() {
        let numeric = byte.is_ascii_digit() || matches!(byte, b'.' | b'+' | b'-');
        if (numeric && !previous_numeric) || byte.is_ascii_alphabetic() {
            count += 1;
        }
        previous_numeric = numeric;
    }
    count
}
