//! Layout calculation implementation for containers.
//!
//! This module provides the [`Calculator`](crate::layout::calc::Calculator) type for performing layout calculations
//! on container trees, including width/height calculation, flexbox layout, margin/padding,
//! and element positioning.

use bumpalo::Bump;
use paste::paste;

use crate::Container;

use super::font::{
    FontMetricsBounds, TextFontFamily, TextFontWeight, TextMeasureError, TextMeasureRequest,
};
use super::{Calc, LayoutError, LayoutErrorKind, TableSpanAxis, font::FontMetrics};

fn reset_text_wrap_metadata(root: &mut Container) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        node.calculated_text_wrap_width = None;
        stack.extend(node.children.iter_mut());
    }
}

#[derive(Clone)]
struct TextStyle {
    family: Option<std::rc::Rc<Vec<String>>>,
    weight: TextFontWeight,
    whitespace: Option<hyperchad_transformer_models::WhiteSpace>,
    size: f32,
    path: Vec<usize>,
}

// One style/path index per calculation; no Container cloning and no mutation of
// authored styles. Node addresses are stable throughout these layout passes.
struct TextMeasurements<'a, F: FontMetrics> {
    metrics: &'a F,
    styles: std::collections::BTreeMap<usize, TextStyle>,
    failure: std::cell::RefCell<Option<LayoutError>>,
}

impl<'a, F: FontMetrics> TextMeasurements<'a, F> {
    fn new(
        metrics: &'a F,
        root: &Container,
        defaults: CalculatorDefaults,
    ) -> Result<Self, LayoutError> {
        let mut styles = std::collections::BTreeMap::new();
        let initial = TextStyle {
            family: None,
            weight: TextFontWeight::CompatibilityDefault,
            whitespace: None,
            size: defaults.font_size,
            path: Vec::new(),
        };
        let vw = root.calculated_width.unwrap_or_default();
        let vh = root.calculated_height.unwrap_or_default();
        let mut stack = vec![(root, initial)];
        while let Some((node, mut style)) = stack.pop() {
            if let Some(family) = &node.font_family {
                style.family = Some(std::rc::Rc::new(family.clone()));
            }
            if let Some(weight) = node.font_weight {
                style.weight = super::font::resolve_text_weight(
                    weight,
                    style.weight,
                    metrics.compatibility_weight(),
                )
                .map_err(|source| LayoutError {
                    path: style.path.clone(),
                    kind: LayoutErrorKind::TextMeasurement(source),
                })?;
            }
            if let Some(whitespace) = node.white_space {
                style.whitespace = Some(whitespace);
            }
            if let Some(size) = &node.font_size {
                style.size = size.calc(style.size, vw, vh);
            } else if let crate::Element::Heading { size } = node.element {
                style.size = match size {
                    crate::HeaderSize::H1 => defaults.h1_font_size,
                    crate::HeaderSize::H2 => defaults.h2_font_size,
                    crate::HeaderSize::H3 => defaults.h3_font_size,
                    crate::HeaderSize::H4 => defaults.h4_font_size,
                    crate::HeaderSize::H5 => defaults.h5_font_size,
                    crate::HeaderSize::H6 => defaults.h6_font_size,
                };
            }
            for (index, child) in node.children.iter().enumerate().rev() {
                let mut inherited = style.clone();
                inherited.path.push(index);
                stack.push((child, inherited));
            }
            styles.insert(std::ptr::from_ref(node).addr(), style);
        }
        Ok(Self {
            metrics,
            styles,
            failure: std::cell::RefCell::new(None),
        })
    }

    fn measure(
        &self,
        node: &Container,
        text: &str,
        width: f32,
    ) -> Result<FontMetricsBounds, LayoutError> {
        let style = &self.styles[&std::ptr::from_ref(node).addr()];
        let request = TextMeasureRequest {
            text,
            family: style
                .family
                .as_ref()
                .map_or(TextFontFamily::CompatibilityDefault, |names| {
                    TextFontFamily::Authored(names)
                }),
            weight: style.weight,
            size: style.size,
            wrap_width: width,
            whitespace: style.whitespace,
        };
        let fail = |source| LayoutError {
            path: style.path.clone(),
            kind: LayoutErrorKind::TextMeasurement(source),
        };
        if !request.size.is_finite()
            || request.size <= 0.0
            || request.wrap_width.is_nan()
            || request.wrap_width < 0.0
        {
            return Err(fail(TextMeasureError::InvalidRequest {
                message: "text size must be positive and finite; wrap width must be nonnegative"
                    .into(),
            }));
        }
        let bounds = self.metrics.measure_text(&request).map_err(fail)?;
        if bounds.rows.iter().any(|row| {
            !row.width.is_finite() || !row.height.is_finite() || row.width < 0.0 || row.height < 0.0
        }) || !bounds.height().is_finite()
        {
            return Err(fail(TextMeasureError::Backend {
                message: "non-finite or negative logical text bounds".into(),
            }));
        }
        Ok(bounds)
    }

    // Shared content-only control policy. Native chrome must fit the allocated
    // box; these are logical font units, not backend widget minimum sizes.
    fn control_size(&self, node: &Container) -> Result<Option<(f32, f32)>, LayoutError> {
        use crate::{Element, Input, Number};
        let style = &self.styles[&std::ptr::from_ref(node).addr()];
        let count = |value: Option<&Number>, default: f32| -> Result<f32, LayoutError> {
            let value = match value {
                None => default,
                Some(value @ (Number::Integer(_) | Number::Real(_))) => value.calc(0.0, 0.0, 0.0),
                _ => {
                    return Err(LayoutError {
                        path: style.path.clone(),
                        kind: LayoutErrorKind::TextMeasurement(TextMeasureError::InvalidRequest {
                            message:
                                "control rows/columns must be positive integral unitless counts"
                                    .into(),
                        }),
                    });
                }
            };
            if !value.is_finite() || value <= 0.0 || value.fract() != 0.0 {
                return Err(LayoutError {
                    path: style.path.clone(),
                    kind: LayoutErrorKind::TextMeasurement(TextMeasureError::InvalidRequest {
                        message: "control rows/columns must be positive integral unitless counts"
                            .into(),
                    }),
                });
            }
            Ok(value)
        };
        let (columns, rows) = match &node.element {
            Element::Input {
                input: Input::Hidden { .. },
                ..
            } => return Ok(Some((0.0, 0.0))),
            Element::Input {
                input: Input::Checkbox { .. },
                ..
            } => return Ok(Some((style.size, style.size))),
            Element::Input {
                input: Input::Text { .. } | Input::Password { .. },
                ..
            } => (20.0, 1.0),
            Element::Textarea { rows, cols, .. } => {
                (count(cols.as_ref(), 20.0)?, count(rows.as_ref(), 2.0)?)
            }
            // Select/file chrome has no approved shared policy. Do not pretend
            // these controls are text editors; explicit authored sizes remain.
            _ => return Ok(None),
        };
        if node.width.is_some() && node.height.is_some() {
            return Ok(None);
        }
        let bounds = self.measure(node, "0", f32::INFINITY)?;
        let width = bounds.width() * columns;
        let height = bounds.height() * rows;
        if !width.is_finite() || !height.is_finite() {
            return Err(LayoutError {
                path: style.path.clone(),
                kind: LayoutErrorKind::InvalidGeometry,
            });
        }
        Ok(Some((width, height)))
    }

    fn record(&self, error: LayoutError) {
        let mut failure = self.failure.borrow_mut();
        if failure.is_none() {
            *failure = Some(error);
        }
    }

    fn check(&self) -> Result<(), LayoutError> {
        self.failure.borrow_mut().take().map_or(Ok(()), Err)
    }

    fn failed(&self) -> bool {
        self.failure.borrow().is_some()
    }
}

// Table tracks are solved in the shared passes, before text reflow and ancestor
// allocation. Paths (never IDs) identify cells, including duplicate-ID trees.
mod table_layout {
    use std::collections::BTreeSet;

    use super::{Container, FontMetrics, LayoutError, LayoutErrorKind, TableSpanAxis};
    use crate::{Element, Number};

    // Bound each axis at 1024, grid slots at 65536, and cumulative grid
    // slots at 262144 per checked calculation. Reject, never truncate spans.
    const MAX_TRACKS: usize = 1024;
    const MAX_SLOTS: usize = 65_536;
    const MAX_TOTAL_SLOTS: usize = 262_144;

    struct Cell {
        path: Vec<usize>,
        row: usize,
        column: usize,
        rows: usize,
        columns: usize,
    }

    struct Grid {
        rows: Vec<Vec<usize>>,
        cells: Vec<Cell>,
        columns: usize,
        slots: usize,
    }

    fn error(path: &[usize], kind: LayoutErrorKind) -> LayoutError {
        LayoutError {
            path: path.to_vec(),
            kind,
        }
    }

    fn span(
        value: Option<&Number>,
        axis: TableSpanAxis,
        path: &[usize],
    ) -> Result<usize, LayoutError> {
        let invalid = || error(path, LayoutErrorKind::InvalidTableSpan { axis });
        let value = match value {
            None => return Ok(1),
            Some(Number::Integer(value)) if *value > 0 => {
                usize::try_from(*value).map_err(|_| invalid())?
            }
            Some(Number::Real(value))
                if value.is_finite() && *value > 0.0 && value.fract() == 0.0 =>
            {
                // Check the allocation bound before the conversion, including on
                // platforms where usize cannot represent every finite f32.
                if *value > track_count(MAX_TRACKS) {
                    return Err(error(path, LayoutErrorKind::TableBudgetExceeded));
                }
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                {
                    *value as usize
                }
            }
            _ => return Err(invalid()),
        };
        if value > MAX_TRACKS {
            return Err(error(path, LayoutErrorKind::TableBudgetExceeded));
        }
        Ok(value)
    }

    fn at<'a>(mut root: &'a Container, path: &[usize]) -> &'a Container {
        for index in path {
            root = &root.children[*index];
        }
        root
    }

    fn at_mut<'a>(mut root: &'a mut Container, path: &[usize]) -> &'a mut Container {
        for index in path {
            root = &mut root.children[*index];
        }
        root
    }

    fn cell_spans(cell: &Container, path: &[usize]) -> Result<(usize, usize), LayoutError> {
        match &cell.element {
            Element::TH { rows, columns } | Element::TD { rows, columns } => Ok((
                span(rows.as_ref(), TableSpanAxis::Row, path)?,
                span(columns.as_ref(), TableSpanAxis::Column, path)?,
            )),
            _ => Err(error(path, LayoutErrorKind::InvalidTableStructure)),
        }
    }

    fn grid(table: &Container) -> Result<Grid, LayoutError> {
        let mut rows = Vec::new();
        let mut group_ends = Vec::new();
        for (index, child) in table.children.iter().enumerate() {
            match child.element {
                Element::TR => {
                    if !child.is_hidden() {
                        rows.push(vec![index]);
                    }
                }
                Element::THead | Element::TBody => {
                    if group_ends.last().copied().unwrap_or_default() < rows.len() {
                        group_ends.push(rows.len());
                    }
                    for (row_index, row) in child.children.iter().enumerate() {
                        if row.element != Element::TR {
                            return Err(error(
                                &[index, row_index],
                                LayoutErrorKind::InvalidTableStructure,
                            ));
                        }
                        if !child.is_hidden() && !row.is_hidden() {
                            rows.push(vec![index, row_index]);
                        }
                        if rows.len() > MAX_TRACKS {
                            return Err(error(
                                &[index, row_index],
                                LayoutErrorKind::TableBudgetExceeded,
                            ));
                        }
                    }
                    group_ends.push(rows.len());
                }
                _ => return Err(error(&[index], LayoutErrorKind::InvalidTableStructure)),
            }
            if rows.len() > MAX_TRACKS {
                return Err(error(&[index], LayoutErrorKind::TableBudgetExceeded));
            }
        }
        group_ends.push(rows.len());
        if rows.len() > MAX_TRACKS {
            return Err(error(&[], LayoutErrorKind::TableBudgetExceeded));
        }
        let mut cells = Vec::new();
        let mut occupied = BTreeSet::new();
        let mut columns = 0;
        for (row_index, path) in rows.iter().enumerate() {
            let row = at(table, path);
            let end = *group_ends
                .iter()
                .find(|end| **end > row_index)
                .unwrap_or(&rows.len());
            let mut column = 0;
            for (index, cell) in row.children.iter().enumerate() {
                let mut path = path.clone();
                path.push(index);
                let (row_span, column_span) = cell_spans(cell, &path)?;
                if cell.is_hidden() {
                    continue;
                }
                if row_index + row_span > end {
                    return Err(error(&path, LayoutErrorKind::InvalidTableStructure));
                }
                while column < MAX_TRACKS
                    && (column..column + column_span).any(|x| occupied.contains(&(row_index, x)))
                {
                    column += 1;
                }
                if column + column_span > MAX_TRACKS
                    || occupied.len() + row_span * column_span > MAX_SLOTS
                {
                    return Err(error(&path, LayoutErrorKind::TableBudgetExceeded));
                }
                for y in row_index..row_index + row_span {
                    for x in column..column + column_span {
                        occupied.insert((y, x));
                    }
                }
                cells.push(Cell {
                    path,
                    row: row_index,
                    column,
                    rows: row_span,
                    columns: column_span,
                });
                column += column_span;
                columns = columns.max(column);
            }
        }
        let slots = rows.len() * columns;
        if slots > MAX_SLOTS {
            return Err(error(&[], LayoutErrorKind::TableBudgetExceeded));
        }
        Ok(Grid {
            rows,
            cells,
            columns,
            slots,
        })
    }

    pub(super) fn preflight(root: &Container) -> Result<(), LayoutError> {
        let mut stack = vec![(root, Vec::new())];
        let mut slots = 0usize;
        while let Some((node, path)) = stack.pop() {
            if let Element::TH { rows, columns } | Element::TD { rows, columns } = &node.element {
                span(rows.as_ref(), TableSpanAxis::Row, &path)?;
                span(columns.as_ref(), TableSpanAxis::Column, &path)?;
            }
            if node.element == Element::Table {
                let grid = grid(node).map_err(|mut diagnostic| {
                    let mut full_path = path.clone();
                    full_path.extend(diagnostic.path);
                    diagnostic.path = full_path;
                    diagnostic
                })?;
                slots += grid.slots;
                if slots > MAX_TOTAL_SLOTS {
                    return Err(error(&path, LayoutErrorKind::TableBudgetExceeded));
                }
            }
            for (index, child) in node.children.iter().enumerate().rev() {
                // Validate structure even in hidden rows/sections.
                if node.element == Element::TR
                    && !matches!(child.element, Element::TH { .. } | Element::TD { .. })
                {
                    let mut path = path;
                    path.push(index);
                    return Err(error(&path, LayoutErrorKind::InvalidTableStructure));
                }
                let mut path = path.clone();
                path.push(index);
                stack.push((child, path));
            }
        }
        Ok(())
    }

    pub(super) fn reset_table_measurements(root: &mut Container) {
        let mut stack = vec![(root, false)];
        while let Some((node, inside)) = stack.pop() {
            let inside = inside || node.element == Element::Table;
            if inside {
                node.calculated_preferred_width = None;
                node.calculated_preferred_height = None;
                node.calculated_child_min_width = None;
                node.calculated_child_min_height = None;
                node.calculated_min_width = None;
                node.calculated_max_width = None;
                node.calculated_min_height = None;
                node.calculated_max_height = None;
                node.calculated_margin_left = None;
                node.calculated_margin_right = None;
                node.calculated_margin_top = None;
                node.calculated_margin_bottom = None;
                node.calculated_padding_left = None;
                node.calculated_padding_right = None;
                node.calculated_padding_top = None;
                node.calculated_padding_bottom = None;
                node.calculated_font_size = None;
            }
            for child in &mut node.children {
                stack.push((child, inside));
            }
        }
    }

    pub(super) fn geometry(root: &Container) -> Vec<[Option<f32>; 4]> {
        let mut result = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            result.push([
                node.calculated_width,
                node.calculated_height,
                node.calculated_x,
                node.calculated_y,
            ]);
            stack.extend(node.children.iter());
        }
        result
    }

    pub(super) fn validate_geometry(root: &Container) -> Result<(), LayoutError> {
        let mut stack = vec![(root, Vec::new())];
        while let Some((node, path)) = stack.pop() {
            if [node.calculated_width, node.calculated_height]
                .into_iter()
                .flatten()
                .any(|value| value < 0.0)
            {
                return Err(error(&path, LayoutErrorKind::InvalidGeometry));
            }
            if [
                node.calculated_width,
                node.calculated_height,
                node.calculated_x,
                node.calculated_y,
            ]
            .into_iter()
            .flatten()
            .any(|value| !value.is_finite())
            {
                return Err(error(&path, LayoutErrorKind::InvalidGeometry));
            }
            for (index, child) in node.children.iter().enumerate().rev() {
                let mut path = path.clone();
                path.push(index);
                stack.push((child, path));
            }
        }
        Ok(())
    }

    // Non-flex column flow keeps tables at measured row height and gives the
    // remaining allocation to ordinary unsized siblings. Do not stretch table
    // rows, discard measured sibling content, or change authored dimensions.
    pub(super) fn allocate_sibling_heights(parent: &mut Container) {
        use hyperchad_transformer_models::LayoutDirection;
        if parent.direction != LayoutDirection::Column
            || parent.is_flex_container()
            || !parent
                .relative_positioned_elements()
                .any(|child| child.element == Element::Table)
        {
            return;
        }
        let eligible = |child: &Container| {
            child.element != Element::Table && child.height.is_none() && !child.is_span()
        };
        let count = parent
            .relative_positioned_elements()
            .filter(|child| eligible(child))
            .count();
        if count == 0 {
            return;
        }
        let used: f32 = parent
            .relative_positioned_elements()
            .map(|child| child.bounding_calculated_height().unwrap_or_default())
            .sum();
        let children = parent.relative_positioned_elements().count();
        #[allow(clippy::cast_precision_loss)]
        let gaps =
            parent.calculated_row_gap.unwrap_or_default() * children.saturating_sub(1) as f32;
        let surplus = (parent.calculated_height.unwrap_or_default() - used - gaps).max(0.0);
        if surplus == 0.0 {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let increment = surplus / count as f32;
        for child in parent
            .relative_positioned_elements_mut()
            .filter(|child| eligible(child))
        {
            child.calculated_height = Some(child.calculated_height.unwrap_or_default() + increment);
        }
    }

    pub(super) const fn structural(node: &Container) -> bool {
        matches!(
            node.element,
            Element::Table | Element::THead | Element::TBody | Element::TR
        )
    }

    fn horizontal_box(node: &Container) -> f32 {
        node.padding_x().unwrap_or_default()
            + node.margin_x().unwrap_or_default()
            + node.scrollbar_right.unwrap_or_default()
    }

    fn vertical_box(node: &Container) -> f32 {
        node.padding_y().unwrap_or_default()
            + node.margin_y().unwrap_or_default()
            + node.scrollbar_bottom.unwrap_or_default()
    }

    fn track_count(count: usize) -> f32 {
        f32::from(u16::try_from(count).expect("bounded table track count"))
    }

    // Equal increments satisfy spanning constraints without depending on cell IDs.
    fn satisfy(tracks: &mut [f32], start: usize, count: usize, minimum: f32) {
        let tracks = &mut tracks[start..start + count];
        let deficit = minimum - tracks.iter().sum::<f32>();
        if deficit > 0.0 {
            let increment = deficit / track_count(count);
            for track in tracks {
                *track += increment;
            }
        }
    }

    fn minimum_width<F: FontMetrics>(
        node: &Container,
        measurements: &super::TextMeasurements<'_, F>,
        vw: f32,
        vh: f32,
    ) -> Result<f32, LayoutError> {
        if node.is_hidden() {
            return Ok(0.0);
        }
        let content = match &node.element {
            Element::Raw { value } | Element::Text { value } => {
                let mut width = 0.0_f32;
                let style = &measurements.styles[&std::ptr::from_ref(node).addr()];
                if style.whitespace == Some(hyperchad_transformer_models::WhiteSpace::Preserve) {
                    width = measurements.measure(node, value, f32::INFINITY)?.width();
                } else {
                    for word in value.split_whitespace() {
                        width = width.max(measurements.measure(node, word, f32::INFINITY)?.width());
                    }
                }
                width
            }
            Element::Table => node.calculated_child_min_width.unwrap_or_default(),
            _ => {
                let mut width = 0.0_f32;
                for child in node.relative_positioned_elements() {
                    let child_width = minimum_width(child, measurements, vw, vh)?;
                    if node.element == Element::TR
                        || node.direction == hyperchad_transformer_models::LayoutDirection::Row
                    {
                        width += child_width;
                    } else {
                        width = width.max(child_width);
                    }
                }
                width
            }
        };
        let authored = node
            .width
            .as_ref()
            .and_then(Number::as_fixed)
            .map_or(0.0, |n| n.calc(0.0, vw, vh));
        Ok(content
            .max(authored)
            .max(node.calculated_min_width.unwrap_or_default())
            + horizontal_box(node))
    }

    // Generic preferred dimensions already include this container's box
    // additions; authored dimensions describe its content box.
    fn columns<F: FontMetrics>(
        table: &Container,
        grid: &Grid,
        measurements: &super::TextMeasurements<'_, F>,
        vw: f32,
        vh: f32,
    ) -> Result<(Vec<f32>, Vec<f32>), LayoutError> {
        let mut minima = vec![0.0; grid.columns];
        let mut preferred = vec![0.0; grid.columns];
        // Non-spanning constraints first makes the result independent of whether
        // a spanning heading precedes or follows an ordinary body row.
        for spanning in [false, true] {
            for cell in &grid.cells {
                if (cell.columns > 1) != spanning {
                    continue;
                }
                let node = at(table, &cell.path);
                let authored = node.width.as_ref().map_or(0.0, |n| {
                    n.calc(table.calculated_width.unwrap_or_default(), vw, vh)
                });
                let min_constraint = node.min_width.as_ref().map_or(0.0, |n| {
                    n.calc(table.calculated_width.unwrap_or_default(), vw, vh)
                });
                let min = minimum_width(node, measurements, vw, vh)?
                    .max(authored.max(min_constraint) + horizontal_box(node));
                let desired = node
                    .calculated_preferred_width
                    .unwrap_or_default()
                    .max(authored + horizontal_box(node));
                satisfy(&mut minima, cell.column, cell.columns, min);
                satisfy(&mut preferred, cell.column, cell.columns, desired.max(min));
            }
        }
        for (preferred, minimum) in preferred.iter_mut().zip(&minima) {
            *preferred = preferred.max(*minimum);
        }
        Ok((minima, preferred))
    }

    pub(super) fn intrinsic<F: FontMetrics>(
        table: &mut Container,
        measurements: &super::TextMeasurements<'_, F>,
        vw: f32,
        vh: f32,
    ) -> Result<(), LayoutError> {
        if table.element != Element::Table {
            return Ok(());
        }
        let grid = grid(table).expect("table preflight");
        let (minima, preferred) = columns(table, &grid, measurements, vw, vh)?;
        table.calculated_child_min_width = Some(minima.iter().sum());
        table.calculated_preferred_width = Some(preferred.iter().sum());
        Ok(())
    }

    pub(super) fn distribute(tracks: &mut [f32], minima: &[f32], available: f32) {
        if tracks.is_empty() {
            return;
        }
        let total = tracks.iter().sum::<f32>();
        if available >= total {
            let increment = (available - total) / track_count(tracks.len());
            for track in tracks {
                *track += increment;
            }
        } else {
            // Equal deficit with saturated minima. When minima exceed the box,
            // retain the larger track sum: the authored overflow mode handles it.
            let mut deficit = total - available;
            for _ in 0..tracks.len() {
                let count = tracks
                    .iter()
                    .zip(minima)
                    .filter(|(track, min)| **track > **min)
                    .count();
                if count == 0 || deficit <= 0.0 {
                    break;
                }
                let share = deficit / track_count(count);
                for (track, min) in tracks.iter_mut().zip(minima) {
                    let reduction = (*track - *min).max(0.0).min(share);
                    *track -= reduction;
                    deficit -= reduction;
                }
            }
        }
    }

    pub(super) fn widths<F: FontMetrics>(
        table: &mut Container,
        measurements: &super::TextMeasurements<'_, F>,
        vw: f32,
        vh: f32,
    ) -> Result<(), LayoutError> {
        let grid = grid(table).expect("table preflight");
        let (minima, mut tracks) = columns(table, &grid, measurements, vw, vh)?;
        distribute(
            &mut tracks,
            &minima,
            table.calculated_width.unwrap_or_default(),
        );
        let width = tracks.iter().sum::<f32>();
        for child in &mut table.children {
            if matches!(child.element, Element::THead | Element::TBody) {
                child.calculated_width = Some((width - horizontal_box(child)).max(0.0));
            }
        }
        for path in &grid.rows {
            let row = at_mut(table, path);
            row.calculated_width = Some((width - horizontal_box(row)).max(0.0));
        }
        for cell in &grid.cells {
            let outer = tracks[cell.column..cell.column + cell.columns]
                .iter()
                .sum::<f32>();
            let node = at_mut(table, &cell.path);
            node.calculated_width = Some((outer - horizontal_box(node)).max(0.0));
            node.calculated_x = Some(tracks[..cell.column].iter().sum());
        }
        Ok(())
    }

    pub(super) fn heights(table: &mut Container, vw: f32, vh: f32) {
        let grid = grid(table).expect("table preflight");
        let mut tracks = vec![0.0; grid.rows.len()];
        for (index, path) in grid.rows.iter().enumerate() {
            let row = at(table, path);
            tracks[index] = row
                .height
                .as_ref()
                .map_or(0.0, |n| {
                    n.calc(table.calculated_height.unwrap_or_default(), vw, vh)
                })
                .max(row.calculated_min_height.unwrap_or_default())
                + vertical_box(row);
        }
        for spanning in [false, true] {
            for cell in &grid.cells {
                if (cell.rows > 1) != spanning {
                    continue;
                }
                let node = at(table, &cell.path);
                let authored = node.height.as_ref().map_or(0.0, |n| {
                    n.calc(table.calculated_height.unwrap_or_default(), vw, vh)
                });
                let height = node
                    .calculated_preferred_height
                    .unwrap_or_default()
                    .max(authored + vertical_box(node))
                    .max(node.calculated_min_height.unwrap_or_default() + vertical_box(node));
                satisfy(&mut tracks, cell.row, cell.rows, height);
            }
        }
        let mut section_offsets = vec![0.0; table.children.len()];
        for (index, path) in grid.rows.iter().enumerate() {
            let row = at_mut(table, path);
            row.calculated_height = Some((tracks[index] - vertical_box(row)).max(0.0));
            row.calculated_preferred_height = row.calculated_height;
            row.calculated_child_min_height = row.calculated_height;
            row.calculated_x = Some(0.0);
            row.calculated_y = Some(if path.len() == 1 {
                tracks[..index].iter().sum()
            } else {
                section_offsets[path[0]]
            });
            section_offsets[path[0]] += tracks[index];
        }
        let mut offset = 0.0;
        for (index, child) in table.children.iter_mut().enumerate() {
            if matches!(child.element, Element::THead | Element::TBody) {
                child.calculated_x = Some(0.0);
                child.calculated_y = Some(offset);
                child.calculated_height =
                    Some((section_offsets[index] - vertical_box(child)).max(0.0));
                child.calculated_preferred_height = child.calculated_height;
                child.calculated_child_min_height = child.calculated_height;
            }
            offset += section_offsets[index];
        }
        for cell in &grid.cells {
            let height = tracks[cell.row..cell.row + cell.rows].iter().sum::<f32>();
            let node = at_mut(table, &cell.path);
            node.calculated_height = Some((height - vertical_box(node)).max(0.0));
            node.calculated_y = Some(0.0);
        }
        table.calculated_preferred_height = Some(tracks.iter().sum());
        table.calculated_child_min_height = table.calculated_preferred_height;
    }
}

/// Default styling values for text elements.
#[derive(Debug, Clone, Copy)]
pub struct CalculatorDefaults {
    /// Default font size.
    pub font_size: f32,
    /// Default top margin for text.
    pub font_margin_top: f32,
    /// Default bottom margin for text.
    pub font_margin_bottom: f32,
    /// Font size for H1 heading elements.
    pub h1_font_size: f32,
    /// Top margin for H1 heading elements.
    pub h1_font_margin_top: f32,
    /// Bottom margin for H1 heading elements.
    pub h1_font_margin_bottom: f32,
    /// Font size for H2 heading elements.
    pub h2_font_size: f32,
    /// Top margin for H2 heading elements.
    pub h2_font_margin_top: f32,
    /// Bottom margin for H2 heading elements.
    pub h2_font_margin_bottom: f32,
    /// Font size for H3 heading elements.
    pub h3_font_size: f32,
    /// Top margin for H3 heading elements.
    pub h3_font_margin_top: f32,
    /// Bottom margin for H3 heading elements.
    pub h3_font_margin_bottom: f32,
    /// Font size for H4 heading elements.
    pub h4_font_size: f32,
    /// Top margin for H4 heading elements.
    pub h4_font_margin_top: f32,
    /// Bottom margin for H4 heading elements.
    pub h4_font_margin_bottom: f32,
    /// Font size for H5 heading elements.
    pub h5_font_size: f32,
    /// Top margin for H5 heading elements.
    pub h5_font_margin_top: f32,
    /// Bottom margin for H5 heading elements.
    pub h5_font_margin_bottom: f32,
    /// Font size for H6 heading elements.
    pub h6_font_size: f32,
    /// Top margin for H6 heading elements.
    pub h6_font_margin_top: f32,
    /// Bottom margin for H6 heading elements.
    pub h6_font_margin_bottom: f32,
}

/// Layout calculator that computes container dimensions and positions.
pub struct Calculator<F: FontMetrics> {
    font_metrics: F,
    defaults: CalculatorDefaults,
}

impl<F: FontMetrics> Calculator<F> {
    /// Creates a new calculator with the given font metrics and defaults.
    #[must_use]
    pub const fn new(font_metrics: F, defaults: CalculatorDefaults) -> Self {
        Self {
            font_metrics,
            defaults,
        }
    }
}

#[cfg(feature = "benchmark")]
macro_rules! time {
    ($label:tt, $expr:expr $(,)?) => {{
        let before = switchy_time::now();
        let ret = $expr;
        let duration = switchy_time::now().duration_since(before).unwrap();
        log::info!("{}: took {}µs", $label, duration.as_micros());
        ret
    }};
}

#[cfg(not(feature = "benchmark"))]
macro_rules! time {
    ($label:tt, $expr:expr $(,)?) => {
        $expr
    };
}

macro_rules! update_changed {
    ($changed:ident, $($reason:tt)+) => {{
        if std::option_env!("LOG_ON_CHANGED") == Some("1") {
            let reason = format!($($reason)*);
            log::info!("change triggered because {reason}");
        }
        $changed = true;
    }};
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl<F: FontMetrics> Calc for Calculator<F> {
    #[allow(clippy::let_and_return, clippy::cognitive_complexity)]
    fn calc(&self, container: &mut Container) -> bool {
        self.try_calc(container)
            .unwrap_or_else(|error| panic!("{error}"))
    }

    fn try_calc(&self, container: &mut Container) -> Result<bool, LayoutError> {
        table_layout::preflight(container)?;
        if container.calculated_width.is_none() || container.calculated_height.is_none() {
            return Err(LayoutError {
                path: Vec::new(),
                kind: LayoutErrorKind::InvalidGeometry,
            });
        }
        table_layout::validate_geometry(container)?;
        let before = table_layout::geometry(container);
        let measurements = TextMeasurements::new(&self.font_metrics, container, self.defaults)?;
        table_layout::reset_table_measurements(container);
        reset_text_wrap_metadata(container);
        log::trace!("calc: container={container}");

        let changed = time!("calc", {
            let arena = time!("arena", Bump::new());
            let context = arena.alloc(Container::default());
            let bfs = time!("bfs", container.bfs());
            time!(
                "calc_widths",
                self.calc_widths_measured(&bfs, container, &measurements)
            );
            measurements.check()?;
            time!(
                "calc_margin_and_padding",
                self.calc_margin_and_padding(&bfs, container)
            );
            time!(
                "flex_width",
                Self::flex_width_measured(&bfs, container, &measurements)
            );
            measurements.check()?;
            time!(
                "wrap_horizontal",
                Self::wrap_horizontal_measured(&bfs, container, &measurements)
            );
            measurements.check()?;
            time!(
                "calc_heights",
                self.calc_heights_measured(&bfs, container, &measurements)
            );
            measurements.check()?;
            time!(
                "flex_height",
                Self::flex_height_measured(&bfs, container, &measurements)
            );
            measurements.check()?;
            time!(
                "position_elements",
                self.position_elements(&arena, &bfs, container, context)
            )
        });
        table_layout::validate_geometry(container)?;
        Ok(changed || before != table_layout::geometry(container))
    }
}

macro_rules! calc_size_on_axis {
    (
        $measurements:ident,
        $label:tt,
        $self:ident,
        $bfs:ident,
        $container:ident,
        $size:ident,
        $axis:ident,
        $cross_axis:ident,
        $x:ident,
        $y:ident,
        $unit:ident,
        $each_parent:expr,
        $each_child:expr
        $(,)?
    ) => {{
        use paste::paste;

        use crate::{LayoutDirection, LayoutOverflow, Position, float_eq};

        const LABEL: &str = $label;

        log::trace!("{LABEL}:\n{}", $container);

        // Traversal does not move containers; compare addresses, never authored
        // IDs (which may be zero or duplicated). No raw pointer is dereferenced.
        let root_ptr = std::ptr::from_ref::<Container>($container);
        let defaults = $self.defaults;
        let view_width = $container.calculated_width.expect("Missing view_width");
        let view_height = $container.calculated_height.expect("Missing view_height");

        $bfs.traverse_rev_with_parents_ref_mut(
            true,
            Container::default(),
            $container,
            |parent, mut context| {
                if std::ptr::eq(std::ptr::from_ref(parent), root_ptr) {
                    context.calculated_font_size = Some(defaults.font_size);
                    context.calculated_margin_top = Some(defaults.font_margin_top);
                    context.calculated_margin_bottom = Some(defaults.font_margin_bottom);
                }

                $each_parent(&mut *parent, view_width, view_height, &context, defaults);

                macro_rules! set_prop_to_context {
                    ($prop:ident) => {
                        if let Some(value) = paste!(parent.[<calculated_ $prop>]) {
                            paste!(context.[<calculated_ $prop>]) = Some(value);
                        }
                    };
                }

                set_prop_to_context!(font_size);
                set_prop_to_context!(margin_top);
                set_prop_to_context!(margin_bottom);

                context
            },
            |parent, context| {
                if $measurements.failed() { return; }
                let mut min_size = 0.0;
                let mut preferred_size = 0.0;

                if let Some(gap) = paste!(parent.[<$cross_axis:lower _gap>]).as_ref().and_then(crate::Number::as_fixed) {
                    let gap = gap.calc(0.0, view_width, view_height);
                    log::trace!("{LABEL}: setting gap={gap}");
                    paste!(parent.[<calculated_ $cross_axis:lower _gap>]) = Some(gap);
                }

                let direction = parent.direction;
                let overflow = paste!(parent.[<overflow_ $unit>]);

                for child in &mut parent.children {
                    log::trace!("{LABEL}: container:\n{child}");

                    $each_child(&mut *child, view_width, view_height, context, defaults);

                    if let Some(max) = paste!(child.[<max_ $size>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = max.calc(0.0, view_width, view_height);
                        log::trace!("{LABEL}: calculated_max_size={size}");
                        paste!(child.[<calculated_max_ $size>]) = Some(size);
                    }
                    if let Some(min) = paste!(child.[<min_ $size>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = min.calc(0.0, view_width, view_height);
                        log::trace!("{LABEL}: calculated_min_size={size}");
                        paste!(child.[<calculated_min_ $size>]) = Some(size);
                    }

                    let intrinsic_control = match $measurements.control_size(child) {
                        Ok(size) => size,
                        Err(error) => { $measurements.record(error); return; }
                    };
                    let (mut min, mut preferred) = if let Some(size) = child.$size.as_ref().and_then(Number::as_fixed) {
                        let new_size = size.calc(0.0, view_width, view_height);

                        paste!(child.[<calculated_ $size>]) = Some(new_size);
                        (Some(new_size), new_size)
                    } else if let (LayoutDirection::Row, crate::Element::Raw { value } | crate::Element::Text { value }) = (LayoutDirection::$axis, &child.element) {
                        log::trace!("{LABEL}: measuring text={value}");
                        let bounds = match $measurements.measure(child, value, f32::INFINITY) {
                            Ok(bounds) => bounds,
                            Err(error) => { $measurements.record(error); return; }
                        };
                        log::trace!("{LABEL}: measured bounds={bounds:?}");
                        let width = bounds.width();
                        let height = bounds.height();

                        child.calculated_width = Some(width);
                        child.calculated_height = Some(height);
                        child.calculated_preferred_height = Some(height);
                        child.calculated_text_wrap_width = Some(f32::INFINITY);

                        (None, width)
                    } else if child.$size.is_none() && intrinsic_control.is_some() {
                        let (width, height) = intrinsic_control.expect("checked control intrinsic size");
                        let size = if stringify!($size) == "width" { width } else { height };
                        paste!(child.[<calculated_ $size>]) = Some(size);
                        // Editor columns are a preferred width, not a hard
                        // minimum that would force narrow ancestors to overflow.
                        (Some(if stringify!($size) == "width" { 0.0 } else { size }), size)
                    } else if let Some(size) = paste!(child.[<calculated_preferred_ $size>]) {
                        paste!(child.[<calculated_ $size>]) = Some(size);
                        (paste!(child.[<calculated_child_min_ $size>]), size)
                    } else if let Some(size) = paste!(child.[<calculated_child_min_ $size>]) {
                        paste!(child.[<calculated_ $size>]) = Some(size);
                        (Some(size), size)
                    } else {
                        paste!(child.[<calculated_ $size>]) = Some(0.0);
                        (None, 0.0)
                    };

                    if let Some(calculated_max) = paste!(child.[<calculated_max_ $size>]) {
                        if let Some(existing) = &mut paste!(child.[<calculated_ $size>]) {
                            if *existing > calculated_max {
                                *existing = calculated_max;
                                preferred = calculated_max;
                            }
                        } else {
                            paste!(child.[<calculated_ $size>]) = Some(calculated_max);
                            preferred = calculated_max;
                        }
                    }
                    if let Some(calculated_min) = paste!(child.[<calculated_min_ $size>]) {
                        if let Some(existing) = &mut paste!(child.[<calculated_ $size>]) {
                            if *existing < calculated_min {
                                *existing = calculated_min;
                                preferred = calculated_min;
                            }
                        } else {
                            paste!(child.[<calculated_ $size>]) = Some(calculated_min);
                            preferred = calculated_min;
                        }
                    }

                    if let Some(margin) = paste!(child.[<margin_ $x>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = margin.calc(0.0, view_width, view_height);
                        paste!(child.[<calculated_margin_ $x>]) = Some(size);
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some(margin) = paste!(child.[<margin_ $y>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = margin.calc(0.0, view_width, view_height);
                        paste!(child.[<calculated_margin_ $y>]) = Some(size);
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some(padding) = paste!(child.[<padding_ $x>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = padding.calc(0.0, view_width, view_height);
                        paste!(child.[<calculated_padding_ $x>]) = Some(size);
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some(padding) = paste!(child.[<padding_ $y>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = padding.calc(0.0, view_width, view_height);
                        paste!(child.[<calculated_padding_ $y>]) = Some(size);
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some((&color, size)) = paste!(child.[<border_ $x>])
                        .as_ref()
                        .and_then(|(color, size)| size.as_fixed().map(|size| (color, size)))
                    {
                        let size = size.calc(0.0, view_width, view_height);
                        if let Some(calculated) = &mut paste!(child.[<calculated_border_ $x>]) {
                            if calculated.0 != color {
                                calculated.0 = color;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $x>]) = Some((color, size));
                        }
                    }
                    if let Some((&color, size)) = paste!(child.[<border_ $y>])
                        .as_ref()
                        .and_then(|(color, size)| size.as_fixed().map(|size| (color, size)))
                    {
                        let size = size.calc(0.0, view_width, view_height);
                        if let Some(calculated) = &mut paste!(child.[<calculated_border_ $y>]) {
                            if calculated.0 != color {
                                calculated.0 = color;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $y>]) = Some((color, size));
                        }
                    }

                    macro_rules! handle_auto_sizing {
                        ($value:ident, $output:ident) => {{
                            match child.position.unwrap_or_default() {
                                Position::Static | Position::Relative | Position::Sticky => match overflow {
                                    LayoutOverflow::Auto
                                    | LayoutOverflow::Scroll
                                    | LayoutOverflow::Expand
                                    | LayoutOverflow::Squash => {
                                        match direction {
                                            LayoutDirection::$axis => $output += $value,
                                            LayoutDirection::$cross_axis => if $value > $output {
                                                $output = $value;
                                            },
                                        }
                                    }
                                    LayoutOverflow::Wrap { .. } => {
                                        if $value > $output {
                                            $output = $value;
                                        }
                                    }
                                    LayoutOverflow::Hidden => {}
                                },
                                Position::Absolute | Position::Fixed => {}
                            }
                        }};
                    }

                    paste!(child.[<calculated_preferred_ $size>]) = Some(preferred);
                    handle_auto_sizing!(preferred, preferred_size);

                    if let Some(size) = min {
                        paste!(child.[<calculated_child_min_ $size>]) = Some(size);
                        handle_auto_sizing!(size, min_size);
                    }
                }

                paste!(parent.[<calculated_child_min_ $size>]) = Some(min_size);
                paste!(parent.[<calculated_preferred_ $size>]) = Some(preferred_size);
                if parent.element == crate::Element::Table {
                    if stringify!($size) == "width" {
                        if let Err(error) = $crate::layout::calc::table_layout::intrinsic(parent, $measurements, view_width, view_height) { $measurements.record(error); }
                    } else {
                        $crate::layout::calc::table_layout::heights(parent, view_width, view_height);
                    }
                }
            });
    }};
}

macro_rules! flex_on_axis {
    (
        $measurements:ident,
        $label:tt,
        $bfs:ident,
        $container:ident,
        $size:ident,
        $axis:ident,
        $cross_axis:ident,
        $cell:ident,
        $x:ident,
        $y:ident,
        $unit:ident
        $(,)?
    ) => {{
        use paste::paste;

        use crate::{Element, LayoutOverflow, float_eq, float_gt};

        const LABEL: &str = $label;

        log::trace!("{LABEL}:\n{}", $container);

        let root_ptr = std::ptr::from_ref::<Container>($container);
        let view_width = $container.calculated_width.expect("Missing view_width");
        let view_height = $container.calculated_height.expect("Missing view_height");
        let mut rect = crate::layout::Rect::default();

        #[allow(clippy::cognitive_complexity)]
        $bfs.traverse_with_parents_ref_mut(
            true,
            &mut rect,
            $container,
            |parent, relative_container| {
                if std::ptr::eq(std::ptr::from_ref(parent), root_ptr) {
                    relative_container.x = 0.0;
                    relative_container.y = 0.0;
                    relative_container.width = view_width;
                    relative_container.height = view_height;
                } else if parent.position == Some(Position::Relative) {
                    relative_container.x = 0.0;
                    relative_container.y = 0.0;
                    relative_container.width = view_width;
                    relative_container.height = view_height;
                    paste!(relative_container.[<$size>] = parent.[<calculated_ $size>].expect("Missing parent calculated size"));
                }
            },
            |parent, relative_container| {
                if $measurements.failed() { return; }
                if $crate::layout::calc::table_layout::structural(parent) {
                    if parent.element == crate::Element::Table && stringify!($size) == "width" {
                        if let Err(error) = $crate::layout::calc::table_layout::widths(parent, $measurements, view_width, view_height) { $measurements.record(error); }
                    }
                    return;
                }
                if stringify!($size) == "height" {
                    $crate::layout::calc::table_layout::allocate_sibling_heights(parent);
                }
                let direction = parent.direction;
                let container_size = paste!(parent.[<calculated_ $size>]).expect("Missing container size");

                if let Some(gap) = paste!(parent.[<$cross_axis:lower _gap>]).as_ref().and_then(crate::Number::as_dynamic) {
                    paste!(parent.[<calculated_ $cross_axis:lower _gap>]) =
                        Some(gap.calc(container_size, view_width, view_height));
                }

                for child in &mut parent.children {
                    log::trace!("{LABEL}: processing child\n{child}");
                    let calculated_size = paste!(child.[<calculated_ $size>]).as_mut().expect("Missing calculated size");
                    if let Some(min) = paste!(child.[<min_ $size>]).as_ref().and_then(crate::Number::as_dynamic) {
                        let size = min.calc(container_size, view_width, view_height);
                        log::trace!("{LABEL}: calculated_min_size={size}");
                        paste!(child.[<calculated_min_ $size>]) = Some(size);
                    }
                    if let Some(max) = paste!(child.[<max_ $size>]).as_ref().and_then(crate::Number::as_dynamic) {
                        let size = max.calc(container_size, view_width, view_height);
                        log::trace!("{LABEL}: calculated_max_size={size}");
                        paste!(child.[<calculated_max_ $size>]) = Some(size);

                        if size < *calculated_size {
                            *calculated_size = size;
                        }
                    }
                    if let Some(min) = paste!(child.[<calculated_min_ $size>]) {
                        if min > *calculated_size {
                            log::trace!("{LABEL}: setting size from={calculated_size} to min_size={min}");
                            *calculated_size = min;
                        }
                    }

                    if matches!(paste!(child.[<overflow_ $unit>]), LayoutOverflow::Auto | LayoutOverflow::Scroll) {
                        if let Some(min) = &mut paste!(child.[<calculated_min_ $size>]).or(paste!(child.[<calculated_child_min_ $size>])) {
                            log::trace!("{LABEL}: checking if min={min} > container_size={container_size}");
                            if *min > container_size {
                                *min = container_size;
                                paste!(child.[<calculated_ $size>]) = Some(container_size);
                            }
                        }
                    }

                    if let Some((&color, size)) = paste!(child.[<border_ $x>])
                        .as_ref()
                        .and_then(|(color, size)| size.as_dynamic().map(|size| (color, size)))
                    {
                        let size = size.calc(0.0, view_width, view_height);
                        if let Some(calculated) = &mut paste!(child.[<calculated_border_ $x>]) {
                            if calculated.0 != color {
                                calculated.0 = color;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $x>]) = Some((color, size));
                        }
                    }
                    if let Some((&color, size)) = paste!(child.[<border_ $y>])
                        .as_ref()
                        .and_then(|(color, size)| size.as_dynamic().map(|size| (color, size)))
                    {
                        let size = size.calc(0.0, view_width, view_height);
                        if let Some(calculated) = &mut paste!(child.[<calculated_border_ $y>]) {
                            if calculated.0 != color {
                                calculated.0 = color;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $y>]) = Some((color, size));
                        }
                    }
                }

                if parent.relative_positioned_elements().any(|x| x.$size.as_ref().is_none_or(crate::Number::is_dynamic)) {
                    let mut remaining_container_size = container_size;

                    // Remove margins & padding from remaining_container_size
                    for child in parent.relative_positioned_elements() {
                        match direction {
                            LayoutDirection::$axis => {
                                if let Some(size) = paste!(child.[<margin_ $unit>]()) {
                                    log::trace!(
                                        "{LABEL}: removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                                        remaining_container_size - size
                                    );
                                    remaining_container_size -= size;
                                }
                                if let Some(size) = paste!(child.[<padding_ $unit>]()) {
                                    log::trace!(
                                        "{LABEL}: removing padding size={size} from remaining_container_size={remaining_container_size} ({})",
                                        remaining_container_size - size
                                    );
                                    remaining_container_size -= size;
                                }
                            }
                            LayoutDirection::$cross_axis => {}
                        }
                    }

                    log::trace!("{LABEL}: container_size={container_size} remaining_container_size={remaining_container_size}");
                    let container_size = remaining_container_size;

                    // Calculate relative positioned children dynamic sizes
                    for child in parent.relative_positioned_elements_mut() {
                        if let Some(size) = child.$size.as_ref().and_then(crate::Number::as_dynamic) {
                            let container_size = match direction {
                                LayoutDirection::$axis => container_size,
                                LayoutDirection::$cross_axis => {
                                    container_size
                                        - paste!(child.[<margin_ $unit>]()).unwrap_or_default()
                                        - paste!(child.[<padding_ $unit>]()).unwrap_or_default()
                                }
                            };
                            log::trace!("{LABEL}: calculating dynamic size={size:?}");
                            let size = size.calc(container_size, view_width, view_height);
                            log::trace!("{LABEL}: calculated dynamic size={size}");
                            paste!(child.[<calculated_ $size>]) = Some(size);
                        }
                    }

                    // Fit all unsized children
                    if parent.relative_positioned_elements().any(|x| x.$size.is_none()) {
                        let mut remaining_size = container_size;
                        let mut last_cell = 0;
                        let mut max_cell_size = 0.0;

                        // Remove sized children sizes from remaining_size
                        for child in parent.relative_positioned_elements() {
                            log::trace!("{LABEL}: calculating remaining size:\n{child}");

                            match direction {
                                LayoutDirection::$axis => {
                                    if let Some(size) = paste!(child.[<calculated_ $size>]) {
                                        log::trace!(
                                            "{LABEL}: removing size={size} from remaining_size={remaining_size} ({})",
                                            remaining_size - size
                                        );
                                        remaining_size -= size;
                                    }
                                }
                                LayoutDirection::$cross_axis => {
                                    if let Some(LayoutPosition::Wrap { $cell: cell, .. }) = child.calculated_position {
                                        if cell != last_cell {
                                            moosicbox_assert::assert!(cell > last_cell);
                                            remaining_size -= max_cell_size;
                                            max_cell_size = paste!(child.[<calculated_ $size>]).unwrap_or_default();
                                        }
                                        last_cell = cell;
                                    }
                                }
                            }
                        }

                        let cell_count = last_cell + 1;
                        remaining_size -= max_cell_size;

                        if remaining_size < 0.0 {
                            remaining_size = 0.0;
                        }

                        log::trace!("{LABEL}: remaining_size={remaining_size}\n{parent}");

                        // Fit all unsized children to remaining_size
                        match direction {
                            LayoutDirection::$axis => {
                                if parent.is_flex_container() {
                                    if float_gt!(remaining_size, 0.0)
                                        && parent
                                            .relative_positioned_elements()
                                            .any(|x| x.$size.is_none() && x.is_expandable(parent))
                                    {
                                        loop {
                                            let mut smallest = f32::INFINITY;
                                            let mut target = f32::INFINITY;
                                            let mut smallest_count = 0_u16;

                                            for size in parent
                                                .relative_positioned_elements()
                                                .filter(|x| x.$size.is_none() && x.is_expandable(parent))
                                                .filter_map(|x| paste!(x.[<calculated_ $size>]))
                                            {
                                                if smallest > size {
                                                    target = smallest;
                                                    smallest = size;
                                                    smallest_count = 1;
                                                } else if float_eq!(smallest, size) {
                                                    smallest_count += 1;
                                                } else if size < target {
                                                    target = size;
                                                }
                                            }

                                            moosicbox_assert::assert!(smallest_count > 0, "expected at least one smallest item");
                                            moosicbox_assert::assert!(smallest.is_finite(), "expected smallest to be finite");

                                            let smallest_countf = f32::from(smallest_count);

                                            let last_iteration = if target.is_infinite() {
                                                log::trace!("{LABEL}: last iteration remaining_size={remaining_size}");
                                                target = smallest + if smallest_count == 1 {
                                                    remaining_size
                                                } else {
                                                    remaining_size / smallest_countf
                                                };
                                                remaining_size = 0.0;
                                                true
                                            } else if target > remaining_size {
                                                log::trace!("{LABEL}: target={target} > remaining_size={remaining_size}");
                                                target = if smallest_count == 1 {
                                                    remaining_size
                                                } else {
                                                    remaining_size / smallest_countf
                                                };
                                                remaining_size = 0.0;
                                                true
                                            } else {
                                                remaining_size = (target - smallest)
                                                    .mul_add(-smallest_countf, remaining_size);
                                                float_eq!(remaining_size, 0.0)
                                            };

                                            log::trace!("{LABEL}: target={target} smallest={smallest} smallest_count={smallest_count} remaining_size={remaining_size} container_size={container_size}");

                                            moosicbox_assert::assert!(target.is_finite(), "expected target to be finite");

                                            let mut dynamic_child_size = false;

                                            for child in parent
                                                .relative_positioned_elements_mut()
                                                .filter(|x| x.$size.is_none())
                                                .filter(|x| paste!(x.[<calculated_ $size>]).is_some_and(|x| float_eq!(x, smallest)))
                                            {
                                                let mut clipped = false;
                                                let mut target = target;

                                                if let Some(min) = paste!(child.[<calculated_min_ $size>]) {
                                                    log::trace!("{LABEL}: calculated_min={min}");
                                                    let min = min - paste!(child.[<padding_ $unit>]()).unwrap_or_default() - paste!(child.[<margin_ $unit>]()).unwrap_or_default();
                                                    log::trace!("{LABEL}: calculated_min={min} without padding/margins");
                                                    if target < min {
                                                        remaining_size -= min - target;
                                                        target = min;
                                                    }
                                                }
                                                if direction == LayoutDirection::Row && child.is_raw() {
                                                    if let Some(preferred) = paste!(child.[<calculated_preferred_ $size>]) {
                                                        log::trace!("{LABEL}: calculated_preferred={preferred}");
                                                        let preferred = preferred - paste!(child.[<padding_ $unit>]()).unwrap_or_default() - paste!(child.[<margin_ $unit>]()).unwrap_or_default();
                                                        log::trace!("{LABEL}: calculated_preferred={preferred} without padding/margins");
                                                        if target > preferred {
                                                            remaining_size += target - preferred;
                                                            target = preferred;
                                                            clipped = true;
                                                        }
                                                    }
                                                }

                                                if !clipped {
                                                    dynamic_child_size = true;
                                                }

                                                let prev = paste!(child.[<calculated_ $size>]).unwrap();
                                                paste!(child.[<calculated_ $size>]) = Some(target);
                                                log::trace!("{LABEL}: increasing child size prev={prev} to target={target}:\n{child}");
                                            }

                                            if last_iteration || !dynamic_child_size {
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            LayoutDirection::$cross_axis => {
                                log::trace!("{LABEL}: size_cross_axis\n{parent}");

                                let align_items = parent.align_items;

                                for child in parent.relative_positioned_elements_mut() {
                                    if matches!(child.element, Element::Raw { .. } | Element::Text { .. }) {
                                        continue;
                                    }

                                    if float_gt!(paste!(child.[<calculated_ $size>]).expect("Missing calculated_size"), container_size) {
                                        set_float(
                                            &mut paste!(child.[<calculated_ $size>]),
                                            paste!(child.[<calculated_min_ $size>]).unwrap_or(container_size)
                                        );
                                    }

                                    if align_items.is_none() {
                                        log::trace!("{LABEL}: setting size to remaining_size={remaining_size}:\n{child}");
                                        let mut remaining_container_size = remaining_size;

                                        if let Some(size) = paste!(child.[<margin_ $unit>]()) {
                                            log::trace!(
                                                "{LABEL}: removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                                                remaining_container_size - size
                                            );
                                            remaining_container_size -= size;
                                        }
                                        if let Some(size) = paste!(child.[<padding_ $unit>]()) {
                                            log::trace!(
                                                "{LABEL}: removing padding size={size} from remaining_container_size={remaining_container_size} ({})",
                                                remaining_container_size - size
                                            );
                                            remaining_container_size -= size;
                                        }

                                        #[allow(clippy::cast_precision_loss)]
                                        let mut new_size = remaining_container_size / (cell_count as f32);

                                        if let Some(max) = paste!(child.[<calculated_max_ $size>]) {
                                            if new_size > max {
                                                log::trace!("{LABEL}: setting size to calculated_max={max}");
                                                new_size = max;
                                            }
                                        }
                                        if let Some(min) = paste!(child.[<calculated_min_ $size>]) {
                                            if new_size < min {
                                                log::trace!("{LABEL}: setting size to calculated_min={min}");
                                                new_size = min;
                                            }
                                        } else if let Some(min) = paste!(child.[<calculated_child_min_ $size>]) {
                                            if new_size < min
                                                && !(child.element == crate::Element::Table && stringify!($size) == "width")
                                                && paste!(child.[<calculated_max_ $size>]).is_none_or(|x| min < x)
                                                && paste!(child.[<calculated_min_ $size>]).is_none_or(|x| min > x)
                                            {
                                                log::trace!("{LABEL}: setting size to calculated_child_min={min}");
                                                new_size = min;
                                            }
                                        }

                                        if new_size < 0.0 {
                                            log::trace!("{LABEL}: clamping size to 0.0");
                                            new_size = 0.0;
                                        }

                                        if child.$size.is_none() {
                                            paste!(child.[<calculated_ $size>]) = Some(new_size);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // absolute positioned

                let crate::layout::Rect { $size: relative_size, .. } = relative_container;

                for child in parent.absolute_positioned_elements_mut() {
                    let mut remaining_container_size = *relative_size;

                    if let Some(size) = paste!(child.[<margin_ $unit>]()) {
                        log::trace!(
                            "{LABEL}: absolute removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                            remaining_container_size - size
                        );
                        remaining_container_size -= size;
                    }
                    if let Some(size) = paste!(child.[<padding_ $unit>]()) {
                        log::trace!(
                            "{LABEL}: absolute removing padding size={size} from remaining_container_size={remaining_container_size} ({})",
                            remaining_container_size - size
                        );
                        remaining_container_size -= size;
                    }

                    if let Some(size) = &child.$size {
                        log::trace!("{LABEL}: calculating absolute child size={size:?}");
                        let size = size.calc(remaining_container_size, view_width, view_height);
                        log::trace!("{LABEL}: calculated absolute child size={size}");
                        paste!(child.[<calculated_ $size>]) = Some(size);
                    } else {
                        paste!(child.[<calculated_ $size>]) = Some(remaining_container_size);
                    }
                }

                for child in parent.fixed_positioned_elements_mut() {
                    let mut remaining_container_size = paste!([<view_ $size>]);

                    if let Some(size) = paste!(child.[<margin_ $unit>]()) {
                        log::trace!(
                            "{LABEL}: fixed removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                            remaining_container_size - size
                        );
                        remaining_container_size -= size;
                    }
                    if let Some(size) = paste!(child.[<padding_ $unit>]()) {
                        log::trace!(
                            "{LABEL}: fixed removing padding size={size} from remaining_container_size={remaining_container_size} ({})",
                            remaining_container_size - size
                        );
                        remaining_container_size -= size;
                    }

                    if let Some(size) = &child.$size {
                        log::trace!("{LABEL}: calculating fixed child size={size:?}");
                        let size = size.calc(remaining_container_size, view_width, view_height);
                        log::trace!("{LABEL}: calculated fixed child size={size}");
                        paste!(child.[<calculated_ $size>]) = Some(size);
                    }
                }
            });
    }};
}

macro_rules! wrap_on_axis {
    (
        $measurements:ident,
        $label:tt,
        $axis:ident,
        $bfs:ident,
        $container:ident,
        $size:ident,
        $overflow:ident,
        $gap:ident,
        $each_child:expr
        $(,)?
    ) => {{
        use paste::paste;

        use crate::models::{LayoutDirection, LayoutOverflow, LayoutPosition};

        paste! {
            const LABEL: &str = $label;

            log::trace!("{LABEL}:\n{}", $container);

            let view_width = $container.calculated_width.expect("Missing view_width");
            let view_height = $container.calculated_height.expect("Missing view_height");

            $bfs.traverse_mut($container, |parent| {
                if $measurements.failed() { return; }
                let container_width = parent.calculated_width.expect("Missing parent calculated_width");

                for child in &mut parent.children {
                    $each_child(child, container_width, view_width, view_height);
                    if $measurements.failed() { return; }
                }

                if !matches!(parent.$overflow, LayoutOverflow::Wrap { .. }) {
                    return;
                }

                let container_size = paste!(parent.[<calculated_ $size>]).expect("Missing parent container_size");

                let direction = parent.direction;
                let mut pos = 0.0;
                let mut row = 0;
                let mut col = 0;
                let gap = paste!(parent.[<calculated_ $gap>]);

                for child in parent.relative_positioned_elements_mut() {
                    let child_size = paste!(child.[<bounding_calculated_ $size>]()).expect("Missing child calculated bounding size");

                    let mut position = LayoutPosition::Wrap { row, col };

                    if direction == LayoutDirection::$axis {
                        pos += child_size;

                        if pos > container_size {
                            log::trace!("{LABEL}: wrapping to next row");
                            pos = child_size + gap.unwrap_or_default();
                            col = 0;
                            row += 1;
                            position = LayoutPosition::Wrap { row, col };
                        } else if let Some(gap) = gap {
                            pos += gap;
                        }

                        col += 1;
                    }

                    if let LayoutPosition::Wrap { row, col } = position {
                        log::trace!("{LABEL}: positioning child ({row}, {col}) pos={pos} container_size={container_size}:\n{child}");
                    }

                    set_value(&mut child.calculated_position, position);
                }
            });
        }
    }};
}

/// # Pass 1: Widths
///
/// This pass traverses the `Container` children in reverse BFS (Breadth-First Search)
/// and calculates the widths required for each of the `Container`s.
mod pass_widths {
    use paste::paste;

    use crate::{
        BfsPaths, Container, Element, HeaderSize, Number,
        layout::{
            calc::{Calculator, CalculatorDefaults},
            font::FontMetrics,
            set_float,
        },
    };

    #[cfg_attr(feature = "profiling", profiling::all_functions)]
    impl Container {
        fn calculate_font_size(
            &mut self,
            view_width: f32,
            view_height: f32,
            context: &Self,
            defaults: CalculatorDefaults,
        ) {
            if self.calculated_font_size.is_some() {
                return;
            }

            macro_rules! default_heading_prop {
                ($size:expr, $prop:ident $(,)?) => {
                    match $size {
                        HeaderSize::H1 => paste!(defaults.[<h1_ $prop>]),
                        HeaderSize::H2 => paste!(defaults.[<h2_ $prop>]),
                        HeaderSize::H3 => paste!(defaults.[<h3_ $prop>]),
                        HeaderSize::H4 => paste!(defaults.[<h4_ $prop>]),
                        HeaderSize::H5 => paste!(defaults.[<h5_ $prop>]),
                        HeaderSize::H6 => paste!(defaults.[<h6_ $prop>]),
                    }
                };
            }

            self.calculated_font_size = self.font_size.as_ref().map_or_else(
                || {
                    match self.element {
                        Element::Heading { size } => {
                            Some(default_heading_prop!(size, font_size))
                        }
                        _ => {
                            context.calculated_font_size
                        }
                    }
                },
                |font_size| {
                    let calculated_font_size = font_size.calc(
                        context
                            .calculated_font_size
                            .expect("Missing calculated_font_size"),
                        view_width,
                        view_height,
                    );
                    log::trace!("calculate_font_size: setting font_size={font_size} to calculated_font_size={calculated_font_size}");

                    Some(calculated_font_size)
                },
            );

            if self.margin_top.is_none()
                && let Element::Heading { size } = self.element
            {
                self.calculated_margin_top = Some(default_heading_prop!(size, font_margin_top));
            }

            if self.margin_bottom.is_none()
                && let Element::Heading { size } = self.element
            {
                self.calculated_margin_bottom =
                    Some(default_heading_prop!(size, font_margin_bottom));
            }
        }

        fn calc_fixed_properties(&mut self, view_width: f32, view_height: f32) -> bool {
            macro_rules! update_prop {
                ($value:expr, $prop:ident, $basis:expr $(,)?) => {{
                    let value = $value;
                    let size = value.calc($basis, view_width, view_height);
                    if set_float(&mut paste!(self.[<calculated_ $prop>]), size).is_some() {
                        log::trace!("calc_fixed_properties: updated from={value} to calculated_{}={size}", stringify!($prop));
                    }
                }};
            }

            macro_rules! update_fixed_prop {
                ($prop:ident, $basis:expr $(,)?) => {
                    if let Some(value) = self.$prop.as_ref().and_then(crate::Number::as_fixed) {
                        update_prop!(value, $prop, $basis);
                    }
                };
            }

            if let Some(value) = &self.opacity {
                update_prop!(value, opacity, 1.0);
            }
            update_fixed_prop!(border_top_left_radius, 0.0);
            update_fixed_prop!(border_top_right_radius, 0.0);
            update_fixed_prop!(border_bottom_left_radius, 0.0);
            update_fixed_prop!(border_bottom_right_radius, 0.0);

            false
        }
    }

    impl<F: FontMetrics> Calculator<F> {
        /// Calculates widths for all containers in the tree.
        ///
        /// Performs font size calculation and fixed property calculation for width-related properties.
        ///
        /// # Panics
        ///
        /// * Panics with a layout diagnostic if styled text measurement fails.
        /// * Panics if required calculated dimensions are missing.
        #[cfg_attr(feature = "profiling", profiling::function)]
        pub fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) {
            let measurements = crate::layout::calc::TextMeasurements::new(
                &self.font_metrics,
                container,
                self.defaults,
            )
            .unwrap_or_else(|error| panic!("{error}"));
            self.calc_widths_measured(bfs, container, &measurements);
            measurements
                .check()
                .unwrap_or_else(|error| panic!("{error}"));
        }

        pub(super) fn calc_widths_measured(
            &self,
            bfs: &BfsPaths,
            container: &mut Container,
            measurements: &crate::layout::calc::TextMeasurements<'_, F>,
        ) {
            let each_parent = |container: &mut Container,
                               view_width,
                               view_height,
                               context: &Container,
                               defaults| {
                container.calculate_font_size(view_width, view_height, context, defaults);
            };
            let each_child = |container: &mut Container,
                              view_width,
                              view_height,
                              context: &Container,
                              defaults| {
                container.calculate_font_size(view_width, view_height, context, defaults);

                container.calc_fixed_properties(view_width, view_height);
            };

            calc_size_on_axis!(
                measurements,
                "calc_widths",
                self,
                bfs,
                container,
                width,
                Row,
                Column,
                left,
                right,
                x,
                each_parent,
                each_child,
            );
        }
    }
}

mod pass_margin_and_padding {
    use paste::paste;

    use crate::{
        BfsPaths, Container,
        layout::{calc::Calculator, font::FontMetrics, set_float},
    };

    impl<F: FontMetrics> Calculator<F> {
        /// Calculates margin and padding for all containers in the tree.
        ///
        /// Processes margin and padding values based on container dimensions and view size.
        ///
        /// # Panics
        ///
        /// * If any of the required container properties are missing
        #[cfg_attr(feature = "profiling", profiling::function)]
        #[allow(clippy::too_many_lines)]
        pub fn calc_margin_and_padding(&self, bfs: &BfsPaths, container: &mut Container) {
            log::trace!("calc_margin_and_padding:\n{container}");

            let view_width = container.calculated_width.expect("Missing view_width");
            let view_height = container.calculated_height.expect("Missing view_height");

            #[allow(clippy::cognitive_complexity)]
            bfs.traverse_mut(container, |parent| {
                let container_width = parent.calculated_width.expect("Missing container_width");

                if let Some(gap) = parent
                    .column_gap
                    .as_ref()
                    .and_then(crate::Number::as_dynamic)
                {
                    parent.calculated_column_gap =
                        Some(gap.calc(container_width, view_width, view_height));
                }

                for child in &mut parent.children {
                    macro_rules! update_dynamic_prop {
                        ($prop:ident $(,)?) => {
                            if let Some(value) =
                                child.$prop.as_ref().and_then(crate::Number::as_dynamic)
                            {
                                let size = value.calc(container_width, view_width, view_height);
                                if set_float(&mut paste!(child.[<calculated_ $prop>]), size).is_some() {
                                    log::trace!("calc_margin_and_padding: updated from={value} to calculated_{}={size}", stringify!($prop));
                                }
                            }
                        };
                    }

                    update_dynamic_prop!(border_top_left_radius);
                    update_dynamic_prop!(border_top_right_radius);
                    update_dynamic_prop!(border_bottom_left_radius);
                    update_dynamic_prop!(border_bottom_right_radius);

                    update_dynamic_prop!(margin_left);
                    update_dynamic_prop!(margin_right);
                    update_dynamic_prop!(margin_top);
                    update_dynamic_prop!(margin_bottom);

                    update_dynamic_prop!(padding_left);
                    update_dynamic_prop!(padding_right);
                    update_dynamic_prop!(padding_top);
                    update_dynamic_prop!(padding_bottom);
                }
            });
        }
    }
}

mod pass_flex_width {
    use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{calc::Calculator, font::FontMetrics, set_float},
    };

    impl<F: FontMetrics> Calculator<F> {
        /// Applies flexbox layout calculations for width on the horizontal axis.
        ///
        /// Distributes available width among flex children based on their flex properties.
        ///
        /// # Panics
        ///
        /// * Panics with a layout diagnostic if styled text measurement fails.
        /// * Panics if required calculated dimensions are missing.
        #[cfg_attr(feature = "profiling", profiling::function)]
        pub fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) {
            let measurements = crate::layout::calc::TextMeasurements::new(
                &self.font_metrics,
                container,
                self.defaults,
            )
            .unwrap_or_else(|error| panic!("{error}"));
            Self::flex_width_measured(bfs, container, &measurements);
            measurements
                .check()
                .unwrap_or_else(|error| panic!("{error}"));
        }

        pub(super) fn flex_width_measured(
            bfs: &BfsPaths,
            container: &mut Container,
            measurements: &crate::layout::calc::TextMeasurements<'_, F>,
        ) {
            flex_on_axis!(
                measurements,
                "flex_width",
                bfs,
                container,
                width,
                Row,
                Column,
                col,
                left,
                right,
                x,
            );
        }
    }
}

mod pass_wrap_horizontal {
    use crate::{
        BfsPaths, Container, Element, float_lte,
        layout::{calc::Calculator, font::FontMetrics, set_value},
    };

    impl<F: FontMetrics> Calculator<F> {
        /// Wraps text content horizontally based on available width.
        ///
        /// Measures and adjusts text dimensions when wrapping is needed.
        ///
        /// # Panics
        ///
        /// * If any of the required container properties are missing
        #[cfg_attr(feature = "profiling", profiling::function)]
        pub fn wrap_horizontal(&self, bfs: &BfsPaths, container: &mut Container) {
            let measurements = crate::layout::calc::TextMeasurements::new(
                &self.font_metrics,
                container,
                self.defaults,
            )
            .unwrap_or_else(|error| panic!("{error}"));
            Self::wrap_horizontal_measured(bfs, container, &measurements);
            measurements
                .check()
                .unwrap_or_else(|error| panic!("{error}"));
        }

        pub(super) fn wrap_horizontal_measured(
            bfs: &BfsPaths,
            container: &mut Container,
            measurements: &crate::layout::calc::TextMeasurements<'_, F>,
        ) {
            let each_child =
                |container: &mut Container, container_width, _view_width, _view_height| {
                    let (Element::Raw { value } | Element::Text { value }) = &container.element
                    else {
                        return;
                    };
                    if float_lte!(
                        container
                            .calculated_width
                            .expect("Missing calculated_width"),
                        container_width
                    ) {
                        return;
                    }

                    log::trace!(
                        "wrap_horizontal: measuring text={value} container_width={container_width}"
                    );
                    let bounds = match measurements.measure(container, value, container_width) {
                        Ok(bounds) => bounds,
                        Err(error) => {
                            measurements.record(error);
                            return;
                        }
                    };
                    log::trace!("wrap_horizontal: measured bounds={bounds:?}");
                    let new_width = bounds.width();
                    let new_height = bounds.height();
                    log::trace!("wrap_horizontal: measured width={new_width} height={new_height}");

                    container.calculated_preferred_width = Some(new_width);
                    container.calculated_width = Some(new_width);
                    container.calculated_preferred_height = Some(new_height);
                    container.calculated_height = Some(new_height);
                    container.calculated_text_wrap_width = Some(container_width);
                };

            wrap_on_axis!(
                measurements,
                "wrap",
                Row,
                bfs,
                container,
                width,
                overflow_x,
                column_gap,
                each_child,
            );
        }
    }
}

mod pass_heights {
    use crate::{
        BfsPaths, Container, Number,
        layout::{calc::Calculator, font::FontMetrics},
    };

    impl<F: FontMetrics> Calculator<F> {
        /// Calculates heights for all containers in the tree.
        ///
        /// Performs height calculations for all containers based on their content and layout properties.
        ///
        /// # Panics
        ///
        /// * Panics with a layout diagnostic if styled text measurement fails.
        /// * Panics if required calculated dimensions are missing.
        #[cfg_attr(feature = "profiling", profiling::function)]
        pub fn calc_heights(&self, bfs: &BfsPaths, container: &mut Container) {
            let measurements = crate::layout::calc::TextMeasurements::new(
                &self.font_metrics,
                container,
                self.defaults,
            )
            .unwrap_or_else(|error| panic!("{error}"));
            self.calc_heights_measured(bfs, container, &measurements);
            measurements
                .check()
                .unwrap_or_else(|error| panic!("{error}"));
        }

        pub(super) fn calc_heights_measured(
            &self,
            bfs: &BfsPaths,
            container: &mut Container,
            measurements: &crate::layout::calc::TextMeasurements<'_, F>,
        ) {
            calc_size_on_axis!(
                measurements,
                "calc_heights",
                self,
                bfs,
                container,
                height,
                Column,
                Row,
                top,
                bottom,
                y,
                (|_, _, _, _, _| {}),
                (|_, _, _, _, _| {}),
            );
        }
    }
}

mod pass_flex_height {
    use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{calc::Calculator, font::FontMetrics, set_float},
    };

    impl<F: FontMetrics> Calculator<F> {
        /// Applies flexbox layout calculations for height on the vertical axis.
        ///
        /// Distributes available height among flex children based on their flex properties.
        ///
        /// # Panics
        ///
        /// * Panics with a layout diagnostic if styled text measurement fails.
        /// * Panics if required calculated dimensions are missing.
        #[cfg_attr(feature = "profiling", profiling::function)]
        pub fn flex_height(&self, bfs: &BfsPaths, container: &mut Container) {
            let measurements = crate::layout::calc::TextMeasurements::new(
                &self.font_metrics,
                container,
                self.defaults,
            )
            .unwrap_or_else(|error| panic!("{error}"));
            Self::flex_height_measured(bfs, container, &measurements);
            measurements
                .check()
                .unwrap_or_else(|error| panic!("{error}"));
        }

        pub(super) fn flex_height_measured(
            bfs: &BfsPaths,
            container: &mut Container,
            measurements: &crate::layout::calc::TextMeasurements<'_, F>,
        ) {
            flex_on_axis!(
                measurements,
                "flex_height",
                bfs,
                container,
                height,
                Column,
                Row,
                row,
                top,
                bottom,
                y,
            );
        }
    }
}

mod pass_positioning {
    use bumpalo::Bump;
    use hyperchad_transformer_models::{
        AlignItems, JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition, Position,
        TextAlign, Visibility,
    };

    use crate::{
        BfsPaths, Container, Element, float_lte,
        layout::{calc::Calculator, font::FontMetrics, set_float},
    };

    impl<F: FontMetrics> Calculator<F> {
        /// Positions all elements in the container tree based on layout rules.
        ///
        /// Calculates final x and y positions for all containers considering alignment,
        /// justification, text alignment, and positioning properties.
        ///
        /// Returns `true` if any positions changed, `false` otherwise.
        ///
        /// # Panics
        ///
        /// * If any of the required container properties are missing
        #[allow(clippy::too_many_lines)]
        #[cfg_attr(feature = "profiling", profiling::function)]
        pub fn position_elements(
            &self,
            arena: &Bump,
            bfs: &BfsPaths,
            container: &mut Container,
            context: &mut Container,
        ) -> bool {
            log::trace!("position_elements:\n{container}");

            let root_ptr = std::ptr::from_ref::<Container>(container);
            let root_text_align = container.text_align;
            let view_width = container.calculated_width.expect("Missing view_width");
            let view_height = container.calculated_height.expect("Missing view_height");

            let mut changed = false;

            #[allow(clippy::cognitive_complexity)]
            bfs.traverse_with_parents_ref_mut(
                true,
                context,
                container,
                |parent, relative_container| {
                    if std::ptr::eq(std::ptr::from_ref(parent), root_ptr) {
                        relative_container.calculated_x = Some(0.0);
                        relative_container.calculated_y = Some(0.0);
                        relative_container.calculated_width = Some(view_width);
                        relative_container.calculated_height = Some(view_height);
                        relative_container.text_align = root_text_align;
                    } else if parent.position == Some(Position::Relative) {
                        relative_container.calculated_x =
                            Some(parent.calculated_x.expect("Missing calculated_x"));
                        relative_container.calculated_y =
                            Some(parent.calculated_y.expect("Missing calculated_y"));
                        relative_container.calculated_width =
                            Some(parent.calculated_width.expect("Missing calculated_width"));
                        relative_container.calculated_height =
                            Some(parent.calculated_height.expect("Missing calculated_height"));
                    }

                    if let Some(align) = parent.text_align {
                        relative_container.text_align = Some(align);
                    }
                },
                |parent, relative_container| {
                    if crate::layout::calc::table_layout::structural(parent) { return; }
                    let is_top_level = std::ptr::eq(std::ptr::from_ref(parent), root_ptr);
                    let direction = parent.direction;
                    let justify_content = parent.justify_content.unwrap_or_default();
                    let align_items = parent.align_items.unwrap_or_default();
                    let container_width = parent
                        .calculated_width
                        .expect("Missing parent calculated_width");
                    let container_height = parent
                        .calculated_height
                        .expect("Missing parent calculated_height");

                    if let LayoutOverflow::Wrap { grid } = parent.overflow_x {
                        let mut last_row = 0;
                        let mut col_count = 0;
                        let mut max_col_count = 0;
                        let mut row_width = 0.0;
                        let gaps = &mut bumpalo::collections::Vec::new_in(arena);
                        let grid_cell_size = parent
                            .grid_cell_size
                            .as_ref()
                            .map(|x| x.calc(container_width, view_width, view_height));
                        let row_gap = parent.calculated_row_gap.unwrap_or_default();
                        let column_gap = parent.calculated_column_gap.unwrap_or_default();

                        #[allow(clippy::cast_precision_loss)]
                        let mut add_gap = |row_width, col_count| {
                            let mut col_count = col_count;
                            let remainder = container_width - grid_cell_size.map_or(
                                row_width,
                                |cell_size| {
                                    col_count = 1;
                                    let mut size = cell_size + column_gap;

                                    while float_lte!(size + cell_size, container_width) {
                                        col_count += 1;
                                        size += cell_size + column_gap;
                                    }

                                    column_gap.mul_add(-(col_count as f32), size)
                                }
                            );

                            let gap = match justify_content {
                                JustifyContent::Start
                                | JustifyContent::Center
                                | JustifyContent::End => column_gap,
                                JustifyContent::SpaceBetween => {
                                    remainder / ((col_count - 1) as f32)
                                }
                                JustifyContent::SpaceEvenly => {
                                    remainder / ((col_count + 1) as f32)
                                }
                            };

                            gaps.push(gap);

                            gap
                        };

                        for child in parent.relative_positioned_elements_mut().filter(|x| x.visibility != Some(Visibility::Hidden)) {
                            let Some(LayoutPosition::Wrap { row, col }) = child.calculated_position
                            else {
                                continue;
                            };
                            log::trace!("position_elements: wrap calculating gaps (r{row}, c{col})");

                            if row != last_row {
                                moosicbox_assert::assert!(row > last_row);

                                let gap = add_gap(row_width, col_count);
                                log::trace!("position_elements: (r{row}, c{col}) gap={gap}");

                                if grid && col_count > max_col_count {
                                    max_col_count = col_count;
                                }

                                row_width = 0.0;
                                col_count = 0;
                                last_row = row;
                            }

                            row_width += grid_cell_size.unwrap_or_else(|| {
                                child
                                    .calculated_width
                                    .expect("Child missing calculated_width")
                            });
                            col_count += 1;
                        }

                        add_gap(row_width, col_count);

                        #[allow(unused_assignments)]
                        if col_count > max_col_count {
                            max_col_count = col_count;
                        }

                        let mut gap = gaps.first().copied().unwrap_or_default();

                        let first_gap = |gap| match justify_content {
                            JustifyContent::Start
                            | JustifyContent::Center
                            | JustifyContent::End
                            | JustifyContent::SpaceBetween => 0.0,
                            JustifyContent::SpaceEvenly => gap,
                        };

                        let mut x = first_gap(gap);
                        let mut y = 0.0;

                        let mut max_height = 0.0;
                        last_row = 0;

                        for child in parent.relative_positioned_elements_mut().filter(|x| x.visibility != Some(Visibility::Hidden)) {
                            let Some(LayoutPosition::Wrap { row, col }) = child.calculated_position
                            else {
                                continue;
                            };
                            log::trace!("position_elements: (r{row}, c{col}) gap={gap} row_gap={row_gap} last_row={last_row} ({x}, {y})");

                            let child_width = child.bounding_calculated_width().unwrap();
                            let child_height = child.bounding_calculated_height().unwrap();

                            if row != last_row {
                                moosicbox_assert::assert!(row > last_row);

                                if !grid {
                                    gap = gaps.get(row as usize).copied().unwrap_or_default();
                                }

                                x = first_gap(gap);
                                y += max_height + row_gap;
                                max_height = 0.0;
                                last_row = row;
                                #[cfg(feature = "layout-offset")]
                                {
                                    child.calculated_offset_x = Some(x);
                                }
                            }

                            #[cfg(feature = "layout-offset")]
                            {
                                child.calculated_offset_x = Some(if col > 0 { gap } else { x });
                                child.calculated_offset_y = Some(if row > 0 { row_gap } else { 0.0 });
                            }

                            if child_height > max_height {
                                max_height = child_height;
                            }

                            log::trace!(
                                "position_elements: setting wrapped position ({x}, {y}):\n{child}"
                            );
                            if set_float(&mut child.calculated_x, x).is_some() && is_top_level {
                                update_changed!(changed, "wrapped calculated_x changed to {x}");
                            }
                            if set_float(&mut child.calculated_y, y).is_some() && is_top_level {
                                update_changed!(changed, "wrapped calculated_y changed to {y}");
                            }

                            x += child_width + gap;
                        }
                    } else {
                        let mut x = 0.0;
                        let mut y = 0.0;
                        let mut col_gap = parent.calculated_column_gap.unwrap_or_default();
                        let row_gap = parent.calculated_row_gap.unwrap_or_default();
                        let axis_gap = match direction {
                            LayoutDirection::Row => col_gap,
                            LayoutDirection::Column => row_gap,
                        };

                        macro_rules! visible_elements {
                            () => {{
                                parent
                                    .relative_positioned_elements()
                                    .filter(|x| x.visibility != Some(Visibility::Hidden))
                            }};
                        }

                        macro_rules! visible_elements_mut {
                            () => {{
                                parent
                                    .relative_positioned_elements_mut()
                                    .filter(|x| x.visibility != Some(Visibility::Hidden))
                            }};
                        }

                        macro_rules! sizes_on_axis {
                            ($direction:expr) => {{
                                visible_elements!().filter_map(|x| match $direction {
                                    LayoutDirection::Row => x.bounding_calculated_width(),
                                    LayoutDirection::Column => x.bounding_calculated_height(),
                                })
                            }};
                        }

                        match justify_content {
                            JustifyContent::Start => {}
                            JustifyContent::Center => {
                                let count = visible_elements!().count();
                                let size: f32 = sizes_on_axis!(direction).sum();
                                #[allow(clippy::cast_precision_loss)]
                                let gap_offset = (count - 1) as f32 * axis_gap;

                                match direction {
                                    LayoutDirection::Row => x += (container_width - size - gap_offset) / 2.0,
                                    LayoutDirection::Column => y += (container_height - size - gap_offset) / 2.0,
                                }
                            }
                            JustifyContent::End => {
                                let count = visible_elements!().count();
                                let size: f32 = sizes_on_axis!(direction).sum();
                                #[allow(clippy::cast_precision_loss)]
                                let gap_offset = (count - 1) as f32 * axis_gap;

                                match direction {
                                    LayoutDirection::Row => x += container_width - size - gap_offset,
                                    LayoutDirection::Column => y += container_height - size - gap_offset,
                                }
                            }
                            JustifyContent::SpaceBetween => {
                                let count = visible_elements!().count();
                                let size: f32 = sizes_on_axis!(direction).sum();

                                #[allow(clippy::cast_precision_loss)]
                                match direction {
                                    LayoutDirection::Row => {
                                        col_gap = (container_width - size) / ((count - 1) as f32);
                                    }
                                    LayoutDirection::Column => {
                                        col_gap = (container_height - size) / ((count - 1) as f32);
                                    }
                                }
                            }
                            JustifyContent::SpaceEvenly => {
                                let count = visible_elements!().count();
                                let size: f32 = sizes_on_axis!(direction).sum();

                                #[allow(clippy::cast_precision_loss)]
                                match direction {
                                    LayoutDirection::Row => {
                                        col_gap = (container_width - size) / ((count + 1) as f32);
                                    }
                                    LayoutDirection::Column => {
                                        col_gap = (container_height - size) / ((count + 1) as f32);
                                    }
                                }

                                x += col_gap;
                            }
                        }

                        if let Some(text_align) = relative_container.text_align
                            && visible_elements!().all(|x| matches!(x.element, Element::Raw { .. } | Element::Text { .. }))
                            {
                                match text_align {
                                    TextAlign::Start => {}
                                    TextAlign::Center => {
                                        let size: f32 = sizes_on_axis!(LayoutDirection::Row).sum();

                                        log::trace!("position_elements: TextAlign::{text_align:?} container_width={container_width} container_height={container_height} size={size}");

                                        x += (container_width - size) / 2.0;
                                    }
                                    TextAlign::End => {
                                        let size: f32 = sizes_on_axis!(LayoutDirection::Row).sum();

                                        log::trace!("position_elements: TextAlign::{text_align:?} container_width={container_width} container_height={container_height} size={size}");

                                        x += container_width - size;
                                    }
                                    TextAlign::Justify => {
                                        // TODO:
                                        // https://github.com/emilk/egui/issues/1724
                                        // https://docs.rs/egui/latest/egui/text/struct.LayoutJob.html
                                        todo!();
                                    }
                                }
                            }

                        for (i, child) in visible_elements_mut!().enumerate()
                        {
                            let start_x = x;
                            let start_y = y;

                            match align_items {
                                AlignItems::Start => {}
                                AlignItems::Center | AlignItems::End => {
                                    let size = match direction {
                                        LayoutDirection::Row => child.bounding_calculated_height(),
                                        LayoutDirection::Column => child.bounding_calculated_width(),
                                    }.unwrap_or_default();

                                    log::trace!("position_elements: AlignItems::{align_items:?} container_width={container_width} container_height={container_height} size={size}:\n{child}");

                                    match align_items {
                                        AlignItems::Start => unreachable!(),
                                        AlignItems::Center => match direction {
                                            LayoutDirection::Row => y += (container_height - size) / 2.0,
                                            LayoutDirection::Column => x += (container_width - size) / 2.0,
                                        },
                                        AlignItems::End => match direction {
                                            LayoutDirection::Row => y += container_height - size,
                                            LayoutDirection::Column => x += container_width - size,
                                        },
                                    }
                                }
                            }

                            log::trace!("position_elements: setting position ({x}, {y}) i={i}:\n{child}");
                            if set_float(&mut child.calculated_x, x).is_some() && is_top_level {
                                update_changed!(changed, "calculated_x changed to {x}");
                            }
                            if set_float(&mut child.calculated_y, y).is_some() && is_top_level {
                                update_changed!(changed, "calculated_y changed to {y}");
                            }

                            match direction {
                                LayoutDirection::Row => {
                                    #[cfg(feature = "layout-offset")]
                                    {
                                        child.calculated_offset_x = Some(if i == 0 { start_x } else { col_gap });
                                        child.calculated_offset_y = Some(y);
                                    }
                                    x += child.bounding_calculated_width().unwrap() + col_gap;
                                    y = start_y;
                                }
                                LayoutDirection::Column => {
                                    #[cfg(feature = "layout-offset")]
                                    {
                                        child.calculated_offset_x = Some(x);
                                        child.calculated_offset_y = Some(if i == 0 { start_y } else { row_gap });
                                    }
                                    x = start_x;
                                    y += child.bounding_calculated_height().unwrap() + row_gap;
                                }
                            }
                        }
                    }

                    // absolute positioned

                    let Container {
                        calculated_width: Some(width),
                        calculated_height: Some(height),
                        ..
                    } = relative_container
                    else {
                        panic!("Missing relative_container size");
                    };
                    let width = *width;
                    let height = *height;

                    macro_rules! position_absolute {
                        ($iter:expr, $width:expr, $height:expr) => {
                            let width = $width;
                            let height = $height;
                            for child in ($iter) {
                                let mut x = 0.0;
                                let mut y = 0.0;

                                if let Some(left) = &child.left {
                                    let left = left.calc(width, view_width, view_height);
                                    x = left;
                                }
                                if let Some(right) = &child.right {
                                    let right = right.calc(width, view_width, view_height);
                                    let bounding_width = child.bounding_calculated_width().unwrap();
                                    let right = width - right - bounding_width;
                                    x = right;
                                }
                                if let Some(top) = &child.top {
                                    let top = top.calc(height, view_width, view_height);
                                    y = top;
                                }
                                if let Some(bottom) = &child.bottom {
                                    let bottom = bottom.calc(height, view_width, view_height);
                                    let bounding_height = child.bounding_calculated_height().unwrap();
                                    let bottom = height - bottom - bounding_height;
                                    y = bottom;
                                }

                                child.calculated_x = Some(x);
                                child.calculated_y = Some(y);
                            }
                        };
                    }

                    position_absolute!(parent.absolute_positioned_elements_mut(), width, height);
                    position_absolute!(parent.fixed_positioned_elements_mut(), view_width, view_height);
                },
            );

            changed
        }
    }
}

impl Container {
    fn is_expandable(&self, parent: &Self) -> bool {
        !self.is_span() && (!parent.is_flex_container() || self.flex.is_some())
    }
}

macro_rules! axis_sum_func {
    ($prop:ident, $unit:ident, $x:ident, $y:ident $(,)?) => {
        paste! {
            impl Container {
                #[doc = concat!("Returns the sum of calculated `", stringify!($prop), "` values on the ", stringify!($unit), " axis.\n\nReturns `None` if neither value is set.")]
                #[must_use]
                pub fn [<$prop _ $unit>](&self) -> Option<f32> {
                    let mut value = if let Some(x) = self.[<calculated_ $prop _ $x>] {
                        Some(x)
                    } else {
                        None
                    };
                    if let Some(y) = self.[<calculated_ $prop _ $y>] {
                        value.replace(value.map_or(y, |x| x + y));
                    }
                    value
                }
            }
        }
    };
}

axis_sum_func!(margin, x, left, right);
axis_sum_func!(margin, y, top, bottom);
axis_sum_func!(padding, x, left, right);
axis_sum_func!(padding, y, top, bottom);

impl Container {
    /// Returns the sum of left and right border widths.
    ///
    /// Returns `None` if neither border is set.
    #[must_use]
    pub fn border_x(&self) -> Option<f32> {
        let mut borders = if let Some((_, border_left)) = self.calculated_border_left {
            Some(border_left)
        } else {
            None
        };
        if let Some((_, border_right)) = self.calculated_border_right {
            borders.replace(borders.map_or(border_right, |x| x + border_right));
        }
        borders
    }

    /// Returns the sum of top and bottom border widths.
    ///
    /// Returns `None` if neither border is set.
    #[must_use]
    pub fn border_y(&self) -> Option<f32> {
        let mut borders = if let Some((_, border_top)) = self.calculated_border_top {
            Some(border_top)
        } else {
            None
        };
        if let Some((_, border_bottom)) = self.calculated_border_bottom {
            borders.replace(borders.map_or(border_bottom, |x| x + border_bottom));
        }
        borders
    }

    /// Returns the total bounding width including content, padding, scrollbar, and margin.
    ///
    /// Returns `None` if calculated width is not set.
    #[must_use]
    pub fn bounding_calculated_width(&self) -> Option<f32> {
        self.calculated_width.map(|width| {
            width
                + self.padding_x().unwrap_or(0.0)
                + self.scrollbar_right.unwrap_or(0.0)
                + self.margin_x().unwrap_or(0.0)
        })
    }

    /// Returns the total bounding height including content, padding, scrollbar, and margin.
    ///
    /// Returns `None` if calculated height is not set.
    #[must_use]
    pub fn bounding_calculated_height(&self) -> Option<f32> {
        self.calculated_height.map(|height| {
            height
                + self.padding_y().unwrap_or(0.0)
                + self.scrollbar_bottom.unwrap_or(0.0)
                + self.margin_y().unwrap_or(0.0)
        })
    }
}

#[cfg(test)]
mod test {
    use hyperchad_transformer_models::AlignItems;
    use maud::html;
    use paste::paste;
    use pretty_assertions::assert_eq;

    use crate::{
        Calculation, Container, Element, HeaderSize, Number, Position,
        layout::{
            Calc as _,
            font::{FontMetrics, FontMetricsBounds, FontMetricsRow},
            get_scrollbar_size,
        },
        models::{JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition},
    };

    use super::{Calculator, CalculatorDefaults};

    fn compare_containers(a: &Container, b: &Container) {
        assert_eq!(
            a.display_to_string(
                true,
                true,
                #[cfg(feature = "format")]
                true,
                #[cfg(feature = "syntax-highlighting")]
                false
            )
            .unwrap(),
            b.display_to_string(
                true,
                true,
                #[cfg(feature = "format")]
                true,
                #[cfg(feature = "syntax-highlighting")]
                false
            )
            .unwrap()
        );
    }

    struct DefaultFontMetrics;

    #[cfg_attr(feature = "profiling", profiling::all_functions)]
    impl FontMetrics for DefaultFontMetrics {
        fn measure_text(
            &self,
            request: &crate::layout::font::TextMeasureRequest<'_>,
        ) -> Result<FontMetricsBounds, crate::layout::font::TextMeasureError> {
            let text = request.text;
            let size = request.size;
            let wrap_width =
                if request.whitespace == Some(hyperchad_transformer_models::WhiteSpace::Preserve) {
                    f32::INFINITY
                } else {
                    request.wrap_width
                };
            let mut rows = vec![];
            #[allow(clippy::cast_precision_loss)]
            let mut width = text.len() as f32 * size;

            #[allow(clippy::while_float)]
            while width > wrap_width {
                rows.push(FontMetricsRow {
                    width: wrap_width,
                    height: size,
                });
                width -= wrap_width;
            }

            if width > 0.0 {
                rows.push(FontMetricsRow {
                    width,
                    height: size,
                });
            }

            Ok(FontMetricsBounds { rows })
        }
    }

    #[allow(clippy::derivable_impls)]
    impl Default for CalculatorDefaults {
        fn default() -> Self {
            Self {
                font_size: Default::default(),
                font_margin_top: Default::default(),
                font_margin_bottom: Default::default(),
                h1_font_size: Default::default(),
                h1_font_margin_top: Default::default(),
                h1_font_margin_bottom: Default::default(),
                h2_font_size: Default::default(),
                h2_font_margin_top: Default::default(),
                h2_font_margin_bottom: Default::default(),
                h3_font_size: Default::default(),
                h3_font_margin_top: Default::default(),
                h3_font_margin_bottom: Default::default(),
                h4_font_size: Default::default(),
                h4_font_margin_top: Default::default(),
                h4_font_margin_bottom: Default::default(),
                h5_font_size: Default::default(),
                h5_font_margin_top: Default::default(),
                h5_font_margin_bottom: Default::default(),
                h6_font_size: Default::default(),
                h6_font_margin_top: Default::default(),
                h6_font_margin_bottom: Default::default(),
            }
        }
    }

    static CALCULATOR: Calculator<DefaultFontMetrics> = Calculator::new(
        DefaultFontMetrics,
        CalculatorDefaults {
            font_size: 14.0,
            font_margin_top: 0.0,
            font_margin_bottom: 0.0,
            h1_font_size: 32.0,
            h1_font_margin_top: 0.0,
            h1_font_margin_bottom: 0.0,
            h2_font_size: 24.0,
            h2_font_margin_top: 0.0,
            h2_font_margin_bottom: 0.0,
            h3_font_size: 18.72,
            h3_font_margin_top: 0.0,
            h3_font_margin_bottom: 0.0,
            h4_font_size: 16.0,
            h4_font_margin_top: 0.0,
            h4_font_margin_bottom: 0.0,
            h5_font_size: 13.28,
            h5_font_margin_top: 0.0,
            h5_font_margin_bottom: 0.0,
            h6_font_size: 10.72,
            h6_font_margin_top: 0.0,
            h6_font_margin_bottom: 0.0,
        },
    );

    mod scrollbar {
        use super::*;

        #[test_log::test]
        #[ignore = "Unimplemented"]
        fn calc_calculates_resized_wrapped_content_with_scrollbar_and_padding_correctly() {
            let mut container: Container = html! {
                div sx-width="100%" sx-height="100%" sx-position="relative" {
                    section sx-dir="row" sx-height=("calc(100% - 140px)") {
                        aside sx-width="calc(max(240, min(280, 15%)))" {}
                        main sx-overflow-y="auto" {
                            div
                                sx-dir="row"
                                sx-overflow-x="wrap"
                                sx-justify-content="space-evenly"
                                sx-gap=(15)
                                sx-padding-left=(30)
                                sx-padding-right=(30)
                                sx-padding-top=(15)
                                sx-padding-bottom=(15)
                            {
                                @for _ in 0..19 {
                                    div sx-width=(200) sx-height=(200 + 30) {}
                                }
                            }
                        }
                    }
                    div
                        sx-width="calc(min(500, 30%))"
                        sx-height="calc(100% - 200)"
                        sx-padding=(20)
                        sx-position="absolute"
                        sx-bottom=(170)
                        sx-right=(0)
                    {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(1600.0);
            container.calculated_height = Some(1000.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            let container = container.children[0].children[0].children[1].children[0].clone();

            compare_containers(
                &container.clone(),
                &Container {
                    calculated_width: Some(1360.0 - 30.0 - 30.0 - f32::from(get_scrollbar_size())),
                    calculated_height: Some(920.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_padding_left: Some(30.0),
                    calculated_padding_right: Some(30.0),
                    calculated_padding_top: Some(15.0),
                    calculated_padding_bottom: Some(15.0),
                    ..container
                },
            );
        }

        #[test_log::test]
        #[ignore = "Unimplemented"]
        fn calc_auto_y_wraps_nested_elements_properly_by_taking_into_account_scrollbar_size() {
            let mut container = Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            width: Some(Number::Integer(25)),
                            ..Default::default()
                        },
                        Container {
                            width: Some(Number::Integer(25)),
                            ..Default::default()
                        },
                        Container {
                            width: Some(Number::Integer(25)),
                            ..Default::default()
                        },
                        Container {
                            width: Some(Number::Integer(25)),
                            ..Default::default()
                        },
                        Container {
                            width: Some(Number::Integer(25)),
                            ..Default::default()
                        },
                    ],
                    calculated_width: Some(75.0),
                    calculated_height: Some(40.0),
                    direction: LayoutDirection::Row,
                    overflow_x: LayoutOverflow::Wrap { grid: true },
                    overflow_y: LayoutOverflow::Expand,
                    ..Default::default()
                }],

                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                overflow_y: LayoutOverflow::Auto,
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(25.0),
                                calculated_y: Some(0.0),
                                ..container.children[0].children[1].clone()
                            },
                            Container {
                                calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(40.0),
                                ..container.children[0].children[2].clone()
                            },
                            Container {
                                calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(25.0),
                                calculated_y: Some(40.0),
                                ..container.children[0].children[3].clone()
                            },
                            Container {
                                calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(80.0),
                                ..container.children[0].children[4].clone()
                            },
                        ],
                        ..container.children[0].clone()
                    }],

                    calculated_width: Some(75.0 - f32::from(get_scrollbar_size())),
                    calculated_height: Some(40.0),
                    ..container
                },
            );
        }
    }

    #[test_log::test]
    fn text_wrap_metadata_tracks_committed_constraint_and_resets() {
        let mut root: Container = html! { div { "aa aa aa" } }
            .into_string()
            .try_into()
            .unwrap();
        root.calculated_width = Some(200.0);
        root.calculated_height = Some(100.0);
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        pretty_assertions::assert_eq!(
            root.children[0].children[0].calculated_text_wrap_width,
            Some(f32::INFINITY)
        );
        root.calculated_width = Some(56.0);
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        pretty_assertions::assert_eq!(
            root.children[0].children[0].calculated_text_wrap_width,
            Some(56.0)
        );
        root.calculated_width = Some(200.0);
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        pretty_assertions::assert_eq!(
            root.children[0].children[0].calculated_text_wrap_width,
            Some(f32::INFINITY)
        );
        super::reset_text_wrap_metadata(&mut root);
        pretty_assertions::assert_eq!(
            root.children[0].children[0].calculated_text_wrap_width,
            None
        );
    }

    fn control_fixture() -> Container {
        Container {
            children: vec![Container {
                children: vec![Container {
                    element: Element::Input {
                        input: crate::Input::Text {
                            value: None,
                            placeholder: None,
                        },
                        name: Some("name".into()),
                        autofocus: None,
                    },
                    ..Default::default()
                }],
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(100.0),
            ..Default::default()
        }
    }

    #[test_log::test]
    fn empty_control_intrinsics_fit_narrow_parent_and_recalculate() {
        let mut root = control_fixture();
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        let input = &root.children[0].children[0];
        pretty_assertions::assert_eq!(input.calculated_height, Some(14.0));
        assert!(
            input
                .calculated_width
                .is_some_and(|width| width > 0.0 && width <= 100.0)
        );
        let before = super::table_layout::geometry(&root);
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        pretty_assertions::assert_eq!(before, super::table_layout::geometry(&root));
        root.calculated_width = Some(50.0);
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        assert!(
            root.children[0].children[0]
                .calculated_width
                .is_some_and(|width| width > 0.0 && width <= 50.0)
        );
    }

    #[test_log::test]
    fn control_intrinsics_preserve_explicit_zero_percent_and_padding() {
        let mut root = control_fixture();
        let input = &mut root.children[0].children[0];
        input.width = Some(Number::Integer(0));
        input.height = Some(Number::Integer(0));
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        pretty_assertions::assert_eq!(root.children[0].children[0].calculated_width, Some(0.0));
        pretty_assertions::assert_eq!(root.children[0].children[0].calculated_height, Some(0.0));
        let input = &mut root.children[0].children[0];
        input.width = Some(Number::IntegerPercent(50));
        input.height = None;
        input.padding_left = Some(Number::Integer(2));
        input.padding_right = Some(Number::Integer(2));
        crate::layout::Calc::try_calc(&CALCULATOR, &mut root).unwrap();
        assert!(
            root.children[0].children[0]
                .calculated_width
                .is_some_and(|width| width > 0.0 && width <= 50.0)
        );
        pretty_assertions::assert_eq!(root.children[0].children[0].calculated_height, Some(14.0));
    }

    struct RejectControlMetrics;
    impl FontMetrics for RejectControlMetrics {
        fn measure_text(
            &self,
            _: &crate::layout::font::TextMeasureRequest<'_>,
        ) -> Result<FontMetricsBounds, crate::layout::font::TextMeasureError> {
            Err(crate::layout::font::TextMeasureError::Backend {
                message: "control metrics failed".into(),
            })
        }
    }

    #[test_log::test]
    fn control_intrinsic_metrics_failure_keeps_structural_path() {
        let mut root = control_fixture();
        let calculator = Calculator::new(RejectControlMetrics, CALCULATOR.defaults);
        let error = crate::layout::Calc::try_calc(&calculator, &mut root).unwrap_err();
        pretty_assertions::assert_eq!(error.path, vec![0, 0]);
        assert!(matches!(
            error.kind,
            crate::layout::LayoutErrorKind::TextMeasurement(_)
        ));
    }

    struct RejectStyle;
    impl FontMetrics for RejectStyle {
        fn measure_text(
            &self,
            request: &crate::layout::font::TextMeasureRequest<'_>,
        ) -> Result<FontMetricsBounds, crate::layout::font::TextMeasureError> {
            use crate::layout::font::{TextFontFamily, TextFontWeight, TextMeasureError};
            assert!(
                matches!(request.family, TextFontFamily::Authored(names) if names == ["Explicit"])
            );
            assert!(matches!(request.weight, TextFontWeight::Absolute(700)));
            Err(TextMeasureError::UnsupportedStyle {
                message: "test authored style rejected".into(),
            })
        }
    }

    #[test_log::test]
    fn styled_measurement_inherits_explicit_provenance_and_reports_path() {
        let mut root = Container {
            font_family: Some(vec!["Explicit".into()]),
            font_weight: Some(hyperchad_transformer_models::FontWeight::Bold),
            children: vec![Container {
                children: vec![Container {
                    element: Element::Raw {
                        value: "text".into(),
                    },
                    ..Default::default()
                }],
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(100.0),
            ..Default::default()
        };
        let calculator = Calculator::new(RejectStyle, CALCULATOR.defaults);
        let error = crate::layout::Calc::try_calc(&calculator, &mut root).unwrap_err();
        pretty_assertions::assert_eq!(error.path, vec![0, 0]);
        assert!(matches!(
            error.kind,
            crate::layout::LayoutErrorKind::TextMeasurement(
                crate::layout::font::TextMeasureError::UnsupportedStyle { .. }
            )
        ));
    }

    struct RejectWord;
    impl FontMetrics for RejectWord {
        fn measure_text(
            &self,
            request: &crate::layout::font::TextMeasureRequest<'_>,
        ) -> Result<FontMetricsBounds, crate::layout::font::TextMeasureError> {
            use crate::layout::font::TextMeasureError;
            if request.text == "two" {
                return Err(TextMeasureError::Backend {
                    message: "minimum measurement failed".into(),
                });
            }
            DefaultFontMetrics.measure_text(request)
        }
    }

    #[test_log::test]
    fn table_minimum_measurement_error_is_not_replaced_with_zero() {
        let mut root: Container = html! { table { tr { td { "one two" } } } }
            .into_string()
            .try_into()
            .unwrap();
        root.calculated_width = Some(100.0);
        root.calculated_height = Some(100.0);
        let calculator = Calculator::new(RejectWord, CALCULATOR.defaults);
        let error = crate::layout::Calc::try_calc(&calculator, &mut root).unwrap_err();
        pretty_assertions::assert_eq!(error.path, vec![0, 0, 0, 0]);
        assert!(matches!(
            error.kind,
            crate::layout::LayoutErrorKind::TextMeasurement(_)
        ));
    }

    mod table {
        use super::*;
        use crate::layout::Calc;

        fn fixture(markup: String, width: f32) -> Container {
            let mut root: Container = markup.try_into().unwrap();
            root.calculated_width = Some(width);
            root.calculated_height = Some(300.0);
            root
        }

        #[test_log::test]
        fn constrained_track_deficit_and_surplus_are_deterministic() {
            let mut tracks = vec![40.0, 30.0];
            crate::layout::calc::table_layout::distribute(&mut tracks, &[20.0, 10.0], 100.0);
            pretty_assertions::assert_eq!(tracks, vec![55.0, 45.0]);
            crate::layout::calc::table_layout::distribute(&mut tracks, &[40.0, 10.0], 55.0);
            pretty_assertions::assert_eq!(tracks, vec![40.0, 15.0]);
            crate::layout::calc::table_layout::distribute(&mut tracks, &[40.0, 10.0], 20.0);
            pretty_assertions::assert_eq!(tracks, vec![40.0, 10.0]);
        }

        #[test_log::test]
        fn row_spans_reserve_slots_and_measure_combined_height() {
            let mut root = fixture(
                html! { table {
                    tbody {
                        tr { td sx-height=(40) {} td sx-height=(10) {} }
                        tr { td sx-height=(10) {} }
                    }
                } }
                .into_string(),
                100.0,
            );
            root.children[0].children[0].children[0].children[0].element = Element::TD {
                rows: Some(Number::Integer(2)),
                columns: None,
            };
            CALCULATOR.try_calc(&mut root).unwrap();
            let body = &root.children[0].children[0];
            pretty_assertions::assert_eq!(
                body.children[0].children[0].calculated_height,
                Some(40.0)
            );
            pretty_assertions::assert_eq!(body.children[1].children[0].calculated_x, Some(50.0));
            pretty_assertions::assert_eq!(body.children[1].calculated_y, Some(20.0));
            pretty_assertions::assert_eq!(body.calculated_height, Some(40.0));
        }

        #[test_log::test]
        fn row_span_cannot_cross_body_boundary() {
            let mut root = fixture(
                html! { table {
                    tbody { tr { td {} } }
                    tbody { tr { td {} } }
                } }
                .into_string(),
                100.0,
            );
            root.children[0].children[0].children[0].children[0].element = Element::TD {
                rows: Some(Number::Integer(2)),
                columns: None,
            };
            let error = CALCULATOR.try_calc(&mut root).unwrap_err();
            pretty_assertions::assert_eq!(error.path, vec![0, 0, 0, 0]);
            pretty_assertions::assert_eq!(
                error.kind,
                crate::layout::LayoutErrorKind::InvalidTableStructure
            );
        }

        #[test_log::test]
        fn repeated_empty_table_calculation_is_stable() {
            let mut root = fixture(html! { table {} }.into_string(), 100.0);
            CALCULATOR.try_calc(&mut root).unwrap();
            let before = crate::layout::calc::table_layout::geometry(&root);
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(
                before,
                crate::layout::calc::table_layout::geometry(&root)
            );
            pretty_assertions::assert_eq!(root.children[0].calculated_height, Some(0.0));
        }

        #[test_log::test]
        fn hidden_rows_do_not_advance_table_sections() {
            let mut root = fixture(
                html! { table {
                    tbody { tr { td { "a" } } tr { td sx-height=(100) {} } }
                    tbody { tr { td { "c" } } }
                } }
                .into_string(),
                100.0,
            );
            root.children[0].children[0].children[1].hidden = Some(true);
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(root.children[0].children[1].calculated_y, Some(14.0));
            pretty_assertions::assert_eq!(root.children[0].calculated_height, Some(28.0));
        }

        #[test_log::test]
        fn nested_table_measurement_propagates_to_following_sibling() {
            let mut root = fixture(
                html! {
                    table { tr { td { table { tr { td sx-height=(30) {} } } } } }
                    div sx-height=(10) {}
                }
                .into_string(),
                100.0,
            );
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(root.children[1].calculated_y, Some(30.0));
            pretty_assertions::assert_eq!(
                root.children[0].children[0].children[0].calculated_height,
                Some(30.0)
            );
        }

        struct Legacy;
        impl Calc for Legacy {
            fn calc(&self, _: &mut Container) -> bool {
                panic!("unchecked entry must not run")
            }
        }

        #[test_log::test]
        fn checked_legacy_implementation_is_explicitly_unsupported() {
            let error = Legacy.try_calc(&mut Container::default()).unwrap_err();
            pretty_assertions::assert_eq!(
                error.kind,
                crate::layout::LayoutErrorKind::CheckedLayoutUnsupported
            );
        }

        #[test_log::test]
        fn checked_invalid_spans_fail_before_mutation() {
            for value in [
                Number::Integer(0),
                Number::Integer(-1),
                Number::Real(1.5),
                Number::Real(f32::NAN),
                Number::Real(f32::INFINITY),
                Number::IntegerPercent(2),
            ] {
                let mut root = fixture(html! { table { tr { td {} } } }.into_string(), 100.0);
                root.children[0].children[0].children[0].element = Element::TD {
                    rows: None,
                    columns: Some(value),
                };
                let before = crate::layout::calc::table_layout::geometry(&root);
                let error = CALCULATOR.try_calc(&mut root).unwrap_err();
                pretty_assertions::assert_eq!(error.path, vec![0, 0, 0]);
                pretty_assertions::assert_eq!(
                    error.kind,
                    crate::layout::LayoutErrorKind::InvalidTableSpan {
                        axis: crate::layout::TableSpanAxis::Column
                    }
                );
                pretty_assertions::assert_eq!(
                    before,
                    crate::layout::calc::table_layout::geometry(&root)
                );
            }
        }

        #[test_log::test]
        #[should_panic(expected = "InvalidTableSpan")]
        fn legacy_calc_panics_with_checked_diagnostic() {
            let mut root = fixture(html! { table { tr { td {} } } }.into_string(), 100.0);
            root.children[0].children[0].children[0].element = Element::TD {
                rows: Some(Number::Integer(0)),
                columns: None,
            };
            CALCULATOR.calc(&mut root);
        }

        #[test_log::test]
        fn checked_invalid_table_structure_has_root_relative_path() {
            let mut root = fixture(
                html! { div { table { tr { td {} } } } }.into_string(),
                100.0,
            );
            root.children[0].children[0].children[0].children[0].element = Element::Div;
            let error = CALCULATOR.try_calc(&mut root).unwrap_err();
            pretty_assertions::assert_eq!(error.path, vec![0, 0, 0, 0]);
            pretty_assertions::assert_eq!(
                error.kind,
                crate::layout::LayoutErrorKind::InvalidTableStructure
            );
        }

        #[test_log::test]
        fn checked_span_budget_is_bounded() {
            let mut root = fixture(html! { table { tr { td {} } } }.into_string(), 100.0);
            root.children[0].children[0].children[0].element = Element::TD {
                rows: None,
                columns: Some(Number::Integer(1025)),
            };
            pretty_assertions::assert_eq!(
                CALCULATOR.try_calc(&mut root).unwrap_err().kind,
                crate::layout::LayoutErrorKind::TableBudgetExceeded
            );
        }

        #[test_log::test]
        fn multiple_bodies_and_direct_rows_survive_both_iterators() {
            let mut root = fixture(
                html! { table {
                    tbody { tr { td { "a" } } }
                    tr { td { "b" } }
                    tbody { tr { td { "c" } } }
                } }
                .into_string(),
                100.0,
            );
            let table = &mut root.children[0];
            pretty_assertions::assert_eq!(table.table_iter().rows.count(), 3);
            let mut observed = 0;
            for row in table
                .table_iter_mut_with_observer(Some(|_: &mut Container| observed += 1))
                .rows
            {
                for cell in row {
                    cell.calculated_width = Some(17.0);
                }
            }
            pretty_assertions::assert_eq!(observed, 3);
            assert!(
                table
                    .table_iter()
                    .rows
                    .flatten()
                    .all(|cell| cell.calculated_width == Some(17.0))
            );
            CALCULATOR.try_calc(&mut root).unwrap();
            let table = &root.children[0];
            pretty_assertions::assert_eq!(table.table_iter().rows.count(), 3);
            pretty_assertions::assert_eq!(table.children[2].calculated_y, Some(28.0));
        }

        #[test_log::test]
        fn empty_padded_cell_has_zero_content_height() {
            let mut root = fixture(
                html! { table { thead { tr { th sx-padding-x=(10) sx-padding-y=(15) {} } } } }
                    .into_string(),
                100.0,
            );
            CALCULATOR.try_calc(&mut root).unwrap();
            let cell = &root.children[0].children[0].children[0].children[0];
            pretty_assertions::assert_eq!(cell.calculated_width, Some(80.0));
            pretty_assertions::assert_eq!(cell.calculated_height, Some(0.0));
            pretty_assertions::assert_eq!(cell.bounding_calculated_height(), Some(30.0));
        }

        #[test_log::test]
        fn spans_share_tracks_and_duplicate_ids_are_not_identity() {
            let mut root = fixture(
                html! { table {
                    tr { td { "a" } td { "b" } }
                    tr { td { "c" } }
                } }
                .into_string(),
                100.0,
            );
            root.children[0].children[1].children[0].element = Element::TD {
                rows: None,
                columns: Some(Number::Real(2.0)),
            };
            CALCULATOR.try_calc(&mut root).unwrap();
            let table = &root.children[0];
            pretty_assertions::assert_eq!(
                table.children[0].children[0].calculated_width,
                Some(50.0)
            );
            pretty_assertions::assert_eq!(table.children[0].children[1].calculated_x, Some(50.0));
            pretty_assertions::assert_eq!(
                table.children[1].children[0].calculated_width,
                Some(100.0)
            );
            pretty_assertions::assert_eq!(table.children[1].calculated_y, Some(14.0));
        }

        #[test_log::test]
        fn resize_and_content_changes_remeasure_table_and_following_sibling() {
            let mut root = fixture(
                html! {
                    table { tr { td { "aa aa aa" } } }
                    div sx-height=(10) {}
                }
                .into_string(),
                112.0,
            );
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(root.children[1].calculated_y, Some(14.0));
            root.calculated_width = Some(56.0);
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(root.children[1].calculated_y, Some(28.0));
            root.calculated_width = Some(112.0);
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(root.children[1].calculated_y, Some(14.0));
            root.children[0].children[0].children[0].children[0].element = Element::Raw {
                value: String::new(),
            };
            CALCULATOR.try_calc(&mut root).unwrap();
            pretty_assertions::assert_eq!(root.children[1].calculated_y, Some(0.0));
        }

        #[test_log::test]
        #[allow(clippy::too_many_lines)]
        fn calc_can_calc_table_column_and_row_sizes() {
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        element: Element::Div,
                                        width: Some(Number::Integer(40)),
                                        height: Some(Number::Integer(10)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        element: Element::Div,
                                        width: Some(Number::Integer(30)),
                                        height: Some(Number::Integer(20)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        element: Element::Div,
                                        width: Some(Number::Integer(10)),
                                        height: Some(Number::Integer(40)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        element: Element::Div,
                                        width: Some(Number::Integer(20)),
                                        height: Some(Number::Integer(30)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                calculated_width: Some(70.0),
                calculated_height: Some(80.0),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(40.0),
                                            calculated_height: Some(10.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(40.0),
                                        calculated_height: Some(20.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(30.0),
                                            calculated_height: Some(20.0),
                                            ..container.children[0].children[0].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(30.0),
                                        calculated_height: Some(20.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(70.0),
                                calculated_height: Some(20.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(10.0),
                                            calculated_height: Some(40.0),
                                            ..container.children[0].children[1].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(40.0),
                                        calculated_height: Some(40.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(20.0),
                                            calculated_height: Some(30.0),
                                            ..container.children[0].children[1].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(30.0),
                                        calculated_height: Some(40.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(70.0),
                                calculated_height: Some(40.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(70.0),
                        calculated_height: Some(20.0 + 40.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[allow(clippy::too_many_lines)]
        fn calc_can_calc_table_column_and_row_sizes_and_expand_to_fill_width() {
            // P4 policy correction: Equal surplus increments preserve the intrinsic column-width difference.
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        width: Some(Number::Integer(40)),
                                        height: Some(Number::Integer(10)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        width: Some(Number::Integer(30)),
                                        height: Some(Number::Integer(20)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        width: Some(Number::Integer(10)),
                                        height: Some(Number::Integer(40)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        width: Some(Number::Integer(20)),
                                        height: Some(Number::Integer(30)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                calculated_width: Some(100.0),
                calculated_height: Some(80.0),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(40.0),
                                            calculated_height: Some(10.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(55.0),
                                        calculated_height: Some(20.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(30.0),
                                            calculated_height: Some(20.0),
                                            ..container.children[0].children[0].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(45.0),
                                        calculated_height: Some(20.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(20.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(10.0),
                                            calculated_height: Some(40.0),
                                            ..container.children[0].children[1].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(55.0),
                                        calculated_height: Some(40.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(20.0),
                                            calculated_height: Some(30.0),
                                            ..container.children[0].children[1].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(45.0),
                                        calculated_height: Some(40.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(40.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0 + 40.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[allow(clippy::too_many_lines)]
        fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells() {
            // P4 policy correction: Equal surplus increments preserve the intrinsic column-width difference.
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        width: Some(Number::Integer(40)),
                                        height: Some(Number::Integer(10)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container {
                                        width: Some(Number::Integer(20)),
                                        height: Some(Number::Integer(30)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                calculated_width: Some(100.0),
                calculated_height: Some(80.0),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(40.0),
                                            calculated_height: Some(10.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(60.0),
                                        calculated_height: Some(10.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        calculated_width: Some(40.0),
                                        calculated_height: Some(10.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(10.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        calculated_width: Some(60.0),
                                        calculated_height: Some(30.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(20.0),
                                            calculated_height: Some(30.0),
                                            ..container.children[0].children[1].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(40.0),
                                        calculated_height: Some(30.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(30.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(10.0 + 30.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[allow(clippy::too_many_lines)]
        fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells_when_all_are_unsized()
         {
            // P4 policy correction: Empty content has zero intrinsic height; there is no default 25px cell.
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD {
                                        rows: None,
                                        columns: None,
                                    },
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                            ],
                            ..Default::default()
                        },
                    ],
                    ..Default::default()
                }],
                calculated_width: Some(100.0),
                calculated_height: Some(80.0),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(0.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(0.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(0.0),
                                            ..container.children[0].children[0].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(0.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(0.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(0.0),
                                            ..container.children[0].children[1].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(0.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(0.0),
                                            ..container.children[0].children[1].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(0.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(0.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(0.0 + 0.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[allow(clippy::too_many_lines)]
        fn calc_can_calc_table_column_and_row_sizes_and_auto_size_raw_data() {
            // P4 policy correction: Test metrics measure "test" at 56x14. Unbreakable minima overflow the 100px allocation.
            let mut container: Container = html! {
                table {
                    tr {
                        td { "test" }
                        td { "test" }
                    }
                    tr {
                        td { "test" }
                        td { "test" }
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(80.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![
                                    Container {
                                        children: container.children[0].children[0].children[0]
                                            .children
                                            .clone(),
                                        calculated_width: Some(56.0),
                                        calculated_height: Some(14.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        children: container.children[0].children[0].children[1]
                                            .children
                                            .clone(),
                                        calculated_width: Some(56.0),
                                        calculated_height: Some(14.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(112.0),
                                calculated_height: Some(14.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        children: container.children[0].children[1].children[0]
                                            .children
                                            .clone(),
                                        calculated_width: Some(56.0),
                                        calculated_height: Some(14.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: container.children[0].children[1].children[1]
                                            .children
                                            .clone(),
                                        calculated_width: Some(56.0),
                                        calculated_height: Some(14.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(112.0),
                                calculated_height: Some(14.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(14.0 + 14.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[allow(clippy::too_many_lines)]
        fn calc_can_calc_table_column_and_row_sizes_with_tbody() {
            // P4 policy correction: Test metrics measure "test" at 56x14. Unbreakable minima overflow the 100px allocation.
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![Container {
                        element: Element::TBody,
                        children: vec![
                            Container {
                                element: Element::TR,
                                direction: LayoutDirection::Row,
                                children: vec![
                                    Container {
                                        element: Element::TD {
                                            rows: None,
                                            columns: None,
                                        },
                                        children: vec![Container {
                                            element: Element::Raw {
                                                value: "test".to_string(),
                                            },
                                            ..Container::default()
                                        }],
                                        ..Container::default()
                                    },
                                    Container {
                                        element: Element::TD {
                                            rows: None,
                                            columns: None,
                                        },
                                        children: vec![Container {
                                            element: Element::Raw {
                                                value: "test".to_string(),
                                            },
                                            ..Container::default()
                                        }],
                                        ..Container::default()
                                    },
                                ],
                                ..Default::default()
                            },
                            Container {
                                element: Element::TR,
                                direction: LayoutDirection::Row,
                                children: vec![
                                    Container {
                                        element: Element::TD {
                                            rows: None,
                                            columns: None,
                                        },
                                        children: vec![Container {
                                            element: Element::Raw {
                                                value: "test".to_string(),
                                            },
                                            ..Container::default()
                                        }],
                                        ..Container::default()
                                    },
                                    Container {
                                        element: Element::TD {
                                            rows: None,
                                            columns: None,
                                        },
                                        children: vec![Container {
                                            element: Element::Raw {
                                                value: "test".to_string(),
                                            },
                                            ..Container::default()
                                        }],
                                        ..Container::default()
                                    },
                                ],
                                ..Default::default()
                            },
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                calculated_width: Some(100.0),
                calculated_height: Some(80.0),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![
                                Container {
                                    children: vec![
                                        Container {
                                            children: container.children[0].children[0].children[0]
                                                .children[0]
                                                .children
                                                .clone(),
                                            calculated_width: Some(56.0),
                                            calculated_height: Some(14.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            children: container.children[0].children[0].children[0]
                                                .children[1]
                                                .children
                                                .clone(),
                                            calculated_width: Some(56.0),
                                            calculated_height: Some(14.0),
                                            ..container.children[0].children[0].children[0].children
                                                [1]
                                            .clone()
                                        },
                                    ],
                                    calculated_width: Some(112.0),
                                    calculated_height: Some(14.0),
                                    ..container.children[0].children[0].children[0].clone()
                                },
                                Container {
                                    children: vec![
                                        Container {
                                            children: container.children[0].children[0].children[1]
                                                .children[0]
                                                .children
                                                .clone(),
                                            calculated_width: Some(56.0),
                                            calculated_height: Some(14.0),
                                            ..container.children[0].children[0].children[1].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            children: container.children[0].children[0].children[1]
                                                .children[1]
                                                .children
                                                .clone(),
                                            calculated_width: Some(56.0),
                                            calculated_height: Some(14.0),
                                            ..container.children[0].children[0].children[1].children
                                                [1]
                                            .clone()
                                        },
                                    ],
                                    calculated_width: Some(112.0),
                                    calculated_height: Some(14.0),
                                    ..container.children[0].children[0].children[1].clone()
                                },
                            ],
                            // The body follows overflowing 56px columns, not
                            // the table's narrower 100px allocation box.
                            calculated_width: Some(112.0),
                            calculated_height: Some(14.0 + 14.0),
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        fn calc_calculates_table_td_height_correctly() {
            let mut container: Container = html! {
                table {
                    tr {
                        td sx-height=(30) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            let td = container.children[0].children[0].children[0].clone();

            compare_containers(
                &td.clone(),
                &Container {
                    calculated_height: Some(30.0),
                    ..td
                },
            );
        }

        #[test_log::test]
        fn calc_calculates_table_tr_height_correctly() {
            let mut container: Container = html! {
                table {
                    tr {
                        td sx-height=(30) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            let tr = container.children[0].children[0].clone();

            compare_containers(
                &tr.clone(),
                &Container {
                    calculated_height: Some(30.0),
                    ..tr
                },
            );
        }

        #[test_log::test]
        fn calc_calculates_table_height_correctly() {
            let mut container: Container = html! {
                table {
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(90.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(100.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn calc_overflow_y_squash_calculates_table_sibling_element_height_correctly() {
            let mut container: Container = html! {
                div {}
                table {
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.overflow_y = LayoutOverflow::Squash;
            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");
            pretty_assertions::assert_eq!(container.children[1].calculated_y, Some(10.0));
            let before = crate::layout::calc::table_layout::geometry(&container);
            CALCULATOR.try_calc(&mut container).unwrap();
            pretty_assertions::assert_eq!(
                before,
                crate::layout::calc::table_layout::geometry(&container)
            );

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_height: Some(10.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_height: Some(90.0),
                            ..container.children[1].clone()
                        },
                    ],
                    calculated_height: Some(100.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn calc_overflow_y_expand_calculates_table_sibling_element_height_correctly() {
            let mut container: Container = html! {
                div {}
                table {
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                    tr sx-height=(30) {
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                        td sx-height=(30) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");
            pretty_assertions::assert_eq!(container.children[1].calculated_y, Some(10.0));
            let before = crate::layout::calc::table_layout::geometry(&container);
            CALCULATOR.try_calc(&mut container).unwrap();
            pretty_assertions::assert_eq!(
                before,
                crate::layout::calc::table_layout::geometry(&container)
            );

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_height: Some(10.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_height: Some(90.0),
                            ..container.children[1].clone()
                        },
                    ],
                    calculated_height: Some(100.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn calc_calculates_table_column_widths_the_same_across_headers_and_body() {
            // P4 policy correction: Intrinsic outer tracks 34/300 receive equal 449px surplus; body padding remains inside tracks.
            let mut container: Container = html! {
                table {
                    thead {
                      tr {
                        th { "#" }
                        th { "Time" }
                      }
                    }
                    tbody id="album-page-tracks" {
                        tr sx-border-radius=(5) {
                            td sx-padding-x=(10) sx-padding-y=(15) {
                                span class="track-number" { "1" }
                                button class="play-button" sx-visibility="hidden" {
                                    img sx-width=(12) sx-height=(12);
                                }
                            }
                            td sx-padding-x=(10) sx-padding-y=(15) {
                                "Even Still I Want To"
                            }
                        }
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(1232.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![Container {
                                    children: vec![
                                        Container {
                                            calculated_width: Some(483.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            calculated_width: Some(749.0),
                                            ..container.children[0].children[0].children[0].children
                                                [1]
                                            .clone()
                                        },
                                    ],
                                    ..container.children[0].children[0].children[0].clone()
                                }],
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![Container {
                                    children: vec![
                                        Container {
                                            calculated_width: Some(463.0),
                                            ..container.children[0].children[1].children[0].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            calculated_width: Some(729.0),
                                            ..container.children[0].children[1].children[0].children
                                                [1]
                                            .clone()
                                        },
                                    ],
                                    ..container.children[0].children[1].children[0].clone()
                                }],
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        fn calc_calculates_table_th_sizes_with_padding_taken_into_account() {
            // P4 policy correction: Padding contributes 30px outer height, not fictitious 25px content.
            let mut container: Container = html! {
                table {
                    thead {
                        tr {
                            th sx-padding-x=(10) sx-padding-y=(15) {}
                        }
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(1232.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                children: vec![Container {
                                    calculated_width: Some(1212.0),
                                    calculated_height: Some(0.0),
                                    ..container.children[0].children[0].children[0].children[0]
                                        .clone()
                                }],
                                ..container.children[0].children[0].children[0].clone()
                            }],
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn calc_calculates_table_td_sizes_with_padding_taken_into_account() {
            // P4 policy correction: Padding contributes 30px outer height, not fictitious 25px content.
            let mut container: Container = html! {
                table {
                    tr {
                        td sx-padding-x=(10) sx-padding-y=(15) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(1232.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                calculated_width: Some(1212.0),
                                calculated_height: Some(0.0),
                                ..container.children[0].children[0].children[0].clone()
                            }],
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }
    }

    #[test_log::test]
    fn calc_can_calc_single_element_size() {
        let mut container = Container {
            children: vec![Container::default()],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(100.0),
                    calculated_height: Some(0.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Default),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_two_elements_with_size_split_evenly_row() {
        let mut container: Container = html! {
            div sx-dir=(LayoutDirection::Row) {
                div {} div {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(0.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_width: Some(0.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    calculated_width: Some(100.0),
                    calculated_height: Some(0.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Default),
                    direction: LayoutDirection::Row,
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_above_a_vertial_split() {
        let mut container: Container = html! {
            div sx-dir=(LayoutDirection::Row) sx-justify-content=(JustifyContent::Start) {
                div {} div {}
            }
            div {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        children: vec![
                            Container {
                                calculated_height: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_height: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(0.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(0.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[1].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_calcs_contained_height_correctly() {
        let mut container: Container = html! {
            div {}
            div sx-dir=(LayoutDirection::Row) {
                div {}
                div {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);
        container.direction = LayoutDirection::Row;
        container.overflow_x = LayoutOverflow::Squash;
        container.overflow_y = LayoutOverflow::Squash;

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![
                            Container {
                                calculated_height: Some(40.0),
                                ..container.children[1].children[0].clone()
                            },
                            Container {
                                calculated_height: Some(40.0),
                                ..container.children[1].children[1].clone()
                            },
                        ],
                        calculated_height: Some(40.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_between_and_wraps_elements_properly() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            justify_content: Some(JustifyContent::SpaceBetween),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_between_and_wraps_elements_properly_with_hidden_div() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..Default::default()
                },
                Container {
                    hidden: Some(true),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            justify_content: Some(JustifyContent::SpaceBetween),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(20.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        hidden: Some(true),
                        ..container.children[5].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn handle_overflow_y_squash_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        let row_height = 40.0 / 4.0;

        let div = Container {
            width: Some(Number::Integer(20)),
            calculated_width: Some(20.0),
            calculated_height: Some(20.0),
            ..Default::default()
        };

        let mut container = Container {
            children: vec![
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::SpaceBetween),
            ..Default::default()
        };

        log::debug!("First calc");
        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 3.0),
                        ..container.children[9].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn handle_overflow_y_expand_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        let row_height = 10.0;

        let div = Container {
            width: Some(Number::Integer(20)),
            ..Default::default()
        };

        let mut container = Container {
            children: vec![
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceBetween),
            ..Default::default()
        };

        log::debug!("First calc");
        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 3.0),
                        ..container.children[9].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_between_with_gap_and_wraps_elements_properly() {
        let row_height = 40.0 / 3.0;

        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceBetween),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(75.0 - 20.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height.mul_add(1.0, 10.0)),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(75.0 - 20.0),
                        calculated_y: Some(row_height.mul_add(1.0, 10.0)),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height.mul_add(2.0, 10.0 + 10.0)),
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_between_with_gap_and_wraps_elements_properly_and_can_recalc() {
        let row_height = 40.0 / 3.0;

        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceBetween),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        let mut actual = container.clone();
        let expected = Container {
            children: vec![
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..container.children[0].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(75.0 - 20.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..container.children[1].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(0.0),
                    calculated_y: Some(row_height.mul_add(1.0, 10.0)),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..container.children[2].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(75.0 - 20.0),
                    calculated_y: Some(row_height.mul_add(1.0, 10.0)),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..container.children[3].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(0.0),
                    calculated_y: Some(row_height.mul_add(2.0, 10.0 + 10.0)),
                    calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                    ..container.children[4].clone()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            ..container
        };

        compare_containers(&actual, &expected);

        CALCULATOR.calc(&mut actual);
        log::trace!("second container:\n{actual}");

        compare_containers(&actual, &expected);
    }

    #[test_log::test]
    fn handles_justify_content_space_evenly_and_wraps_elements_properly() {
        let row_height = 40.0 / 2.0;

        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 1.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 1.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_with_padding_and_wraps_elements_properly()
     {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    height: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    height: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    height: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    height: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    height: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            calculated_padding_left: Some(20.0),
            calculated_padding_right: Some(20.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[4].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_with_padding_and_wraps_elements_properly()
     {
        let mut container: Container = html! {
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-width=(20) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(75.0);
        container.calculated_height = Some(40.0);
        container.calculated_padding_left = Some(20.0);
        container.calculated_padding_right = Some(20.0);
        container.direction = LayoutDirection::Row;
        container.overflow_x = LayoutOverflow::Wrap { grid: true };
        container.overflow_y = LayoutOverflow::Expand;
        container.justify_content = Some(JustifyContent::SpaceEvenly);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[4].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_evenly_and_wraps_elements_properly_with_hidden_div() {
        let mut container: Container = html! {
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-width=(20) {}
            div sx-hidden=(true) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(75.0);
        container.calculated_height = Some(40.0);
        container.direction = LayoutDirection::Row;
        container.overflow_x = LayoutOverflow::Wrap { grid: true };
        container.justify_content = Some(JustifyContent::SpaceEvenly);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[4].clone()
                    },
                    Container {
                        hidden: Some(true),
                        ..container.children[5].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        let row_height = 40.0 / 4.0;

        let div = Container {
            width: Some(Number::Integer(20)),
            ..Default::default()
        };

        let mut container = Container {
            children: vec![
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };

        log::debug!("First calc");
        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 3.0),
                        ..container.children[9].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        let row_height = 40.0 / 4.0;

        let div = Container {
            width: Some(Number::Integer(20)),
            ..Default::default()
        };

        let mut container = Container {
            children: vec![
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
                div.clone(),
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };

        log::debug!("First calc");
        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(row_height * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(3.75),
                        calculated_y: Some(row_height * 3.0),
                        ..container.children[9].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_evenly_with_gap_and_wraps_elements_properly() {
        let row_height = 40.0 / 3.0;

        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceEvenly),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(43.333_336),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(row_height + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(43.333_336),
                        calculated_y: Some(row_height + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(row_height.mul_add(2.0, 10.0 + 10.0)),
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handles_justify_content_space_evenly_with_gap_and_wraps_elements_properly_and_can_recalc() {
        let row_height = 40.0 / 3.0;

        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceEvenly),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        let mut actual = container.clone();
        let expected = Container {
            children: vec![
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..container.children[0].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(43.333_336),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..container.children[1].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(row_height + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..container.children[2].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(43.333_336),
                    calculated_y: Some(row_height + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..container.children[3].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(row_height),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(row_height.mul_add(2.0, 10.0 + 10.0)),
                    calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                    ..container.children[4].clone()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            ..container
        };

        compare_containers(&actual, &expected);

        CALCULATOR.calc(&mut actual);
        log::trace!("second container:\n{actual}");

        compare_containers(&actual, &expected);
    }

    #[test_log::test]
    fn calc_child_minimum_height_is_propagated_upward() {
        let mut container = Container {
            children: vec![Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        height: Some(Number::Integer(40)),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        height: Some(Number::Integer(40)),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        height: Some(Number::Integer(40)),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_child_min_width: Some(25.0),
                    calculated_width: Some(25.0),
                    calculated_height: Some(120.0),
                    ..container.children[0].clone()
                }],
                calculated_width: Some(50.0),
                calculated_height: Some(40.0),
                direction: LayoutDirection::Row,
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_child_minimum_height_is_propagated_upward_and_recalc() {
        let mut container = Container {
            children: vec![Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        height: Some(Number::Integer(40)),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        height: Some(Number::Integer(40)),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        height: Some(Number::Integer(40)),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            }],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("first container:\n{container}");

        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_child_min_width: Some(25.0),
                    calculated_width: Some(25.0),
                    calculated_height: Some(120.0),
                    ..container.children[0].clone()
                }],
                calculated_width: Some(50.0),
                calculated_height: Some(40.0),
                direction: LayoutDirection::Row,
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_resizes_when_a_new_row_was_shifted_into_view() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[2].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_allows_expanding_height_for_overflow_y_scroll() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0 + f32::from(get_scrollbar_size())),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[2].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handle_overflow_wraps_single_row_overflow_content_correctly() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handle_overflow_wraps_multi_row_overflow_content_correctly() {
        let row_height = 40.0;

        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(25.0),
                        calculated_y: Some(row_height),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(row_height),
                        calculated_x: Some(0.0),
                        calculated_y: Some(row_height * 2.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        ..container.children[4].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn handle_overflow_wraps_row_content_correctly_in_overflow_y_scroll() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    height: Some(Number::Integer(40)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0 + f32::from(get_scrollbar_size())),
            calculated_height: Some(80.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_inner_wraps_row_content_correctly() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn calc_inner_children_overflow_squash_wraps_row_content_with_nested_width_correctly() {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[2].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn calc_inner_children_overflow_expand_wraps_row_content_with_nested_width_correctly() {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[2].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn calc_inner_children_overflow_squash_wraps_row_content_with_nested_explicit_width_correctly()
    {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[2].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[allow(clippy::too_many_lines)]
    fn calc_inner_children_overflow_expand_wraps_row_content_with_nested_explicit_width_correctly()
    {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[2].children[0].clone()
                        }],
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_with_row_content_in_right_pane_above_a_vertial_split() {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![
                        Container::default(),
                        Container {
                            children: vec![Container::default(), Container::default()],
                            direction: LayoutDirection::Row,
                            justify_content: Some(JustifyContent::Start),
                            ..Default::default()
                        },
                    ],
                    direction: LayoutDirection::Row,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container::default(),
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(0.0),
                                calculated_height: Some(0.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(0.0),
                                calculated_height: Some(0.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                direction: LayoutDirection::Row,
                                children: vec![
                                    Container {
                                        calculated_width: Some(0.0),
                                        calculated_height: Some(0.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        calculated_width: Some(0.0),
                                        calculated_height: Some(0.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(0.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(0.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[1].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_with_row_content_in_right_pane_above_a_vertial_split_with_a_specified_height()
     {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![
                        Container::default(),
                        Container {
                            children: vec![Container::default(), Container::default()],
                            direction: LayoutDirection::Row,
                            justify_content: Some(JustifyContent::Start),
                            ..Default::default()
                        },
                    ],
                    direction: LayoutDirection::Row,
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                },
                Container {
                    height: Some(Number::Integer(10)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(0.0),
                                calculated_height: Some(0.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(0.0),
                                calculated_height: Some(0.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                direction: LayoutDirection::Row,
                                children: vec![
                                    Container {
                                        calculated_width: Some(0.0),
                                        calculated_height: Some(0.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        calculated_width: Some(0.0),
                                        calculated_height: Some(0.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(0.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..container.children[0].clone()
                    },
                    Container {
                        height: Some(Number::Integer(10)),
                        calculated_width: Some(100.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_absolute_positioned_element_on_top_of_a_relative_element() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    position: Some(Position::Absolute),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            position: Some(Position::Relative),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_x: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        position: Some(Position::Absolute),
                        ..container.children[1].clone()
                    },
                ],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_absolute_positioned_element_nested_on_top_of_a_relative_element_with_left_offset()
     {
        let mut container = Container {
            children: vec![Container {
                children: vec![
                    Container::default(),
                    Container {
                        left: Some(Number::Integer(30)),
                        position: Some(Position::Absolute),
                        ..Default::default()
                    },
                ],
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            position: Some(Position::Relative),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_width: Some(100.0),
                            calculated_height: Some(50.0),
                            calculated_x: Some(30.0),
                            calculated_y: Some(0.0),
                            position: Some(Position::Absolute),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_absolute_positioned_element_on_top_of_a_relative_element_with_left_offset() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    left: Some(Number::Integer(30)),
                    position: Some(Position::Absolute),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            position: Some(Position::Relative),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_x: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(30.0),
                        calculated_y: Some(0.0),
                        position: Some(Position::Absolute),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_absolute_positioned_element_with_explicit_sizes() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    width: Some(Number::Integer(30)),
                    height: Some(Number::Integer(20)),
                    left: Some(Number::Integer(30)),
                    position: Some(Position::Absolute),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            position: Some(Position::Relative),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_x: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(30.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(30.0),
                        calculated_y: Some(0.0),
                        position: Some(Position::Absolute),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_justify_content_center_horizontally() {
        let mut container = Container {
            children: vec![Container {
                width: Some(Number::Integer(30)),
                ..Default::default()
            }],

            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            justify_content: Some(JustifyContent::Center),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_x: Some((100.0 - 30.0) / 2.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_justify_content_start() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(30)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(30)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_x: Some(30.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_includes_horizontal_margins_in_content_width() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(30)),
                    margin_left: Some(Number::Integer(35)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(30.0),
                        calculated_margin_left: Some(35.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_x: Some(65.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_includes_horizontal_padding_in_content_width() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(30)),
                    padding_right: Some(Number::Integer(35)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(30.0),
                        calculated_padding_right: Some(35.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_x: Some(65.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_includes_horizontal_padding_in_auto_calculated_content_width() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    padding_right: Some(Number::Integer(30)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_padding_right: Some(30.0),
                        calculated_x: Some(0.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_includes_horizontal_margin_in_auto_calculated_content_width() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    margin_right: Some(Number::Integer(30)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_margin_right: Some(30.0),
                        calculated_x: Some(0.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_sized_widths_based_on_the_container_width_minus_all_its_childrens_padding() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::IntegerPercent(50)),
                    padding_right: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::IntegerPercent(50)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(40.0),
                        calculated_padding_right: Some(20.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(40.0),
                        calculated_x: Some(60.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_unsized_widths_based_on_the_container_width_minus_all_its_childrens_padding()
    {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::IntegerPercent(50)),
                    padding_right: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container::default(),
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(40.0),
                        calculated_padding_right: Some(20.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(60.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_unsized_widths_based_on_the_container_width_minus_second_childs_padding() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::IntegerPercent(50)),
                    ..Default::default()
                },
                Container {
                    padding_right: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(40.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_padding_right: Some(20.0),
                        calculated_x: Some(40.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_horizontal_padding_on_vertical_sibling_doesnt_affect_size_of_other_sibling() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    padding_right: Some(Number::Integer(20)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(80.0),
                        calculated_padding_right: Some(20.0),
                        calculated_x: Some(0.0),
                        ..container.children[1].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_child_padding_does_not_add_to_parent_container() {
        let mut container = Container {
            children: vec![
                Container {
                    padding_right: Some(Number::Integer(20)),
                    ..Default::default()
                },
                Container::default(),
                Container::default(),
            ],
            calculated_width: Some(110.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(0.0),
                        calculated_padding_right: Some(20.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(20.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(20.0),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_nested_child_padding_does_not_offset_unsized_container_siblings() {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![Container {
                        padding_right: Some(Number::Integer(20)),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                Container::default(),
                Container::default(),
            ],
            calculated_width: Some(90.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            calculated_width: Some(20.0),
                            calculated_padding_right: Some(20.0),
                            calculated_x: Some(0.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(20.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(20.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(20.0),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_horizontal_sibling_left_raw_still_divides_the_unsized_width() {
        let metrics = DefaultFontMetrics;
        let text_width = metrics
            .measure_text(&crate::layout::font::TextMeasureRequest {
                text: "test",
                family: crate::layout::font::TextFontFamily::CompatibilityDefault,
                weight: crate::layout::font::TextFontWeight::CompatibilityDefault,
                size: 14.0,
                wrap_width: f32::INFINITY,
                whitespace: None,
            })
            .unwrap()
            .width();

        let mut container = Container {
            children: vec![
                Container {
                    element: Element::Raw {
                        value: "test".to_string(),
                    },
                    ..Container::default()
                },
                Container::default(),
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    container.children[0].clone(),
                    Container {
                        calculated_width: Some(0.0),
                        calculated_x: Some(text_width),
                        ..container.children[1].clone()
                    },
                ],
                calculated_width: Some(100.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_width_minus_the_horizontal_padding() {
        let mut container = Container {
            children: vec![Container {
                padding_left: Some(Number::Integer(10)),
                padding_right: Some(Number::Integer(20)),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(70.0),
                    calculated_x: Some(0.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_height_minus_the_vertical_padding() {
        let mut container = Container {
            children: vec![Container {
                padding_top: Some(Number::Integer(10)),
                padding_bottom: Some(Number::Integer(20)),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_height: Some(0.0),
                    calculated_y: Some(0.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_width_minus_the_horizontal_padding_with_percentage_width() {
        let mut container = Container {
            children: vec![Container {
                width: Some(Number::IntegerPercent(50)),
                padding_left: Some(Number::Integer(10)),
                padding_right: Some(Number::Integer(20)),
                padding_top: Some(Number::Integer(15)),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(35.0),
                    calculated_x: Some(0.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_width_minus_the_horizontal_padding_with_percentage_width_nested() {
        let mut container = Container {
            children: vec![Container {
                children: vec![Container {
                    width: Some(Number::IntegerPercent(50)),
                    padding_left: Some(Number::Integer(2)),
                    padding_right: Some(Number::Integer(3)),
                    padding_top: Some(Number::Integer(1)),
                    ..Default::default()
                }],
                width: Some(Number::IntegerPercent(100)),
                padding_left: Some(Number::Integer(10)),
                padding_right: Some(Number::Integer(20)),
                padding_top: Some(Number::Integer(15)),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        calculated_width: Some(32.5),
                        calculated_x: Some(0.0),
                        calculated_padding_left: Some(2.0),
                        calculated_padding_right: Some(3.0),
                        calculated_padding_top: Some(1.0),
                        ..container.children[0].children[0].clone()
                    }],
                    calculated_width: Some(70.0),
                    calculated_x: Some(0.0),
                    calculated_padding_left: Some(10.0),
                    calculated_padding_right: Some(20.0),
                    calculated_padding_top: Some(15.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_width_minus_the_horizontal_padding_with_calc_width_nested() {
        let mut container = Container {
            children: vec![Container {
                children: vec![Container {
                    width: Some(Number::IntegerPercent(50)),
                    padding_left: Some(Number::Integer(2)),
                    padding_right: Some(Number::Integer(3)),
                    padding_top: Some(Number::Integer(1)),
                    ..Default::default()
                }],
                width: Some(Number::Calc(Calculation::Number(Box::new(
                    Number::IntegerPercent(100),
                )))),
                padding_left: Some(Number::Integer(10)),
                padding_right: Some(Number::Integer(20)),
                padding_top: Some(Number::Integer(15)),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        calculated_width: Some(32.5),
                        calculated_x: Some(0.0),
                        calculated_padding_left: Some(2.0),
                        calculated_padding_right: Some(3.0),
                        calculated_padding_top: Some(1.0),
                        ..container.children[0].children[0].clone()
                    }],
                    calculated_width: Some(70.0),
                    calculated_x: Some(0.0),
                    calculated_padding_left: Some(10.0),
                    calculated_padding_right: Some(20.0),
                    calculated_padding_top: Some(15.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_width_minus_the_horizontal_padding_for_absolute_position_children() {
        let mut container = Container {
            children: vec![Container {
                width: Some(Number::Calc(Calculation::Number(Box::new(
                    Number::IntegerPercent(100),
                )))),
                padding_left: Some(Number::Integer(10)),
                padding_right: Some(Number::Integer(20)),
                padding_top: Some(Number::Integer(15)),
                position: Some(Position::Absolute),
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            position: Some(Position::Relative),
            justify_content: Some(JustifyContent::Start),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(70.0),
                    calculated_height: Some(35.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_padding_left: Some(10.0),
                    calculated_padding_right: Some(20.0),
                    calculated_padding_top: Some(15.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_uses_bounding_width_for_absolute_position_children_with_right_offset() {
        let mut container = Container {
            children: vec![Container {
                width: Some(Number::Calc(Calculation::Number(Box::new(
                    Number::IntegerPercent(50),
                )))),
                padding_left: Some(Number::Integer(10)),
                padding_right: Some(Number::Integer(20)),
                right: Some(Number::Integer(5)),
                position: Some(Position::Absolute),
                ..Default::default()
            }],

            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            position: Some(Position::Relative),
            ..Default::default()
        };
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(35.0),
                    calculated_height: Some(50.0),
                    calculated_x: Some(100.0 - 35.0 - 10.0 - 20.0 - 5.0),
                    calculated_y: Some(0.0),
                    calculated_padding_left: Some(10.0),
                    calculated_padding_right: Some(20.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_flex_height_for_single_unsized_child_with_sized_child() {
        let mut container: Container = html! {
            div {
                div sx-dir="row" {
                    div sx-width=(50) sx-height=(36) { "Albums" }
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(160.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(50.0),
                            calculated_height: Some(36.0),
                            ..container.children[0].children[0].children[0].clone()
                        }],
                        ..container.children[0].children[0].clone()
                    }],
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_flex_height_for_two_unsized_children_with_sized_child() {
        let mut container: Container = html! {
            div {
                div sx-dir="row" {
                    div sx-width=(50) sx-height=(36) { "Albums" }
                }
                div sx-dir="row" {
                    div sx-width=(50) sx-height=(36) { "Albums2" }
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(160.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            children: vec![Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(36.0),
                                ..container.children[0].children[0].children[0].clone()
                            }],
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            children: vec![Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(36.0),
                                ..container.children[0].children[1].children[0].clone()
                            }],
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_width_minus_the_horizontal_padding_for_nested_children_with_calc_parent_sizes()
     {
        let mut container: Container = html! {
            div sx-width="100%" sx-height="100%" sx-position="relative" {
                section sx-dir="row" sx-height=("calc(100% - 140px)") {
                    aside sx-width="calc(max(240, min(280, 15%)))" {}
                    main sx-overflow-y="auto" sx-flex=(1) {
                        div sx-height=(76) sx-justify-content=(JustifyContent::Start) {
                            div
                                sx-padding-left=(30)
                                sx-padding-right=(30)
                                sx-padding-top=(15)
                            {
                                div sx-dir="row" {
                                    h1 sx-width=(50) sx-height=(36) { "Albums" }
                                }
                            }
                        }
                    }
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        let container = container.children[0].children[0].children[1].clone();

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                element: Element::Heading {
                                    size: HeaderSize::H1,
                                },
                                calculated_width: Some(50.0),
                                calculated_height: Some(36.0),
                                ..container.children[0].children[0].children[0].children[0].clone()
                            }],
                            ..container.children[0].children[0].children[0].clone()
                        }],
                        calculated_width: Some(1300.0),
                        calculated_height: Some(36.0),
                        calculated_padding_left: Some(30.0),
                        calculated_padding_right: Some(30.0),
                        calculated_padding_top: Some(15.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        ..container.children[0].children[0].clone()
                    }],
                    calculated_width: Some(1360.0),
                    calculated_height: Some(76.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_horizontal_position_from_right_for_absolute_position_with_padding() {
        let mut container: Container = html! {
            div
                sx-width="calc(min(500, 30%))"
                sx-height="calc(100% - 200)"
                sx-padding-left=(20)
                sx-padding-right=(20)
                sx-position="absolute"
                sx-right=(0)
            {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.position = Some(Position::Relative);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some((1600.0 - 40.0) * 0.3),
                    calculated_height: Some(800.0),
                    calculated_x: Some(1092.0),
                    calculated_y: Some(0.0),
                    calculated_padding_left: Some(20.0),
                    calculated_padding_right: Some(20.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_vertical_position_from_right_for_absolute_position_with_padding() {
        let mut container: Container = html! {
            div
                sx-width="calc(min(500, 30%))"
                sx-height="calc(100% - 200)"
                sx-padding-top=(20)
                sx-padding-bottom=(20)
                sx-position="absolute"
                sx-bottom=(170)
            {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.position = Some(Position::Relative);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(1600.0 * 0.3),
                    calculated_height: Some(1000.0 - 200.0 - 40.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(30.0),
                    calculated_padding_top: Some(20.0),
                    calculated_padding_bottom: Some(20.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_horizontal_and_vertical_position_from_right_for_absolute_position_with_padding()
     {
        let mut container: Container = html! {
            div
                sx-width="calc(min(500, 30%))"
                sx-height="calc(100% - 200)"
                sx-padding=(20)
                sx-position="absolute"
                sx-bottom=(170)
                sx-right=(0)
            {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.position = Some(Position::Relative);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some((1600.0 - 40.0) * 0.3),
                    calculated_height: Some(1000.0 - 200.0 - 40.0),
                    calculated_x: Some(1092.0),
                    calculated_y: Some(30.0),
                    calculated_padding_left: Some(20.0),
                    calculated_padding_right: Some(20.0),
                    calculated_padding_top: Some(20.0),
                    calculated_padding_bottom: Some(20.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_horizontal_and_vertical_position_from_right_for_nested_absolute_position_with_padding()
     {
        let mut container: Container = html! {
            div sx-width="100%" sx-height="100%" sx-position="relative" {
                section sx-dir="row" sx-height=("calc(100% - 140px)") {
                    aside sx-width="calc(max(240, min(280, 15%)))" {}
                    main sx-overflow-y="auto" {}
                }
                div
                    sx-width="calc(min(500, 30%))"
                    sx-height="calc(100% - 200)"
                    sx-padding=(20)
                    sx-position="absolute"
                    sx-bottom=(170)
                    sx-right=(0)
                {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        let container = container.children[0].children[1].clone();

        compare_containers(
            &container.clone(),
            &Container {
                calculated_width: Some((1600.0 - 40.0) * 0.3),
                calculated_height: Some(1000.0 - 200.0 - 40.0),
                calculated_x: Some(1092.0),
                calculated_y: Some(30.0),
                calculated_padding_left: Some(20.0),
                calculated_padding_right: Some(20.0),
                calculated_padding_top: Some(20.0),
                calculated_padding_bottom: Some(20.0),
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_horizontal_padding_on_sized_element_correctly() {
        let mut container: Container = html! {
            div sx-width="100%" sx-height="100%" sx-position="relative" {
                section sx-dir="row" sx-height=("calc(100% - 140)") {
                    aside sx-width="calc(max(240, min(280, 15%)))" sx-padding=(20) {}
                    main sx-overflow-y="auto" sx-flex=(1) {}
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        let main = container.children[0].children[0].children[1].clone();

        compare_containers(
            &main.clone(),
            &Container {
                calculated_width: Some(1320.0),
                calculated_height: Some(860.0),
                calculated_x: Some(280.0),
                calculated_y: Some(0.0),
                ..main
            },
        );

        let aside = container.children[0].children[0].children[0].clone();

        compare_containers(
            &aside.clone(),
            &Container {
                calculated_width: Some(240.0),
                calculated_x: Some(0.0),
                calculated_y: Some(0.0),
                calculated_padding_left: Some(20.0),
                calculated_padding_right: Some(20.0),
                ..aside
            },
        );
    }

    #[test_log::test]
    fn calc_calculates_vertical_padding_on_sized_element_correctly() {
        let mut container: Container = html! {
            div sx-width="100%" sx-height="100%" sx-position="relative" {
                section sx-dir="row" sx-height=("calc(100% - 140)") {
                    aside
                        sx-justify-content=(JustifyContent::Start)
                        sx-width="calc(max(240, min(280, 15%)))"
                        sx-padding=(20)
                    {
                        div sx-justify-content=(JustifyContent::Start) {
                            div {}
                            ul sx-justify-content=(JustifyContent::Start) { li {} li {} }
                        }
                    }
                    main sx-overflow-y="auto" sx-flex=(1) {}
                }
                footer sx-height=(140) {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        let main = container.children[0].children[0].children[1].clone();

        compare_containers(
            &main.clone(),
            &Container {
                calculated_width: Some(1320.0),
                calculated_height: Some(860.0),
                calculated_x: Some(280.0),
                calculated_y: Some(0.0),
                ..main
            },
        );

        let aside = container.children[0].children[0].children[0].clone();

        compare_containers(
            &aside.clone(),
            &Container {
                calculated_height: Some(820.0),
                calculated_x: Some(0.0),
                calculated_y: Some(0.0),
                calculated_padding_top: Some(20.0),
                calculated_padding_bottom: Some(20.0),
                ..aside
            },
        );
    }

    #[test_log::test]
    fn calc_overflow_y_squash_expands_height_of_largest_child_as_much_as_possible() {
        let mut container: Container = html! {
            div {
                div sx-height=(40) {}
            }
            div {
                div sx-height=(600) {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_height: Some(40.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_height: Some(600.0),
                        ..container.children[1].clone()
                    },
                ],
                calculated_height: Some(500.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_overflow_y_expand_expands_height_when_contained_height_is_greater_than_single_unsized_div()
     {
        let mut container: Container = html! {
            div {
                div {
                    div sx-height=(600) {}
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_height: Some(600.0),
                            ..container.children[0].children[0].children[0].clone()
                        }],
                        calculated_height: Some(600.0),
                        ..container.children[0].children[0].clone()
                    }],
                    calculated_height: Some(600.0),
                    ..container.children[0].clone()
                }],
                calculated_height: Some(500.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_overflow_y_expand_expands_height_when_contained_height_is_greater_than_two_unsized_divs()
     {
        let mut container: Container = html! {
            div {
                div {
                    div sx-height=(40) {}
                }
                div {
                    div sx-height=(600) {}
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_height: Some(40.0),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_height: Some(600.0),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    calculated_height: Some(640.0),
                    ..container.children[0].clone()
                }],
                calculated_height: Some(500.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_overflow_y_auto_justify_content_start_only_takes_up_sized_height() {
        let mut container: Container = html! {
            div sx-overflow-y="auto" sx-justify-content=(JustifyContent::Start) {
                div {
                    div sx-height=(40) {}
                }
                div {
                    div sx-height=(600) {}
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_height: Some(40.0),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_height: Some(600.0),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    calculated_height: Some(500.0),
                    ..container.children[0].clone()
                }],
                calculated_height: Some(500.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_fixed_top_left_border_radius() {
        let mut container: Container = html! {
            div sx-border-top-left-radius=(5) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_top_left_radius: Some(5.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_fixed_top_right_border_radius() {
        let mut container: Container = html! {
            div sx-border-top-right-radius=(5) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_top_right_radius: Some(5.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_fixed_bottom_left_border_radius() {
        let mut container: Container = html! {
            div sx-border-bottom-left-radius=(5) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_bottom_left_radius: Some(5.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_fixed_bottom_right_border_radius() {
        let mut container: Container = html! {
            div sx-border-bottom-right-radius=(5) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_bottom_right_radius: Some(5.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_percentage_top_left_border_radius() {
        let mut container: Container = html! {
            div sx-border-top-left-radius="100%" {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_top_left_radius: Some(100.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_percentage_top_right_border_radius() {
        let mut container: Container = html! {
            div sx-border-top-right-radius="100%" {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_top_right_radius: Some(100.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_percentage_bottom_left_border_radius() {
        let mut container: Container = html! {
            div sx-border-bottom-left-radius="100%" {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_bottom_left_radius: Some(100.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_percentage_bottom_right_border_radius() {
        let mut container: Container = html! {
            div sx-border-bottom-right-radius="100%" {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_border_bottom_right_radius: Some(100.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_fixed_opacity() {
        let mut container: Container = html! {
            div sx-opacity=(0.5) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_opacity: Some(0.5),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_does_calculate_percentage_opacity() {
        let mut container: Container = html! {
            div sx-opacity="100%" {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_opacity: Some(1.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_left_margin_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-left=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_child_min_width: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_right_margin_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-right=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_child_min_width: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_top_margin_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-top=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(0.0),
                    calculated_child_min_height: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_bottom_margin_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-bottom=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(0.0),
                    calculated_child_min_height: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_left_margin_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-left=(50.0) sx-width=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_child_min_width: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_right_margin_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-right=(50.0) sx-width=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_child_min_width: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_top_margin_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-top=(50.0) sx-height=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_child_min_height: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_bottom_margin_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-margin-bottom=(50.0) sx-height=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_child_min_height: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_left_padding_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-left=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_child_min_width: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_right_padding_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-right=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_child_min_width: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_top_padding_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-top=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(0.0),
                    calculated_child_min_height: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_bottom_padding_in_min_size_with_no_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-bottom=(50.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(0.0),
                    calculated_child_min_height: Some(50.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_left_padding_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-left=(50.0) sx-width=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_child_min_width: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_right_padding_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-right=(50.0) sx-width=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_child_min_width: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_top_padding_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-top=(50.0) sx-height=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_child_min_height: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_includes_fixed_bottom_padding_in_min_size_with_explicit_fixed_size() {
        let mut container: Container = html! {
            div sx-padding-bottom=(50.0) sx-height=(25.0) {}
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_child_min_height: Some(75.0),
                    ..container.children[0].clone()
                }],
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_evenly_distributes_row_width_even_when_one_child_has_a_min_width() {
        let mut container: Container = html! {
            div sx-dir="row" {
                div {}
                div {
                    div sx-width=(50) {}
                }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);
        container.justify_content = Some(JustifyContent::Start);
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(0.0),
                            calculated_height: Some(0.0),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_width: Some(50.0),
                            calculated_height: Some(0.0),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    calculated_width: Some(400.0),
                    calculated_height: Some(0.0),
                    ..container.children[0].clone()
                }],
                calculated_width: Some(400.0),
                calculated_height: Some(100.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_only_takes_height_necessary_for_non_flex_container_contents() {
        let mut container: Container = html! {
            div {
                div sx-height=(10) {}
            }
            div {
                div sx-height=(15) {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(400.0),
                        calculated_height: Some(10.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(400.0),
                        calculated_height: Some(15.0),
                        ..container.children[1].clone()
                    },
                ],
                calculated_width: Some(400.0),
                calculated_height: Some(100.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_only_takes_height_necessary_for_flex_cross_axis_column_container_contents_when_align_items_is_set()
     {
        let mut container: Container = html! {
            div sx-dir=(LayoutDirection::Row) sx-align-items=(AlignItems::Center) sx-height=(50) {
                div { div { "one" } div { "two" } }
                div { "three" }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("full container:\n{container}");
        container = container.children[0].clone();
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_height: Some(14.0 * 2.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_height: Some(14.0),
                        ..container.children[1].clone()
                    },
                ],
                calculated_height: Some(50.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_only_takes_height_necessary_for_flex_cross_axis_row_container_contents_when_align_items_is_set()
     {
        let mut container: Container = html! {
            div sx-align-items=(AlignItems::Center) sx-width=(150) {
                div sx-dir=(LayoutDirection::Row) { div { "one" } div { "two" } }
                div sx-dir=(LayoutDirection::Row) { "three" }
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("full container:\n{container}");
        container = container.children[0].clone();
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(14.0 * 6.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(14.0 * 5.0),
                        ..container.children[1].clone()
                    },
                ],
                calculated_width: Some(150.0),
                ..container.clone()
            },
        );
    }

    #[test_log::test]
    fn calc_direction_row_doesnt_take_full_width() {
        let mut container: Container = html! {
            div sx-width=(200) sx-dir=(LayoutDirection::Row) {
                h1 { "test" }
                div sx-width=(10) {}
            }
        }
        .into_string()
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("full container:\n{container}");
        container = container.children[0].clone();
        log::trace!("container:\n{container}");

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(32.0 * 4.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(10.0),
                        ..container.children[1].clone()
                    },
                ],
                calculated_width: Some(200.0),
                ..container.clone()
            },
        );
    }

    mod text {
        use super::*;

        #[test_log::test]
        fn does_calculate_text_height_properly() {
            let mut container: Container = html! {
                div { "test" }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(14.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_propagate_text_height_properly() {
            let mut container: Container = html! {
                div { "test" }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(14.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(14.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_preferred_width_for_nested_text() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-align-items=(AlignItems::Start) {
                    "test" span { "two" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_width: Some(14.0 * 4.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_width: Some(14.0 * 3.0),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_preferred_width_for_nested_text_in_dynamically_sized_child() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-align-items=(AlignItems::Center) {
                    div sx-justify-content=(JustifyContent::Center) {
                        div sx-dir=(LayoutDirection::Row) {
                            "test" span { "two" }
                        }
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![
                                Container {
                                    calculated_width: Some(14.0 * 4.0),
                                    ..container.children[0].children[0].children[0].clone()
                                },
                                Container {
                                    calculated_width: Some(14.0 * 3.0),
                                    ..container.children[0].children[0].children[1].clone()
                                },
                            ],
                            calculated_width: Some(14.0 * (4.0 + 3.0)),
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_not_expand_past_preferred_width() {
            let mut container: Container = html! {
                div sx-width=(100) { "test" }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(14.0 * 4.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(100.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_wrap_long_line_of_text_to_2_lines() {
            let mut container: Container = html! {
                div sx-width=(100) { "test test" }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(14.0 * 2.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(100.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_wrap_nested_text_longer_than_container_can_fit() {
            let mut container: Container = html! {
                div sx-width=(200) {
                    span { "aoeu aoeu aoeu aoeu aoeu" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        calculated_height: Some(14.0 * 2.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(200.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_wrap_nested_text_longer_than_container_can_fit_with_align_items_center() {
            let mut container: Container = html! {
                div sx-width=(200) sx-align-items=(AlignItems::Center) {
                    span { "aoeu aoeu aoeu aoeu aoeu" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        calculated_height: Some(14.0 * 2.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(200.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_wrap_text_longer_than_nested_div_container_can_fit_with_align_items_center() {
            let mut container: Container = html! {
                div sx-width=(200) sx-align-items=(AlignItems::Center) {
                    div { "aoeu aoeu aoeu aoeu aoeu" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        calculated_height: Some(14.0 * 2.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(200.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_default_font_size() {
            let mut container: Container = html! {
                "aoeu"
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_font_size = Some(20.0);
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(20.0 * 4.0),
                        calculated_height: Some(20.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_direct_parent_fixed_font_size() {
            let mut container: Container = html! {
                div sx-font-size=(10) {
                    "aoeu"
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(10.0 * 4.0),
                            calculated_height: Some(10.0),
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_direct_parent_dynamic_font_size() {
            let mut container: Container = html! {
                div sx-font-size="80%" {
                    "aoeu"
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(16.0 * 4.0),
                            calculated_height: Some(16.0),
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_ascestor_fixed_font_size() {
            let mut container: Container = html! {
                div sx-font-size=(10) {
                    div { "aoeu" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                calculated_width: Some(10.0 * 4.0),
                                calculated_height: Some(10.0),
                                ..container.children[0].children[0].children[0].clone()
                            }],
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_use_ascestor_dynamic_font_size() {
            let mut container: Container = html! {
                div sx-font-size="80%" {
                    div { "aoeu" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                calculated_width: Some(16.0 * 4.0),
                                calculated_height: Some(16.0),
                                ..container.children[0].children[0].children[0].clone()
                            }],
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn doesnt_use_sibling_fixed_font_size() {
            let mut container: Container = html! {
                div sx-font-size=(10) {}
                div { "aoeu" }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        container.children[0].clone(),
                        Container {
                            children: vec![Container {
                                calculated_width: Some(20.0 * 4.0),
                                calculated_height: Some(20.0),
                                ..container.children[1].children[0].clone()
                            }],
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn doesnt_use_sibling_dynamic_font_size() {
            let mut container: Container = html! {
                div sx-font-size="80%" {}
                div { "aoeu" }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            let calculator = Calculator::new(
                DefaultFontMetrics,
                CalculatorDefaults {
                    font_size: 20.0,
                    ..Default::default()
                },
            );
            calculator.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        container.children[0].clone(),
                        Container {
                            children: vec![Container {
                                calculated_width: Some(20.0 * 4.0),
                                calculated_height: Some(20.0),
                                ..container.children[1].children[0].clone()
                            }],
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        macro_rules! test_heading {
            ($heading:tt, $font_size:ident, $margin_top:ident, $margin_bottom:ident $(,)?) => {
                paste! {
                    #[test_log::test]
                    fn [<does_use_ $heading _font_size>]() {
                        let mut container: Container = html! {
                            $heading {
                                "aoeu"
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    calculated_font_size: Some(30.0),
                                    ..container.children[0].clone()
                                }],
                                calculated_font_size: Some(20.0),
                                ..container.clone()
                            },
                        );
                    }

                    #[test_log::test]
                    fn [<does_use_ $heading _font_size_for_raw_child>]() {
                        let mut container: Container = html! {
                            $heading {
                                "aoeu"
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    children: vec![Container {
                                        calculated_font_size: Some(30.0),
                                        ..container.children[0].children[0].clone()
                                    }],
                                    calculated_font_size: Some(30.0),
                                    ..container.children[0].clone()
                                }],
                                calculated_font_size: Some(20.0),
                                ..container.clone()
                            },
                        );
                    }

                    #[test_log::test]
                    fn [<does_use_ $heading _font_size_for_nested_raw_child>]() {
                        let mut container: Container = html! {
                            $heading {
                                div {
                                    "aoeu"
                                }
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    children: vec![Container {
                                        children: vec![Container {
                                            calculated_font_size: Some(30.0),
                                            ..container.children[0].children[0].children[0].clone()
                                        }],
                                        calculated_font_size: Some(30.0),
                                        ..container.children[0].children[0].clone()
                                    }],
                                    calculated_font_size: Some(30.0),
                                    ..container.children[0].clone()
                                }],
                                calculated_font_size: Some(20.0),
                                ..container.clone()
                            },
                        );
                    }

                    #[test_log::test]
                    fn [<does_use_ $margin_top>]() {
                        let mut container: Container = html! {
                            $heading {
                                "aoeu"
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                $margin_top: 6.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    calculated_margin_top: Some(6.0),
                                    ..container.children[0].clone()
                                }],
                                ..container.clone()
                            },
                        );
                    }
                    #[test_log::test]
                    fn [<does_use_ $margin_bottom>]() {
                        let mut container: Container = html! {
                            $heading {
                                "aoeu"
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                $margin_bottom: 6.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    calculated_margin_bottom: Some(6.0),
                                    ..container.children[0].clone()
                                }],
                                ..container.clone()
                            },
                        );
                    }

                    #[test_log::test]
                    fn [<doesnt_propagate_ $margin_top _to_children>]() {
                        let mut container: Container = html! {
                            $heading {
                                "aoeu"
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                $margin_top: 6.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    children: vec![Container {
                                        calculated_margin_bottom: None,
                                        ..container.children[0].children[0].clone()
                                    }],
                                    calculated_margin_top: Some(6.0),
                                    ..container.children[0].clone()
                                }],
                                ..container.clone()
                            },
                        );
                    }

                    #[test_log::test]
                    fn [<doesnt_propagate_ $margin_bottom _to_children>]() {
                        let mut container: Container = html! {
                            $heading {
                                "aoeu"
                            }
                        }
                        .into_string().try_into()
                        .unwrap();

                        container.calculated_font_size = Some(20.0);
                        container.calculated_width = Some(400.0);
                        container.calculated_height = Some(100.0);

                        let calculator = Calculator::new(
                            DefaultFontMetrics,
                            CalculatorDefaults {
                                font_size: 20.0,
                                $font_size: 30.0,
                                $margin_bottom: 6.0,
                                ..Default::default()
                            },
                        );
                        calculator.calc(&mut container);
                        log::trace!("container:\n{}", container);

                        compare_containers(
                            &container,
                            &Container {
                                children: vec![Container {
                                    children: vec![Container {
                                        calculated_margin_bottom: None,
                                        ..container.children[0].children[0].clone()
                                    }],
                                    calculated_margin_bottom: Some(6.0),
                                    ..container.children[0].clone()
                                }],
                                ..container.clone()
                            },
                        );
                    }
                }
            };
        }

        test_heading!(h1, h1_font_size, h1_font_margin_top, h1_font_margin_bottom);
        test_heading!(h2, h2_font_size, h2_font_margin_top, h2_font_margin_bottom);
        test_heading!(h3, h3_font_size, h3_font_margin_top, h3_font_margin_bottom);
        test_heading!(h4, h4_font_size, h4_font_margin_top, h4_font_margin_bottom);
        test_heading!(h5, h5_font_size, h5_font_margin_top, h5_font_margin_bottom);
        test_heading!(h6, h6_font_size, h6_font_margin_top, h6_font_margin_bottom);
    }

    mod sizing {
        use super::*;

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_min_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width=(100) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_min_width_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width=(100) {
                        div sx-width=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_min_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height=(100) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_min_height_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height=(100) {
                        div sx-height=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_min_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width="50%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_min_width_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width="50%" {
                        div sx-width=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_min_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height="50%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_min_height_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height="50%" {
                        div sx-height=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_max_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) {
                    div sx-max-width=(100) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_max_width_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) {
                    div sx-max-width=(100) {
                        div sx-width=(200) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_max_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-height="100%" {
                    div sx-max-height=(100) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_fixed_max_height_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-height="100%" {
                    div sx-max-height=(100) {
                        div sx-height=(200) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_max_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) {
                    div sx-max-width="50%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_max_width_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) {
                    div sx-max-width="50%" {
                        div sx-width=(300) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_max_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-height="100%" {
                    div sx-max-height="50%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_explicit_dynamic_max_height_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-height="100%" {
                    div sx-max-height="50%" {
                        div sx-height=(300) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_width_and_max_fixed_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width=(100) sx-max-width=(90) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_width_and_max_fixed_width_with_fixed_child_content() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width=(100) sx-max-width=(90) {
                        div sx-width=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_height_and_max_fixed_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height=(100) sx-max-height=(90) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_height_and_max_fixed_height_with_fixed_child_content()
        {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height=(100) sx-max-height=(90) {
                        div sx-height=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_width_and_max_fixed_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width="50%" sx-max-width=(90) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_width_and_max_fixed_width_with_fixed_child_content()
        {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width="50%" sx-max-width=(90) {
                        div sx-width=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_height_and_max_fixed_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height="50%" sx-max-height=(90) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_height_and_max_fixed_height_with_fixed_child_content()
         {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height="50%" sx-max-height=(90) {
                        div sx-height=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_width_and_max_dynamic_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width=(100) sx-max-width="0%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_width_and_max_dynamic_width_with_fixed_child_content()
        {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width=(100) sx-max-width="0%" {
                        div sx-width=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_height_and_max_dynamic_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height=(100) sx-max-height="0%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_fixed_min_height_and_max_dynamic_height_with_fixed_child_content()
         {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height=(100) sx-max-height="0%" {
                        div sx-height=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_width_and_max_dynamic_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width="50%" sx-max-width="0%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_width_and_max_dynamic_width_with_fixed_child_content()
         {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-min-width="50%" sx-max-width="0%" {
                        div sx-width=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_height_and_max_dynamic_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height="50%" sx-max-height="0%" {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_prioritize_explicit_dynamic_min_height_and_max_dynamic_height_with_fixed_child_content()
         {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) sx-height="100%" {
                    div sx-min-height="50%" sx-max-height="0%" {
                        div sx-height=(10) {}
                    }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(200.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_include_margin_top_in_parent_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) {
                    div sx-margin-top=(5) sx-height=(10) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_height: Some(10.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_height: Some(15.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_include_margin_bottom_in_parent_height() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Column) {
                    div sx-margin-bottom=(5) sx-height=(10) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_height: Some(10.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_height: Some(15.0),
                        ..container.children[0].clone()
                    }],
                    calculated_height: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_include_margin_left_in_parent_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-margin-left=(5) sx-width=(10) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(10.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(15.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_include_margin_right_in_parent_width() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) {
                    div sx-margin-right=(5) sx-width=(10) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(10.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(15.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        #[ignore = "Unimplemented"]
        fn flex_child_does_take_full_width_if_flex_is_specified() {
            let mut container: Container = html! {
                div sx-width="100%" sx-dir=(LayoutDirection::Row) {
                    div flex=(1) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(400.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(400.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(400.0),
                    ..container.clone()
                },
            );
        }
    }

    mod position_sticky {
        use super::*;

        #[test_log::test]
        fn does_include_size_in_parent_width() {
            let mut container: Container = html! {
                div {
                    div sx-position=(Position::Sticky) sx-width=(100) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(100.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_include_size_in_parent_height() {
            let mut container: Container = html! {
                div {
                    div sx-position=(Position::Sticky) sx-height=(100) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(200.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_height: Some(100.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }
    }

    mod position_fixed {
        use super::*;

        fn nested_positioned_tree() -> Container {
            let positioned = |position| Container {
                position: Some(position),
                width: Some(Number::IntegerPercent(50)),
                height: Some(Number::IntegerPercent(50)),
                right: Some(Number::IntegerPercent(10)),
                bottom: Some(Number::IntegerPercent(20)),
                ..Container::default()
            };
            Container {
                calculated_width: Some(400.0),
                calculated_height: Some(200.0),
                children: vec![Container {
                    position: Some(Position::Relative),
                    width: Some(Number::Integer(120)),
                    height: Some(Number::Integer(80)),
                    children: vec![Container {
                        position: Some(Position::Relative),
                        width: Some(Number::Integer(100)),
                        height: Some(Number::Integer(60)),
                        font_size: Some(Number::Integer(22)),
                        children: vec![positioned(Position::Fixed), positioned(Position::Absolute)],
                        ..Container::default()
                    }],
                    ..Container::default()
                }],
                ..Container::default()
            }
        }

        #[test_log::test]
        fn nested_fixed_percentages_use_viewport_absolute_uses_relative_container() {
            let mut tree = nested_positioned_tree();
            CALCULATOR.calc(&mut tree);
            let children = &tree.children[0].children[0].children;
            let fixed = &children[0];
            let absolute = &children[1];
            pretty_assertions::assert_eq!(fixed.calculated_width, Some(200.0));
            pretty_assertions::assert_eq!(fixed.calculated_height, Some(100.0));
            pretty_assertions::assert_eq!(fixed.calculated_x, Some(160.0));
            pretty_assertions::assert_eq!(fixed.calculated_y, Some(60.0));
            pretty_assertions::assert_eq!(absolute.calculated_width, Some(50.0));
            pretty_assertions::assert_eq!(absolute.calculated_height, Some(30.0));
            pretty_assertions::assert_eq!(absolute.calculated_x, Some(40.0));
            pretty_assertions::assert_eq!(absolute.calculated_y, Some(18.0));
        }

        fn assign_ids(tree: &mut Container, next: &mut usize, repeated: Option<usize>) {
            tree.id = repeated.unwrap_or(*next);
            *next += 1;
            for child in &mut tree.children {
                assign_ids(child, next, repeated);
            }
        }

        fn all_ids(tree: &Container, ids: &mut Vec<usize>) {
            ids.push(tree.id);
            for child in &tree.children {
                all_ids(child, ids);
            }
        }

        #[test_log::test]
        fn duplicate_and_zero_authored_ids_do_not_change_layout() {
            let mut unique = nested_positioned_tree();
            // A text descendant exercises inherited font context, not only
            // containing-block dimensions. No IDs are rewritten by Calculator.
            unique.children[0].children[0].children[1]
                .children
                .push(Container {
                    element: Element::Text {
                        value: "ab".to_owned(),
                    },
                    ..Container::default()
                });
            let mut next = 10;
            assign_ids(&mut unique, &mut next, None);
            for repeated_id in [0, 10] {
                let mut repeated = unique.clone();
                assign_ids(&mut repeated, &mut 0, Some(repeated_id));
                let mut before = Vec::new();
                all_ids(&repeated, &mut before);
                let mut expected = unique.clone();
                let expected_changed = CALCULATOR.calc(&mut expected);
                let actual_changed = CALCULATOR.calc(&mut repeated);
                pretty_assertions::assert_eq!(actual_changed, expected_changed);
                let mut after = Vec::new();
                all_ids(&repeated, &mut after);
                pretty_assertions::assert_eq!(after, before);
                // Compare all serializable calculated outputs after normalizing
                // identities on test-only copies; authored IDs remain intact.
                assign_ids(&mut expected, &mut 0, Some(repeated_id));
                compare_containers(&repeated, &expected);
            }
        }

        #[test_log::test]
        fn sticky_flow_position_is_not_overwritten_by_fixed_positioning() {
            let mut tree = Container {
                calculated_width: Some(200.0),
                calculated_height: Some(100.0),
                children: vec![
                    Container {
                        width: Some(Number::Integer(20)),
                        height: Some(Number::Integer(25)),
                        ..Container::default()
                    },
                    Container {
                        position: Some(Position::Sticky),
                        width: Some(Number::Integer(20)),
                        height: Some(Number::Integer(15)),
                        ..Container::default()
                    },
                ],
                ..Container::default()
            };
            CALCULATOR.calc(&mut tree);
            pretty_assertions::assert_eq!(tree.children[1].calculated_y, Some(25.0));
            pretty_assertions::assert_eq!(tree.children[1].calculated_height, Some(15.0));
        }

        #[test_log::test]
        fn does_size_dynamic_width_correctly() {
            let mut container: Container = html! {
                div sx-position=(Position::Fixed) sx-width="100%" {}
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_width: Some(400.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_size_dynamic_height_correctly() {
            let mut container: Container = html! {
                div sx-position=(Position::Fixed) sx-height="100%" {}
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_height: Some(100.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_place_child_element_with_horizontal_margins_correctly() {
            let mut container: Container = html! {
                div
                    sx-position=(Position::Fixed)
                    sx-width="100%"
                    sx-height="100%"
                    sx-justify-content=(JustifyContent::Center)
                {
                    div
                        sx-flex=(1)
                        sx-margin-x="calc(20vw)"
                        sx-min-height="calc(min(90vh, 300px))"
                        sx-max-height="90vh"
                        sx-overflow-y=(LayoutOverflow::Auto)
                    {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(1000.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_width: Some(600.0),
                            calculated_height: Some(500.0),
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_size_horizontally_child_element_correctly_with_flex_1_and_direction_row_parent() {
            let mut container: Container = html! {
                div
                    sx-dir=(LayoutDirection::Row)
                    sx-position=(Position::Fixed)
                    sx-width="100%"
                    sx-align-items=(AlignItems::Center)
                    sx-justify-content=(JustifyContent::Center)
                {
                    div
                        sx-flex=(1)
                        sx-margin-x="calc(20vw)"
                        sx-overflow-y=(LayoutOverflow::Auto)
                    { "test" }
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(1000.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            calculated_width: Some(600.0),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(1000.0),
                        ..container.children[0].clone()
                    }],
                    calculated_width: Some(1000.0),
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn calc_can_calc_fixed_positioned_element_nested_on_top_of_a_relative_element_with_left_offset()
         {
            let mut container = Container {
                children: vec![Container {
                    children: vec![
                        Container::default(),
                        Container {
                            left: Some(Number::Integer(30)),
                            position: Some(Position::Fixed),
                            ..Default::default()
                        },
                    ],
                    justify_content: Some(JustifyContent::Start),
                    ..Default::default()
                }],
                calculated_width: Some(100.0),
                calculated_height: Some(50.0),
                position: Some(Position::Relative),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                calculated_width: Some(100.0),
                                calculated_x: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(0.0),
                                calculated_height: Some(0.0),
                                calculated_x: Some(30.0),
                                calculated_y: Some(0.0),
                                position: Some(Position::Fixed),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        fn calc_can_calc_fixed_positioned_element_on_top_of_a_relative_element_with_left_offset() {
            let mut container = Container {
                children: vec![
                    Container::default(),
                    Container {
                        left: Some(Number::Integer(30)),
                        position: Some(Position::Fixed),
                        ..Default::default()
                    },
                ],
                calculated_width: Some(100.0),
                calculated_height: Some(50.0),
                position: Some(Position::Relative),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_x: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].clone()
                        },
                        Container {
                            left: Some(Number::Integer(30)),
                            calculated_width: Some(0.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(30.0),
                            calculated_y: Some(0.0),
                            position: Some(Position::Fixed),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container
                },
            );
        }

        #[test_log::test]
        fn calc_can_calc_fixed_positioned_element_with_explicit_sizes() {
            let mut container = Container {
                children: vec![
                    Container::default(),
                    Container {
                        width: Some(Number::Integer(30)),
                        height: Some(Number::Integer(20)),
                        left: Some(Number::Integer(30)),
                        position: Some(Position::Fixed),
                        ..Default::default()
                    },
                ],
                calculated_width: Some(100.0),
                calculated_height: Some(50.0),
                position: Some(Position::Relative),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_x: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_width: Some(30.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(30.0),
                            calculated_y: Some(0.0),
                            position: Some(Position::Fixed),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container
                },
            );
        }

        #[test_log::test]
        fn calc_can_calc_fixed_positioned_element_on_top_of_a_relative_element_doesnt_affect_fixed_position_element_location()
         {
            let mut container = Container {
                children: vec![
                    Container::default(),
                    Container {
                        children: vec![Container {
                            position: Some(Position::Fixed),
                            ..Default::default()
                        }],
                        left: Some(Number::Integer(30)),
                        position: Some(Position::Relative),
                        ..Default::default()
                    },
                ],
                calculated_width: Some(100.0),
                calculated_height: Some(50.0),
                position: Some(Position::Relative),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            };

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_x: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].clone()
                        },
                        Container {
                            children: vec![Container {
                                calculated_width: Some(0.0),
                                calculated_x: Some(0.0),
                                position: Some(Position::Fixed),
                                ..container.children[1].children[0].clone()
                            }],
                            calculated_width: Some(100.0),
                            calculated_x: Some(0.0),
                            position: Some(Position::Relative),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container
                },
            );
        }

        #[test_log::test]
        fn calc_can_calc_fixed_positioned_element_on_top_of_a_relative_element_and_have_it_not_inherit_position_or_size()
         {
            let mut container = Container {
                children: vec![
                    Container::default(),
                    Container {
                        position: Some(Position::Fixed),
                        ..Default::default()
                    },
                ],
                calculated_width: Some(100.0),
                calculated_height: Some(50.0),
                position: Some(Position::Relative),
                justify_content: Some(JustifyContent::Start),
                ..Default::default()
            };
            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_x: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_width: Some(0.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            position: Some(Position::Fixed),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container
                },
            );
        }
    }

    mod positioning {
        use hyperchad_transformer_models::{AlignItems, TextAlign, Visibility};

        use super::*;

        #[test_log::test]
        fn does_center_child_correctly() {
            let mut container: Container = html! {
                div
                    sx-width=(100)
                    sx-height=(50)
                    sx-justify-content=(JustifyContent::Center)
                    sx-align-items=(AlignItems::Center)
                {
                    div sx-width=(20) sx-height=(10) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_x: Some(40.0),
                        calculated_y: Some(20.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_center_raw_content_vertically_correctly() {
            let mut container: Container = html! {
                div
                    sx-width=(100)
                    sx-height=(50)
                    sx-justify-content=(JustifyContent::Center)
                {
                    "test"
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_y: Some(18.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_not_include_invisible_children_in_position() {
            let mut container: Container = html! {
                div sx-width=(100) sx-height=(50) {
                    div sx-visibility=(Visibility::Hidden) sx-height=(20) {}
                    "test"
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_y: None,
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_y: Some(0.0),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_center_child_correctly_with_dir_row_and_multiple_children() {
            let mut container: Container = html! {
                div
                    sx-dir=(LayoutDirection::Row)
                    sx-width=(100)
                    sx-height=(50)
                    sx-justify-content=(JustifyContent::Center)
                    sx-align-items=(AlignItems::Center)
                {
                    div sx-width=(20) sx-height=(10) {}
                    div sx-width=(20) sx-height=(10) {}
                    div sx-width=(20) sx-height=(10) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_x: Some(20.0),
                            calculated_y: Some(20.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_x: Some(40.0),
                            calculated_y: Some(20.0),
                            ..container.children[1].clone()
                        },
                        Container {
                            calculated_x: Some(60.0),
                            calculated_y: Some(20.0),
                            ..container.children[2].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_center_row_text_when_text_align_is_center() {
            let mut container: Container = html! {
                div
                    sx-width=(100)
                    sx-height=(50)
                    sx-text-align=(TextAlign::Center)
                {
                    "test"
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_x: Some(22.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_center_row_text_when_text_align_is_center_and_container_direction_is_row() {
            let mut container: Container = html! {
                div
                    sx-dir=(LayoutDirection::Row)
                    sx-width=(100)
                    sx-height=(50)
                    sx-text-align=(TextAlign::Center)
                {
                    "test"
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        calculated_x: Some(22.0),
                        ..container.children[0].clone()
                    }],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_center_vertical_row_children() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-width=(100) sx-height=(100) sx-align-items=(AlignItems::Center) {
                    div sx-height=(70) {}
                    div sx-height=(48) {}
                }
            }
            .into_string().try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_y: Some(15.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_y: Some(26.0),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_center_horizontal_column_children() {
            let mut container: Container = html! {
                div sx-width=(100) sx-height=(100) sx-align-items=(AlignItems::Center) {
                    div sx-width=(70) {}
                    div sx-width=(48) {}
                }
            }
            .into_string()
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{container}");
            container = container.children[0].clone();
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_x: Some(15.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_x: Some(26.0),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_column_gap_for_non_wrapping_div() {
            let mut container: Container = html! {
                div sx-width=(20) {}
                div sx-width=(30) {}
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            container.column_gap = Some(Number::Integer(5));

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_x: Some(0.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_x: Some(20.0 + 5.0),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_column_gap_for_non_wrapping_div_and_centers_properly() {
            let mut container: Container = html! {
                div sx-width=(20) {}
                div sx-width=(30) {}
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            container.column_gap = Some(Number::Integer(5));
            container.justify_content = Some(JustifyContent::Center);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_x: Some(400.0 / 2.0 - (20.0 + 30.0 + 5.0) / 2.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_x: Some(
                                400.0 / 2.0 - (20.0 + 30.0 + 5.0) / 2.0 + 20.0 + 5.0,
                            ),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_row_gap_for_non_wrapping_div() {
            let mut container: Container = html! {
                div sx-height=(20) {}
                div sx-height=(30) {}
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);
            container.row_gap = Some(Number::Integer(5));

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_y: Some(0.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_y: Some(20.0 + 5.0),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[test_log::test]
        fn does_take_into_account_row_gap_for_non_wrapping_div_and_centers_properly() {
            let mut container: Container = html! {
                div sx-height=(20) {}
                div sx-height=(30) {}
            }
            .into_string()
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);
            container.row_gap = Some(Number::Integer(5));
            container.justify_content = Some(JustifyContent::Center);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{container}");

            compare_containers(
                &container,
                &Container {
                    children: vec![
                        Container {
                            calculated_y: Some(400.0 / 2.0 - (20.0 + 30.0 + 5.0) / 2.0),
                            ..container.children[0].clone()
                        },
                        Container {
                            calculated_y: Some(
                                400.0 / 2.0 - (20.0 + 30.0 + 5.0) / 2.0 + 20.0 + 5.0,
                            ),
                            ..container.children[1].clone()
                        },
                    ],
                    ..container.clone()
                },
            );
        }

        #[cfg(feature = "layout-offset")]
        mod offset {
            use super::*;

            #[test_log::test]
            fn does_set_offset_x_for_single_element() {
                let mut container: Container = html! {
                    div {}
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        calculated_offset_x: Some(0.0),
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_y_for_single_element() {
                let mut container: Container = html! {
                    div {}
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        calculated_offset_y: Some(0.0),
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_x_for_two_elements_col() {
                let mut container: Container = html! {
                    div {
                        div {}
                        div {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_x: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_x: Some(0.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_x_for_justify_content_end_row() {
                let mut container: Container = html! {
                    div sx-dir=(LayoutDirection::Row) sx-justify-content=(JustifyContent::End) {
                        div sx-width=(50) {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![Container {
                            calculated_offset_x: Some(400.0 - 50.0),
                            ..container.children[0].clone()
                        }],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_y_for_justify_content_end_col() {
                let mut container: Container = html! {
                    div sx-height=(100) sx-justify-content=(JustifyContent::End) {
                        div sx-height=(20) {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![Container {
                            calculated_offset_y: Some(100.0 - 20.0),
                            ..container.children[0].clone()
                        }],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_y_for_align_items_end_row() {
                let mut container: Container = html! {
                    div sx-height=(100) sx-dir=(LayoutDirection::Row) sx-align-items=(JustifyContent::End) {
                        div sx-height=(20) {}
                    }
                }
                .into_string().try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![Container {
                            calculated_offset_y: Some(100.0 - 20.0),
                            ..container.children[0].clone()
                        }],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_x_for_align_items_end_col() {
                let mut container: Container = html! {
                    div sx-align-items=(JustifyContent::End) {
                        div sx-width=(50) {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![Container {
                            calculated_offset_x: Some(400.0 - 50.0),
                            ..container.children[0].clone()
                        }],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_x_for_two_elements_row() {
                let mut container: Container = html! {
                    div sx-dir=(LayoutDirection::Row) {
                        div {}
                        div {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_x: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_x: Some(0.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_y_for_two_elements_col() {
                let mut container: Container = html! {
                    div {
                        div {}
                        div {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_y: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_y: Some(0.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_set_offset_y_for_two_elements_row() {
                let mut container: Container = html! {
                    div sx-dir=(LayoutDirection::Row) {
                        div {}
                        div {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_y: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_y: Some(0.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_include_gap_in_offset_x_overflow_x_wrap() {
                let mut container: Container = html! {
                    div sx-dir=(LayoutDirection::Row) sx-overflow-x=(LayoutOverflow::Wrap { grid: false }) sx-gap=(10) {
                        div sx-width=(50) {}
                        div sx-width=(50) {}
                    }
                }
                .into_string().try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_x: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_x: Some(10.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            #[ignore = "Unimplemented"]
            fn does_include_gap_in_offset_y_overflow_y_wrap() {
                let mut container: Container = html! {
                    div sx-overflow-y=(LayoutOverflow::Wrap { grid: false }) sx-gap=(10) {
                        div sx-height=(50) {}
                        div sx-height=(50) {}
                    }
                }
                .into_string()
                .try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_y: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_y: Some(10.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            fn does_include_gap_in_offset_y_overflow_x_wrap() {
                let mut container: Container = html! {
                    div sx-width=(50) sx-dir=(LayoutDirection::Row) sx-overflow-x=(LayoutOverflow::Wrap { grid: false }) sx-gap=(10) {
                        div sx-width=(50) {}
                        div sx-width=(50) {}
                    }
                }
                .into_string().try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_y: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_y: Some(10.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }

            #[test_log::test]
            #[ignore = "Unimplemented"]
            fn does_include_gap_in_offset_x_overflow_y_wrap() {
                let mut container: Container = html! {
                    div sx-height=(50) sx-overflow-y=(LayoutOverflow::Wrap { grid: false }) sx-gap=(10) {
                        div sx-height=(50) {}
                        div sx-height=(50) {}
                    }
                }
                .into_string().try_into()
                .unwrap();

                container.calculated_width = Some(400.0);
                container.calculated_height = Some(100.0);

                CALCULATOR.calc(&mut container);
                log::trace!("full container:\n{container}");
                container = container.children[0].clone();
                log::trace!("container:\n{container}");

                compare_containers(
                    &container,
                    &Container {
                        children: vec![
                            Container {
                                calculated_offset_x: Some(0.0),
                                ..container.children[0].clone()
                            },
                            Container {
                                calculated_offset_x: Some(10.0),
                                ..container.children[1].clone()
                            },
                        ],
                        ..container.clone()
                    },
                );
            }
        }
    }
}
