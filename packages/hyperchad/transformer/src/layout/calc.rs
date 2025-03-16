use bumpalo::Bump;

use crate::Container;

use super::{Calc, font::FontMetrics};

pub struct Calculator<F: FontMetrics> {
    #[allow(unused)]
    font_metrics: F,
}

impl<F: FontMetrics> Calculator<F> {
    pub const fn new(font_metrics: F) -> Self {
        Self { font_metrics }
    }
}

impl<F: FontMetrics> Calc for Calculator<F> {
    #[allow(clippy::let_and_return)]
    fn calc(&self, container: &mut Container) -> bool {
        use pass_flex_height::Pass as _;
        use pass_flex_width::Pass as _;
        use pass_heights::Pass as _;
        use pass_positioning::Pass as _;
        use pass_widths::Pass as _;
        use pass_wrap_horizontal::Pass as _;

        log::trace!("calc: container={container}");

        let bfs = container.bfs();
        let arena = Bump::new();

        let changed = false;

        let changed = self.calc_widths(&bfs, container) || changed;
        let changed = self.flex_width(&bfs, container) || changed;
        let changed = self.wrap_horizontal(&bfs, container) || changed;
        let changed = self.calc_heights(&bfs, container) || changed;
        let changed = self.flex_height(&bfs, container) || changed;
        let changed = self.position_elements(&arena, &bfs, container) || changed;

        changed
    }
}

macro_rules! float_eq {
    ($a:expr, $b:expr $(,)?) => {{ ($a - $b).abs() < crate::layout::EPSILON }};
}

macro_rules! calc_size_on_axis {
    (
        $label:tt,
        $self:ident,
        $bfs:ident,
        $container:ident,
        $fixed:ident,
        $calculated:ident,
        $calculated_min:ident,
        $calculated_preferred:ident,
        $axis:ident,
        $cross_axis:ident,
        $margin_x:ident,
        $margin_y:ident,
        $calculated_margin_x:ident,
        $calculated_margin_y:ident,
        $padding_x:ident,
        $padding_y:ident,
        $calculated_padding_x:ident,
        $calculated_padding_y:ident,
        $border_x:ident,
        $border_y:ident,
        $calculated_border_x:ident,
        $calculated_border_y:ident,
        $gap:ident,
        $calculated_gap:ident,
        $overflow:ident,
        $each_child:expr
        $(,)?
    ) => {{
        use crate::{LayoutDirection, LayoutOverflow, Position};

        const LABEL: &str = $label;

        moosicbox_logging::debug_or_trace!(("{LABEL}"), ("{LABEL}:\n{}", $container));

        let view_width = $container.calculated_width.expect("Missing view_width");
        let view_height = $container.calculated_height.expect("Missing view_height");

        let mut changed = false;

        $bfs.traverse_rev_mut($container, |parent| {
            let mut min_size = 0.0;
            let mut preferred_size = 0.0;

            if let Some(gap) = &parent.$gap.as_ref().and_then(crate::Number::as_fixed) {
                let gap = gap.calc(0.0, view_width, view_height);
                log::trace!("{LABEL}: setting gap={gap}");
                parent.$calculated_gap = Some(gap);
            }

            let direction = parent.direction;
            let overflow = parent.$overflow;

            for child in &mut parent.children {
                log::trace!("{LABEL}: container:\n{child}");

                let (mut min, mut preferred) = if let Some(size) = child.$fixed.as_ref().and_then(Number::as_fixed) {
                    let new_size = size.calc(0.0, view_width, view_height);

                    if set_float(&mut child.$calculated, new_size).is_some() {
                        changed = true;
                    }
                    (Some(new_size), new_size)
                } else if let crate::Element::Raw { value } = &child.element {
                    log::trace!("{LABEL}: measuring text={value}");
                    let bounds = $self.font_metrics.measure_text(value, 14.0, f32::INFINITY);
                    log::trace!("{LABEL}: measured bounds={bounds:?}");
                    let new_size = bounds.$fixed();
                    log::trace!("{LABEL}: measured size={new_size}");

                    if set_float(&mut child.$calculated, new_size).is_some() {
                        changed = true;
                    }
                    if LayoutDirection::$axis == LayoutDirection::Column {
                        (Some(new_size), new_size)
                    } else {
                        (None, new_size)
                    }
                } else if let Some(size) = child.$calculated_preferred {
                    set_float(&mut child.$calculated, size);
                    (child.$calculated_min, size)
                } else if let Some(size) = child.$calculated_min {
                    set_float(&mut child.$calculated, size);
                    (Some(size), size)
                } else {
                    set_float(&mut child.$calculated, 0.0);
                    (None, 0.0)
                };

                if let Some(margin) = child.$margin_x.as_ref().and_then(crate::Number::as_fixed) {
                    let size = margin.calc(0.0, view_width, view_height);
                    if set_float(&mut child.$calculated_margin_x, size).is_some() {
                        changed = true;
                    }
                    preferred += size;
                    crate::layout::increase_opt(&mut min, size);
                }
                if let Some(margin) = child.$margin_y.as_ref().and_then(crate::Number::as_fixed) {
                    let size = margin.calc(0.0, view_width, view_height);
                    if set_float(&mut child.$calculated_margin_y, size).is_some() {
                        changed = true;
                    }
                    preferred += size;
                    crate::layout::increase_opt(&mut min, size);
                }
                if let Some(padding) = child.$padding_x.as_ref().and_then(crate::Number::as_fixed) {
                    let size = padding.calc(0.0, view_width, view_height);
                    if set_float(&mut child.$calculated_padding_x, size).is_some() {
                        changed = true;
                    }
                    preferred += size;
                    crate::layout::increase_opt(&mut min, size);
                }
                if let Some(padding) = child.$padding_y.as_ref().and_then(crate::Number::as_fixed) {
                    let size = padding.calc(0.0, view_width, view_height);
                    if set_float(&mut child.$calculated_padding_y, size).is_some() {
                        changed = true;
                    }
                    preferred += size;
                    crate::layout::increase_opt(&mut min, size);
                }
                if let Some((&color, size)) = child
                    .$border_x
                    .as_ref()
                    .and_then(|(color, size)| size.as_fixed().map(|size| (color, size)))
                {
                    let size = size.calc(0.0, view_width, view_height);
                    if let Some(calculated) = &mut child.$calculated_border_x {
                        if calculated.0 != color {
                            calculated.0 = color;
                            changed = true;
                        }
                        if !float_eq!(calculated.1, size) {
                            calculated.1 = size;
                            changed = true;
                        }
                    } else {
                        child.$calculated_border_x = Some((color, size));
                        changed = true;
                    }
                }
                if let Some((&color, size)) = child
                    .$border_y
                    .as_ref()
                    .and_then(|(color, size)| size.as_fixed().map(|size| (color, size)))
                {
                    let size = size.calc(0.0, view_width, view_height);
                    if let Some(calculated) = &mut child.$calculated_border_y {
                        if calculated.0 != color {
                            calculated.0 = color;
                            changed = true;
                        }
                        if !float_eq!(calculated.1, size) {
                            calculated.1 = size;
                            changed = true;
                        }
                    } else {
                        child.$calculated_border_y = Some((color, size));
                        changed = true;
                    }
                }

                let handle_sizing = |child: &mut Container, size, output: &mut f32| {
                    enum MinSizeHandling {
                        Add,
                        Max,
                    }

                    let handling = match child.position.unwrap_or_default() {
                        Position::Static | Position::Relative => match overflow {
                            LayoutOverflow::Expand | LayoutOverflow::Squash => {
                                Some(match direction {
                                    LayoutDirection::$axis => MinSizeHandling::Add,
                                    LayoutDirection::$cross_axis => MinSizeHandling::Max,
                                })
                            }
                            LayoutOverflow::Wrap { .. } => {
                                Some(MinSizeHandling::Max)
                            }
                            LayoutOverflow::Auto | LayoutOverflow::Scroll | LayoutOverflow::Hidden => None
                        },
                        Position::Absolute | Position::Sticky | Position::Fixed => None
                    };

                    if let Some(handling) = handling {
                        match handling {
                            MinSizeHandling::Add => {
                                log::trace!("{LABEL}: MinSizeHandling::Add output={output} += size={size} ({})", *output + size);
                                *output += size;
                            }
                            MinSizeHandling::Max => {
                                log::trace!("{LABEL}: MinSizeHandling::Add size={size} > output={output} ({})", if size > *output { size } else { *output });
                                if size > *output {
                                    *output = size;
                                }
                            }
                        }
                    }
                };

                child.$calculated_preferred = Some(preferred);
                handle_sizing(child, preferred, &mut preferred_size);

                if let Some(size) = min {
                    child.$calculated_min = Some(size);
                    handle_sizing(child, size, &mut min_size);
                }

                $each_child(child, view_width, view_height);
            }

            set_float(&mut parent.$calculated_min, min_size);
            set_float(&mut parent.$calculated_preferred, preferred_size);
        });

        changed
    }};
}

#[derive(Clone, Copy, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

macro_rules! flex_on_axis {
    (
        $label:tt,
        $bfs:ident,
        $container:ident,
        $fixed:ident,
        $calculated:ident,
        $calculated_min:ident,
        $axis:ident,
        $cross_axis:ident,
        $cell:ident,
        $margin_x:ident,
        $margin_y:ident,
        $calculated_margin_x:ident,
        $calculated_margin_y:ident,
        $margin_axis:ident,
        $padding_x:ident,
        $padding_y:ident,
        $calculated_padding_x:ident,
        $calculated_padding_y:ident,
        $padding_axis:ident,
        $border_x:ident,
        $border_y:ident,
        $calculated_border_x:ident,
        $calculated_border_y:ident,
        $gap:ident,
        $calculated_gap:ident,
        $each_child:expr
        $(,)?
    ) => {{
        use crate::Element;

        const LABEL: &str = $label;

        moosicbox_logging::debug_or_trace!(("{LABEL}"), ("{LABEL}:\n{}", $container));

        let mut changed = false;

        let root_id = $container.id;
        let view_width = $container.calculated_width.expect("Missing view_width");
        let view_height = $container.calculated_height.expect("Missing view_height");

        #[allow(clippy::cognitive_complexity)]
        $bfs.traverse_with_parents_mut(
            true,
            super::Rect::default(),
            $container,
            |parent, relative_container| {
                if parent.id == root_id {
                    super::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: view_width,
                        height: view_height,
                    }
                } else if parent.position == Some(Position::Relative) {
                    super::Rect {
                        $fixed: parent.$calculated.expect("Missing parent calculated size"),
                        ..Default::default()
                    }
                } else {
                    relative_container
                }
            },
            |parent, relative_container| {
                let direction = parent.direction;
                let container_size = parent.$calculated.expect("Missing container size");

                if let Some(gap) = &parent.$gap.as_ref().and_then(crate::Number::as_dynamic) {
                    parent.$calculated_gap =
                        Some(gap.calc(container_size, view_width, view_height));
                }

                for child in &mut parent.children {
                    if let Some((&color, size)) = child
                        .$border_x
                        .as_ref()
                        .and_then(|(color, size)| size.as_dynamic().map(|size| (color, size)))
                    {
                        let size = size.calc(0.0, view_width, view_height);
                        if let Some(calculated) = &mut child.$calculated_border_x {
                            if calculated.0 != color {
                                calculated.0 = color;
                                changed = true;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                                changed = true;
                            }
                        } else {
                            child.$calculated_border_x = Some((color, size));
                            changed = true;
                        }
                    }
                    if let Some((&color, size)) = child
                        .$border_y
                        .as_ref()
                        .and_then(|(color, size)| size.as_dynamic().map(|size| (color, size)))
                    {
                        let size = size.calc(0.0, view_width, view_height);
                        if let Some(calculated) = &mut child.$calculated_border_y {
                            if calculated.0 != color {
                                calculated.0 = color;
                                changed = true;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                                changed = true;
                            }
                        } else {
                            child.$calculated_border_y = Some((color, size));
                            changed = true;
                        }
                    }

                    if let Some(margin) = child.$margin_x.as_ref().and_then(crate::Number::as_dynamic) {
                        let size = margin.calc(container_size, view_width, view_height);
                        if set_float(&mut child.$calculated_margin_x, size).is_some() {
                            changed = true;
                        }
                    }
                    if let Some(margin) = child.$margin_y.as_ref().and_then(crate::Number::as_dynamic) {
                        let size = margin.calc(container_size, view_width, view_height);
                        if set_float(&mut child.$calculated_margin_y, size).is_some() {
                            changed = true;
                        }
                    }
                    if let Some(padding) = child.$padding_x.as_ref().and_then(crate::Number::as_dynamic) {
                        let size = padding.calc(container_size, view_width, view_height);
                        if set_float(&mut child.$calculated_padding_x, size).is_some() {
                            changed = true;
                        }
                    }
                    if let Some(padding) = child.$padding_y.as_ref().and_then(crate::Number::as_dynamic) {
                        let size = padding.calc(container_size, view_width, view_height);
                        if set_float(&mut child.$calculated_padding_y, size).is_some() {
                            changed = true;
                        }
                    }

                    $each_child(child, container_size, view_width, view_height);
                }

                if parent.relative_positioned_elements().any(|x| x.$fixed.as_ref().is_none_or(crate::Number::is_dynamic)) {
                    let mut remaining_container_size = container_size;

                    // Remove margins & padding from remaining_container_size
                    for child in parent.relative_positioned_elements() {
                        match direction {
                            LayoutDirection::$axis => {
                                if let Some(size) = child.$margin_axis() {
                                    log::trace!(
                                        "{LABEL}: removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                                        remaining_container_size - size
                                    );
                                    remaining_container_size -= size;
                                }
                                if let Some(size) = child.$padding_axis() {
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
                        if let Some(size) = child.$fixed.as_ref().and_then(crate::Number::as_dynamic) {
                            let container_size = match direction {
                                LayoutDirection::$axis => container_size,
                                LayoutDirection::$cross_axis => {
                                    container_size
                                        - child.$margin_axis().unwrap_or_default()
                                        - child.$padding_axis().unwrap_or_default()
                                }
                            };
                            log::trace!("{LABEL}: calculating dynamic size={size:?}");
                            let size = size.calc(container_size, view_width, view_height);
                            log::trace!("{LABEL}: calculated dynamic size={size}");
                            if set_float(&mut child.$calculated, size).is_some() {
                                changed = true;
                            }
                        }
                    }

                    // Fit all unsized children
                    if parent.relative_positioned_elements().any(|x| x.$fixed.as_ref().is_none()) {
                        let mut remaining_size = container_size;
                        let mut last_cell = 0;
                        let mut max_cell_size = 0.0;

                        // Remove sized children sizes from remaining_size
                        for child in parent.relative_positioned_elements() {
                            log::trace!("{LABEL}: calculating remaining size:\n{child}");

                            match direction {
                                LayoutDirection::$axis => {
                                    if let Some(size) = child.$calculated {
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
                                            max_cell_size = child.$calculated.unwrap_or_default();
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

                        let mut position_cross_axis = |parent: &mut Container| {
                            for child in parent.relative_positioned_elements_mut() {
                                if matches!(child.element, Element::Raw { .. }) {
                                    continue;
                                }

                                log::trace!("{LABEL}: setting size to remaining_size={remaining_size}:\n{child}");
                                let mut remaining_container_size = remaining_size;

                                if let Some(size) = child.$margin_axis() {
                                    log::trace!(
                                        "{LABEL}: removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                                        remaining_container_size - size
                                    );
                                    remaining_container_size -= size;
                                }
                                if let Some(size) = child.$padding_axis() {
                                    log::trace!(
                                        "{LABEL}: removing padding size={size} from remaining_container_size={remaining_container_size} ({})",
                                        remaining_container_size - size
                                    );
                                    remaining_container_size -= size;
                                }

                                #[allow(clippy::cast_precision_loss)]
                                let mut new_size = remaining_container_size / (cell_count as f32);

                                if let Some(min) = child.$calculated_min {
                                    if new_size < min {
                                        new_size = min;
                                    }
                                } else if new_size < 0.0 {
                                    new_size = 0.0;
                                }

                                if child.$fixed.is_none() && set_float(&mut child.$calculated, new_size).is_some() {
                                    changed = true;
                                }
                            }
                        };

                        log::trace!("{LABEL}: remaining_size={remaining_size}\n{parent}");

                        if parent.is_flex_container() {
                            // Fit all unsized children to remaining_size
                            match direction {
                                LayoutDirection::$axis => {
                                    loop {
                                        let mut smallest = f32::INFINITY;
                                        let mut target = f32::INFINITY;
                                        let mut smallest_count = 0_u16;

                                        for size in parent
                                            .relative_positioned_elements()
                                            .filter(|x| x.$fixed.is_none())
                                            .filter_map(|x| x.$calculated)
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
                                            log::trace!("{LABEL}: target > remaining_size");
                                            target = if smallest_count == 1 {
                                                remaining_size
                                            } else {
                                                remaining_size / smallest_countf
                                            };
                                            remaining_size = 0.0;
                                            true
                                        } else {
                                            remaining_size -= (target - smallest) * smallest_countf;
                                            false
                                        };

                                        log::trace!("{LABEL}: target={target} smallest={smallest} smallest_count={smallest_count} remaining_size={remaining_size} container_size={container_size}");

                                        moosicbox_assert::assert!(target.is_finite(), "expected target to be finite");

                                        for child in parent
                                            .relative_positioned_elements_mut()
                                            .filter(|x| x.$fixed.is_none())
                                            .filter(|x| x.$calculated.is_some_and(|x| float_eq!(x, smallest)))
                                        {
                                            let mut target = target;

                                            if let Some(min) = child.$calculated_min {
                                                log::trace!("{LABEL}: calculated_min={min}");
                                                let min = min - child.$padding_axis().unwrap_or_default() - child.$margin_axis().unwrap_or_default();
                                                log::trace!("{LABEL}: without padding/margins calculated_min={min}");
                                                if target < min {
                                                    target = min;
                                                }
                                            }

                                            log::trace!("{LABEL}: increasing child size to target={target}:\n{child}");
                                            set_float(&mut child.$calculated, target);
                                        }

                                        if last_iteration {
                                            break;
                                        }
                                    }
                                }
                                LayoutDirection::$cross_axis => {
                                    position_cross_axis(parent);
                                }
                            }
                        } else {
                            match direction {
                                LayoutDirection::$axis => {}
                                LayoutDirection::$cross_axis => {
                                    position_cross_axis(parent);
                                }
                            }
                        }
                    }
                }

                // absolute positioned

                let super::Rect { $fixed: relative_size, .. } = relative_container;

                for child in parent.absolute_positioned_elements_mut() {
                    let mut remaining_container_size = relative_size;

                    if let Some(size) = child.$margin_axis() {
                        log::trace!(
                            "{LABEL}: absolute removing margin size={size} from remaining_container_size={remaining_container_size} ({})",
                            remaining_container_size - size
                        );
                        remaining_container_size -= size;
                    }
                    if let Some(size) = child.$padding_axis() {
                        log::trace!(
                            "{LABEL}: absolute removing padding size={size} from remaining_container_size={remaining_container_size} ({})",
                            remaining_container_size - size
                        );
                        remaining_container_size -= size;
                    }

                    if let Some(size) = &child.$fixed {
                        log::trace!("{LABEL}: calculating absolute child size={size:?}");
                        let size = size.calc(remaining_container_size, view_width, view_height);
                        log::trace!("{LABEL}: calculated absolute child size={size}");
                        if set_float(&mut child.$calculated, size).is_some() {
                            changed = true;
                        }
                    } else if set_float(&mut child.$calculated, remaining_container_size).is_some() {
                        changed = true;
                    }
                }
            });

        changed
    }};
}

macro_rules! wrap_on_axis {
    (
        $label:tt,
        $axis:ident,
        $bfs:ident,
        $container:ident,
        $calculated:ident,
        $overflow:ident,
        $margin_axis:ident,
        $padding_axis:ident,
        $calculated_gap:ident
        $(,)?
    ) => {{
        const LABEL: &str = $label;

        moosicbox_logging::debug_or_trace!(("{LABEL}"), ("{LABEL}:\n{}", $container));

        let mut changed = true;

        $bfs.traverse_mut($container, |parent| {
            if !matches!(parent.$overflow, LayoutOverflow::Wrap { .. }) {
                return;
            }

            let container_size = parent.$calculated.expect("Missing parent container_size");

            let direction = parent.direction;
            let mut pos = 0.0;
            let mut row = 0;
            let mut col = 0;
            let gap = parent.$calculated_gap;

            for child in parent.relative_positioned_elements_mut() {
                let child_size =
                    child.$calculated.expect("Missing child calculated size")
                        + child.$margin_axis().unwrap_or_default()
                        + child.$padding_axis().unwrap_or_default();

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

                if set_value(&mut child.calculated_position, position).is_some() {
                    changed = true;
                }
            }
        });

        changed
    }};
}

/// # Pass 1: Widths
///
/// This pass traverses the `Container` children in reverse BFS (Breadth-First Search)
/// and calculates the widths required for each of the `Container`s.
mod pass_widths {
    use crate::{
        BfsPaths, Container, Number,
        layout::{font::FontMetrics, set_float},
    };

    use super::Calculator;

    pub trait Pass {
        fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for Calculator<F> {
        fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            let each_child = |container: &mut Container, view_width, view_height| {
                if let Some(opacity) = &container.opacity {
                    container.calculated_opacity = Some(opacity.calc(1.0, view_width, view_height));
                }
                if let Some(radius) = &container
                    .border_top_left_radius
                    .as_ref()
                    .and_then(crate::Number::as_fixed)
                {
                    container.calculated_border_top_left_radius =
                        Some(radius.calc(0.0, view_width, view_height));
                }
                if let Some(radius) = &container
                    .border_top_right_radius
                    .as_ref()
                    .and_then(crate::Number::as_fixed)
                {
                    container.calculated_border_top_right_radius =
                        Some(radius.calc(0.0, view_width, view_height));
                }
                if let Some(radius) = &container
                    .border_bottom_left_radius
                    .as_ref()
                    .and_then(crate::Number::as_fixed)
                {
                    container.calculated_border_bottom_left_radius =
                        Some(radius.calc(0.0, view_width, view_height));
                }
                if let Some(radius) = &container
                    .border_bottom_right_radius
                    .as_ref()
                    .and_then(crate::Number::as_fixed)
                {
                    container.calculated_border_bottom_right_radius =
                        Some(radius.calc(0.0, view_width, view_height));
                }
            };

            calc_size_on_axis!(
                "calc_widths",
                self,
                bfs,
                container,
                width,
                calculated_width,
                calculated_min_width,
                calculated_preferred_width,
                Row,
                Column,
                margin_left,
                margin_right,
                calculated_margin_left,
                calculated_margin_right,
                padding_left,
                padding_right,
                calculated_padding_left,
                calculated_padding_right,
                border_left,
                border_right,
                calculated_border_left,
                calculated_border_right,
                column_gap,
                calculated_column_gap,
                overflow_x,
                each_child,
            )
        }
    }
}

mod pass_flex_width {
    use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{font::FontMetrics, set_float},
    };

    use super::Calculator;

    pub trait Pass {
        fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for Calculator<F> {
        fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            let each_child =
                |container: &mut Container, container_width, view_width, view_height| {
                    if let Some(radius) = &container
                        .border_top_left_radius
                        .as_ref()
                        .and_then(crate::Number::as_dynamic)
                    {
                        container.calculated_border_top_left_radius =
                            Some(radius.calc(container_width, view_width, view_height));
                    }
                    if let Some(radius) = &container
                        .border_top_right_radius
                        .as_ref()
                        .and_then(crate::Number::as_dynamic)
                    {
                        container.calculated_border_top_right_radius =
                            Some(radius.calc(container_width, view_width, view_height));
                    }
                    if let Some(radius) = &container
                        .border_bottom_left_radius
                        .as_ref()
                        .and_then(crate::Number::as_dynamic)
                    {
                        container.calculated_border_bottom_left_radius =
                            Some(radius.calc(container_width, view_width, view_height));
                    }
                    if let Some(radius) = &container
                        .border_bottom_right_radius
                        .as_ref()
                        .and_then(crate::Number::as_dynamic)
                    {
                        container.calculated_border_bottom_right_radius =
                            Some(radius.calc(container_width, view_width, view_height));
                    }
                };

            flex_on_axis!(
                "flex_width",
                bfs,
                container,
                width,
                calculated_width,
                calculated_min_width,
                Row,
                Column,
                col,
                margin_left,
                margin_right,
                calculated_margin_left,
                calculated_margin_right,
                horizontal_margin,
                padding_left,
                padding_right,
                calculated_padding_left,
                calculated_padding_right,
                horizontal_padding,
                border_left,
                border_right,
                calculated_border_left,
                calculated_border_right,
                column_gap,
                calculated_column_gap,
                each_child,
            )
        }
    }
}

mod pass_wrap_horizontal {
    use hyperchad_transformer_models::{LayoutDirection, LayoutOverflow, LayoutPosition};

    use crate::{
        BfsPaths, Container,
        layout::{font::FontMetrics, set_value},
    };

    use super::Calculator;

    pub trait Pass {
        fn wrap_horizontal(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for Calculator<F> {
        fn wrap_horizontal(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            wrap_on_axis!(
                "wrap",
                Row,
                bfs,
                container,
                calculated_width,
                overflow_x,
                horizontal_margin,
                horizontal_padding,
                calculated_column_gap,
            )
        }
    }
}

mod pass_heights {
    use crate::{
        BfsPaths, Container, Number,
        layout::{font::FontMetrics, set_float},
    };

    use super::Calculator;

    pub trait Pass {
        fn calc_heights(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for Calculator<F> {
        fn calc_heights(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            calc_size_on_axis!(
                "calc_heights",
                self,
                bfs,
                container,
                height,
                calculated_height,
                calculated_min_height,
                calculated_preferred_height,
                Column,
                Row,
                margin_top,
                margin_bottom,
                calculated_margin_top,
                calculated_margin_bottom,
                padding_top,
                padding_bottom,
                calculated_padding_top,
                calculated_padding_bottom,
                border_top,
                border_bottom,
                calculated_border_top,
                calculated_border_bottom,
                row_gap,
                calculated_row_gap,
                overflow_y,
                |_, _, _| {},
            )
        }
    }
}

mod pass_flex_height {
    use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{font::FontMetrics, set_float},
    };

    use super::Calculator;

    pub trait Pass {
        fn flex_height(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for Calculator<F> {
        fn flex_height(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            flex_on_axis!(
                "flex_height",
                bfs,
                container,
                height,
                calculated_height,
                calculated_min_height,
                Column,
                Row,
                row,
                margin_top,
                margin_bottom,
                calculated_margin_top,
                calculated_margin_bottom,
                vertical_margin,
                padding_top,
                padding_bottom,
                calculated_padding_top,
                calculated_padding_bottom,
                vertical_padding,
                border_top,
                border_bottom,
                calculated_border_top,
                calculated_border_bottom,
                row_gap,
                calculated_row_gap,
                |_, _, _, _| {},
            )
        }
    }
}

mod pass_positioning {
    use bumpalo::Bump;
    use hyperchad_transformer_models::{
        AlignItems, JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition, Position,
        TextAlign,
    };

    use crate::{
        BfsPaths, Container, Element,
        layout::{font::FontMetrics, order_float, set_float},
    };

    use super::Calculator;

    pub trait Pass {
        fn position_elements(
            &self,
            arena: &Bump,
            bfs: &BfsPaths,
            container: &mut Container,
        ) -> bool;
    }

    impl<F: FontMetrics> Pass for Calculator<F> {
        #[allow(clippy::too_many_lines)]
        fn position_elements(
            &self,
            arena: &Bump,
            bfs: &BfsPaths,
            container: &mut Container,
        ) -> bool {
            moosicbox_logging::debug_or_trace!(
                ("position_elements"),
                ("position_elements:\n{container}")
            );

            let root_id = container.id;
            let view_width = container.calculated_width.expect("Missing view_width");
            let view_height = container.calculated_height.expect("Missing view_height");

            let mut changed = false;

            #[allow(clippy::cognitive_complexity)]
            bfs.traverse_with_parents_mut(
                true,
                Container::default(),
                container,
                |parent, mut relative_container| {
                    if parent.id == root_id {
                        relative_container.calculated_x = Some(0.0);
                        relative_container.calculated_y = Some(0.0);
                        relative_container.calculated_width = Some(view_width);
                        relative_container.calculated_height = Some(view_height);
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

                    relative_container
                },
                |parent, relative_container| {
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

                        for child in parent.relative_positioned_elements_mut() {
                            let Some(LayoutPosition::Wrap { row, col }) = child.calculated_position
                            else {
                                continue;
                            };
                            log::trace!("position_elements: wrap calculating gaps ({row}, {col})");

                            if row != last_row {
                                moosicbox_assert::assert!(row > last_row);

                                let remainder = container_width - row_width;

                                #[allow(clippy::cast_precision_loss)]
                                let gap = match justify_content {
                                    JustifyContent::Start
                                    | JustifyContent::Center
                                    | JustifyContent::End => 0.0,
                                    JustifyContent::SpaceBetween => {
                                        remainder / ((col_count - 1) as f32)
                                    }
                                    JustifyContent::SpaceEvenly => {
                                        remainder / ((col_count + 1) as f32)
                                    }
                                };
                                gaps.push(gap);

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

                        let remainder = container_width - row_width;
                        #[allow(clippy::cast_precision_loss)]
                        let gap = remainder / ((col_count + 1) as f32);
                        gaps.push(gap);

                        #[allow(unused_assignments)]
                        if col_count > max_col_count {
                            max_col_count = col_count;
                        }

                        let mut gap = match justify_content {
                            JustifyContent::Start
                            | JustifyContent::Center
                            | JustifyContent::End => 0.0,
                            JustifyContent::SpaceBetween | JustifyContent::SpaceEvenly => {
                                gaps.first().copied().unwrap_or_default()
                            }
                        };

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
                        let row_gap = parent.calculated_row_gap.unwrap_or_default();

                        for child in parent.relative_positioned_elements_mut() {
                            let Some(LayoutPosition::Wrap { row, .. }) = child.calculated_position
                            else {
                                continue;
                            };

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
                            }

                            if child_height > max_height {
                                max_height = child_height;
                            }

                            log::trace!(
                                "position_elements: setting wrapped position ({x}, {y}):\n{child}"
                            );
                            if set_float(&mut child.calculated_x, x).is_some() {
                                changed = true;
                            }
                            if set_float(&mut child.calculated_y, y).is_some() {
                                changed = true;
                            }

                            x += child_width + gap;
                        }
                    } else {
                        let mut x = 0.0;
                        let mut y = 0.0;
                        let mut col_gap = 0.0;
                        let row_gap = parent.calculated_row_gap.unwrap_or_default();

                        match justify_content {
                            JustifyContent::Start => {}
                            JustifyContent::Center => {
                                let size: f32 = parent
                                    .relative_positioned_elements()
                                    .filter_map(|x| match direction {
                                        LayoutDirection::Row => x.bounding_calculated_width(),
                                        LayoutDirection::Column => x.bounding_calculated_height(),
                                    })
                                    .sum();

                                match direction {
                                    LayoutDirection::Row => x += (container_width - size) / 2.0,
                                    LayoutDirection::Column => y += (container_height - size) / 2.0,
                                }
                            }
                            JustifyContent::End => {
                                let size: f32 = parent
                                    .relative_positioned_elements()
                                    .filter_map(|x| match direction {
                                        LayoutDirection::Row => x.bounding_calculated_width(),
                                        LayoutDirection::Column => x.bounding_calculated_height(),
                                    })
                                    .sum();

                                match direction {
                                    LayoutDirection::Row => x += container_width - size,
                                    LayoutDirection::Column => y += container_height - size,
                                }
                            }
                            JustifyContent::SpaceBetween => {
                                let count = parent.relative_positioned_elements().count();
                                let size: f32 = parent
                                    .relative_positioned_elements()
                                    .filter_map(|x| match direction {
                                        LayoutDirection::Row => x.bounding_calculated_width(),
                                        LayoutDirection::Column => x.bounding_calculated_height(),
                                    })
                                    .sum();

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
                                let count = parent.relative_positioned_elements().count();
                                let size: f32 = parent
                                    .relative_positioned_elements()
                                    .filter_map(|x| match direction {
                                        LayoutDirection::Row => x.bounding_calculated_width(),
                                        LayoutDirection::Column => x.bounding_calculated_height(),
                                    })
                                    .sum();

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
                        };

                        if let Some(text_align) = relative_container.text_align {
                            if parent
                                .relative_positioned_elements()
                                .all(|x| matches!(x.element, Element::Raw { .. }))
                            {
                                match text_align {
                                    TextAlign::Start => {}
                                    TextAlign::Center => {
                                        let size: f32 = parent
                                            .relative_positioned_elements()
                                            .filter_map(Container::bounding_calculated_width)
                                            .sum();

                                        x += (container_width - size) / 2.0;
                                    }
                                    TextAlign::End => {
                                        let size: f32 = parent
                                            .relative_positioned_elements()
                                            .filter_map(Container::bounding_calculated_width)
                                            .sum();

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
                        }

                        match align_items {
                            AlignItems::Start => {}
                            AlignItems::Center | AlignItems::End => {
                                let sizes = parent.relative_positioned_elements().filter_map(|x| {
                                    match direction {
                                        LayoutDirection::Row => x.bounding_calculated_height(),
                                        LayoutDirection::Column => x.bounding_calculated_width(),
                                    }
                                });
                                let size = match direction {
                                    LayoutDirection::Row => {
                                        sizes.max_by(order_float).unwrap_or_default()
                                    }
                                    LayoutDirection::Column => sizes.sum(),
                                };

                                match align_items {
                                    AlignItems::Start => unreachable!(),
                                    AlignItems::Center => match direction {
                                        LayoutDirection::Row => {
                                            y += (container_height - size) / 2.0;
                                        }
                                        LayoutDirection::Column => {
                                            x += (container_width - size) / 2.0;
                                        }
                                    },
                                    AlignItems::End => match direction {
                                        LayoutDirection::Row => {
                                            y += container_height - size;
                                        }
                                        LayoutDirection::Column => {
                                            x += container_width - size;
                                        }
                                    },
                                }
                            }
                        };

                        for child in parent.relative_positioned_elements_mut() {
                            log::trace!("position_elements: setting position ({x}, {y}):\n{child}");
                            if set_float(&mut child.calculated_x, x).is_some() {
                                changed = true;
                            }
                            if set_float(&mut child.calculated_y, y).is_some() {
                                changed = true;
                            }

                            match direction {
                                LayoutDirection::Row => {
                                    x += child.bounding_calculated_width().unwrap() + col_gap;
                                }
                                LayoutDirection::Column => {
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

                    for child in parent.absolute_positioned_elements_mut() {
                        if let Some(left) = &child.left {
                            let left = left.calc(width, view_width, view_height);
                            if set_float(&mut child.calculated_x, left).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(right) = &child.right {
                            let right = right.calc(width, view_width, view_height);
                            let bounding_width = child.bounding_calculated_width().unwrap();
                            let right = width - right - bounding_width;
                            if set_float(&mut child.calculated_x, right).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(top) = &child.top {
                            let top = top.calc(height, view_width, view_height);
                            if set_float(&mut child.calculated_y, top).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(bottom) = &child.bottom {
                            let bottom = bottom.calc(height, view_width, view_height);
                            let bounding_height = child.bounding_calculated_height().unwrap();
                            let bottom = height - bottom - bounding_height;
                            if set_float(&mut child.calculated_y, bottom).is_some() {
                                changed = true;
                            }
                        }

                        if child.calculated_x.is_none() {
                            child.calculated_x = Some(0.0);
                        }
                        if child.calculated_y.is_none() {
                            child.calculated_y = Some(0.0);
                        }
                    }

                    // fixed positioned

                    for child in parent.fixed_positioned_elements_mut() {
                        if let Some(left) = &child.left {
                            let left = left.calc(view_width, view_width, view_height);
                            if set_float(&mut child.calculated_x, left).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(right) = &child.right {
                            let right = right.calc(view_width, view_width, view_height);
                            let bounding_width = child.bounding_calculated_width().unwrap();
                            let right = width - right - bounding_width;
                            if set_float(&mut child.calculated_x, right).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(top) = &child.top {
                            let top = top.calc(view_height, view_width, view_height);
                            if set_float(&mut child.calculated_y, top).is_some() {
                                changed = true;
                            }
                        }
                        if let Some(bottom) = &child.bottom {
                            let bottom = bottom.calc(view_height, view_width, view_height);
                            let bounding_height = child.bounding_calculated_height().unwrap();
                            let bottom = height - bottom - bounding_height;
                            if set_float(&mut child.calculated_y, bottom).is_some() {
                                changed = true;
                            }
                        }

                        if child.calculated_x.is_none() {
                            child.calculated_x = Some(0.0);
                        }
                        if child.calculated_y.is_none() {
                            child.calculated_y = Some(0.0);
                        }
                    }
                },
            );

            changed
        }
    }
}

impl Container {
    #[must_use]
    pub fn horizontal_margin(&self) -> Option<f32> {
        let mut margin = None;
        if let Some(margin_left) = self.calculated_margin_left {
            margin = Some(margin_left);
        }
        if let Some(margin_right) = self.calculated_margin_right {
            margin.replace(margin.map_or(margin_right, |x| x + margin_right));
        }
        margin
    }

    #[must_use]
    pub fn vertical_margin(&self) -> Option<f32> {
        let mut margin = None;
        if let Some(margin_top) = self.calculated_margin_top {
            margin = Some(margin_top);
        }
        if let Some(margin_bottom) = self.calculated_margin_bottom {
            margin.replace(margin.map_or(margin_bottom, |x| x + margin_bottom));
        }
        margin
    }

    #[must_use]
    pub fn horizontal_padding(&self) -> Option<f32> {
        let mut padding = None;
        if let Some(padding_left) = self.calculated_padding_left {
            padding = Some(padding_left);
        }
        if let Some(padding_right) = self.calculated_padding_right {
            padding.replace(padding.map_or(padding_right, |x| x + padding_right));
        }
        padding
    }

    #[must_use]
    pub fn vertical_padding(&self) -> Option<f32> {
        let mut padding = None;
        if let Some(padding_top) = self.calculated_padding_top {
            padding = Some(padding_top);
        }
        if let Some(padding_bottom) = self.calculated_padding_bottom {
            padding.replace(padding.map_or(padding_bottom, |x| x + padding_bottom));
        }
        padding
    }

    #[must_use]
    pub fn horizontal_borders(&self) -> Option<f32> {
        let mut borders = None;
        if let Some((_, border_left)) = self.calculated_border_left {
            borders = Some(border_left);
        }
        if let Some((_, border_right)) = self.calculated_border_right {
            borders.replace(borders.map_or(border_right, |x| x + border_right));
        }
        borders
    }

    #[must_use]
    pub fn vertical_borders(&self) -> Option<f32> {
        let mut borders = None;
        if let Some((_, border_top)) = self.calculated_border_top {
            borders = Some(border_top);
        }
        if let Some((_, border_bottom)) = self.calculated_border_bottom {
            borders.replace(borders.map_or(border_bottom, |x| x + border_bottom));
        }
        borders
    }

    #[must_use]
    pub fn calculated_width_minus_borders(&self) -> Option<f32> {
        self.calculated_width.map(|x| {
            self.horizontal_borders().map_or(x, |borders| {
                let x = x - borders;
                if x < 0.0 { 0.0 } else { x }
            })
        })
    }

    #[must_use]
    pub fn calculated_height_minus_borders(&self) -> Option<f32> {
        self.calculated_height.map(|x| {
            self.vertical_borders().map_or(x, |borders| {
                let x = x - borders;
                if x < 0.0 { 0.0 } else { x }
            })
        })
    }

    #[must_use]
    pub fn bounding_calculated_width(&self) -> Option<f32> {
        self.calculated_width.map(|width| {
            width
                + self.horizontal_padding().unwrap_or(0.0)
                + self.scrollbar_right.unwrap_or(0.0)
                + self.horizontal_margin().unwrap_or(0.0)
        })
    }

    #[must_use]
    pub fn bounding_calculated_height(&self) -> Option<f32> {
        self.calculated_height.map(|height| {
            height
                + self.vertical_padding().unwrap_or(0.0)
                + self.scrollbar_bottom.unwrap_or(0.0)
                + self.vertical_margin().unwrap_or(0.0)
        })
    }
}

#[cfg(test)]
mod test {
    use maud::html;
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

    use super::Calculator;

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

    impl FontMetrics for DefaultFontMetrics {
        fn measure_text(&self, text: &str, size: f32, wrap_width: f32) -> FontMetricsBounds {
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

            FontMetricsBounds { rows }
        }
    }

    static CALCULATOR: Calculator<DefaultFontMetrics> = Calculator::new(DefaultFontMetrics);

    mod scrollbar {
        use super::*;

        #[test_log::test]
        #[ignore]
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(1600.0);
            container.calculated_height = Some(1000.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
        #[ignore]
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
            log::trace!("container:\n{}", container);

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

    mod table {
        use super::*;

        #[test_log::test]
        #[ignore]
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
                                    element: Element::TD,
                                    children: vec![Container {
                                        element: Element::Div,
                                        width: Some(Number::Integer(40)),
                                        height: Some(Number::Integer(10)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
                                    element: Element::TD,
                                    children: vec![Container {
                                        element: Element::Div,
                                        width: Some(Number::Integer(10)),
                                        height: Some(Number::Integer(40)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
            log::trace!("container:\n{}", container);

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
        #[ignore]
        fn calc_can_calc_table_column_and_row_sizes_and_expand_to_fill_width() {
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD,
                                    children: vec![Container {
                                        width: Some(Number::Integer(40)),
                                        height: Some(Number::Integer(10)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
                                    element: Element::TD,
                                    children: vec![Container {
                                        width: Some(Number::Integer(10)),
                                        height: Some(Number::Integer(40)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
            log::trace!("container:\n{}", container);

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
                                        calculated_width: Some(50.0),
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
                                        calculated_width: Some(50.0),
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
                                        calculated_width: Some(50.0),
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
                                        calculated_width: Some(50.0),
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
        #[ignore]
        fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells() {
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD,
                                    children: vec![Container {
                                        width: Some(Number::Integer(40)),
                                        height: Some(Number::Integer(10)),
                                        ..Default::default()
                                    }],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
                                    element: Element::TD,
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
            log::trace!("container:\n{}", container);

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
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(10.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        calculated_width: Some(50.0),
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
                                        calculated_width: Some(50.0),
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
                                        calculated_width: Some(50.0),
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
        #[ignore]
        fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells_when_all_are_unsized()
         {
            let mut container = Container {
                children: vec![Container {
                    element: Element::Table,
                    children: vec![
                        Container {
                            element: Element::TR,
                            direction: LayoutDirection::Row,
                            children: vec![
                                Container {
                                    element: Element::TD,
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
                                    element: Element::TD,
                                    children: vec![Container::default()],
                                    ..Container::default()
                                },
                                Container {
                                    element: Element::TD,
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
            log::trace!("container:\n{}", container);

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
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[0].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(25.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[1].children[0].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: vec![Container {
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[1].children[1].children
                                                [0]
                                            .clone()
                                        }],
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(25.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(25.0 + 25.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[ignore]
        fn calc_can_calc_table_column_and_row_sizes_and_auto_size_raw_data() {
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(80.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[0].children[0].clone()
                                    },
                                    Container {
                                        children: container.children[0].children[0].children[1]
                                            .children
                                            .clone(),
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[0].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(25.0),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                children: vec![
                                    Container {
                                        children: container.children[0].children[1].children[0]
                                            .children
                                            .clone(),
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        children: container.children[0].children[1].children[1]
                                            .children
                                            .clone(),
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(25.0),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(25.0 + 25.0),
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[ignore]
        fn calc_can_calc_table_column_and_row_sizes_with_tbody() {
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
                                        element: Element::TD,
                                        children: vec![Container {
                                            element: Element::Raw {
                                                value: "test".to_string(),
                                            },
                                            ..Container::default()
                                        }],
                                        ..Container::default()
                                    },
                                    Container {
                                        element: Element::TD,
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
                                        element: Element::TD,
                                        children: vec![Container {
                                            element: Element::Raw {
                                                value: "test".to_string(),
                                            },
                                            ..Container::default()
                                        }],
                                        ..Container::default()
                                    },
                                    Container {
                                        element: Element::TD,
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
            log::trace!("container:\n{}", container);

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
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            children: container.children[0].children[0].children[0]
                                                .children[1]
                                                .children
                                                .clone(),
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[0].children[0].children
                                                [1]
                                            .clone()
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(25.0),
                                    ..container.children[0].children[0].children[0].clone()
                                },
                                Container {
                                    children: vec![
                                        Container {
                                            children: container.children[0].children[0].children[1]
                                                .children[0]
                                                .children
                                                .clone(),
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[0].children[1].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            children: container.children[0].children[0].children[1]
                                                .children[1]
                                                .children
                                                .clone(),
                                            calculated_width: Some(50.0),
                                            calculated_height: Some(25.0),
                                            ..container.children[0].children[0].children[1].children
                                                [1]
                                            .clone()
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(25.0),
                                    ..container.children[0].children[0].children[1].clone()
                                },
                            ],
                            calculated_width: Some(100.0),
                            calculated_height: Some(25.0 + 25.0),
                            ..container.children[0].children[0].clone()
                        }],
                        ..container.children[0].clone()
                    }],
                    ..container
                },
            );
        }

        #[test_log::test]
        #[ignore]
        fn calc_calculates_table_td_height_correctly() {
            let mut container: Container = html! {
                table {
                    tr {
                        td sx-height=(30) {}
                    }
                }
            }
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
        #[ignore]
        fn calc_calculates_table_tr_height_correctly() {
            let mut container: Container = html! {
                table {
                    tr {
                        td sx-height=(30) {}
                    }
                }
            }
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
        #[ignore]
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
        #[ignore]
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
            .try_into()
            .unwrap();

            container.overflow_y = LayoutOverflow::Squash;
            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
        #[ignore]
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(50.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
        #[ignore]
        fn calc_calculates_table_column_widths_the_same_across_headers_and_body() {
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(1232.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

            compare_containers(
                &container.clone(),
                &Container {
                    children: vec![Container {
                        children: vec![
                            Container {
                                children: vec![Container {
                                    children: vec![
                                        Container {
                                            calculated_width: Some(616.0),
                                            ..container.children[0].children[0].children[0].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            calculated_width: Some(616.0),
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
                                            calculated_width: Some(596.0),
                                            ..container.children[0].children[1].children[0].children
                                                [0]
                                            .clone()
                                        },
                                        Container {
                                            calculated_width: Some(596.0),
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
        #[ignore]
        fn calc_calculates_table_th_sizes_with_padding_taken_into_account() {
            let mut container: Container = html! {
                table {
                    thead {
                        tr {
                            th sx-padding-x=(10) sx-padding-y=(15) {}
                        }
                    }
                }
            }
            .try_into()
            .unwrap();

            container.calculated_width = Some(1232.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                children: vec![Container {
                                    calculated_width: Some(1212.0),
                                    calculated_height: Some(25.0),
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
        #[ignore]
        fn calc_calculates_table_td_sizes_with_padding_taken_into_account() {
            let mut container: Container = html! {
                table {
                    tr {
                        td sx-padding-x=(10) sx-padding-y=(15) {}
                    }
                }
            }
            .try_into()
            .unwrap();

            container.calculated_width = Some(1232.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

            compare_containers(
                &container,
                &Container {
                    children: vec![Container {
                        children: vec![Container {
                            children: vec![Container {
                                calculated_width: Some(1212.0),
                                calculated_height: Some(25.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(100.0),
                    calculated_height: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(50.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_width: Some(50.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(50.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    calculated_width: Some(100.0),
                    calculated_height: Some(40.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(20.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(20.0),
                                calculated_x: Some(50.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);
        container.direction = LayoutDirection::Row;
        container.overflow_x = LayoutOverflow::Squash;
        container.overflow_y = LayoutOverflow::Squash;

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                ..container.children[1].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(25.0),
                                calculated_y: Some(0.0),
                                ..container.children[1].children[1].clone()
                            },
                        ],
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(50.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
    fn handle_overflow_y_squash_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        const ROW_HEIGHT: f32 = 40.0 / 4.0;

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
        log::trace!("first container:\n{}", container);

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 3.0),
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
    fn handle_overflow_y_expand_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        const ROW_HEIGHT: f32 = 10.0;

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
        log::trace!("first container:\n{}", container);

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 3.0),
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
        const ROW_HEIGHT: f32 = 40.0 / 3.0;

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(75.0 - 20.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT.mul_add(1.0, 10.0)),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(75.0 - 20.0),
                        calculated_y: Some(ROW_HEIGHT.mul_add(1.0, 10.0)),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT.mul_add(2.0, 10.0 + 10.0)),
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
        const ROW_HEIGHT: f32 = 40.0 / 3.0;

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
        log::trace!("first container:\n{}", container);

        let mut actual = container.clone();
        let expected = Container {
            children: vec![
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..container.children[0].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(75.0 - 20.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..container.children[1].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(0.0),
                    calculated_y: Some(ROW_HEIGHT.mul_add(1.0, 10.0)),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..container.children[2].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(75.0 - 20.0),
                    calculated_y: Some(ROW_HEIGHT.mul_add(1.0, 10.0)),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..container.children[3].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(0.0),
                    calculated_y: Some(ROW_HEIGHT.mul_add(2.0, 10.0 + 10.0)),
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
        log::trace!("second container:\n{}", actual);

        compare_containers(&actual, &expected);
    }

    #[test_log::test]
    fn handles_justify_content_space_evenly_and_wraps_elements_properly() {
        const ROW_HEIGHT: f32 = 40.0 / 2.0;

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(75.0);
        container.calculated_height = Some(40.0);
        container.direction = LayoutDirection::Row;
        container.overflow_x = LayoutOverflow::Wrap { grid: true };
        container.justify_content = Some(JustifyContent::SpaceEvenly);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        const ROW_HEIGHT: f32 = 40.0 / 4.0;

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
        log::trace!("first container:\n{}", container);

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 3.0),
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
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        const ROW_HEIGHT: f32 = 40.0 / 4.0;

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
        log::trace!("first container:\n{}", container);

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second calc");
        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 3.0),
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
        const ROW_HEIGHT: f32 = 40.0 / 3.0;

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(43.333_336),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(ROW_HEIGHT + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(43.333_336),
                        calculated_y: Some(ROW_HEIGHT + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(ROW_HEIGHT.mul_add(2.0, 10.0 + 10.0)),
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
        const ROW_HEIGHT: f32 = 40.0 / 3.0;

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
        log::trace!("first container:\n{}", container);

        let mut actual = container.clone();
        let expected = Container {
            children: vec![
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..container.children[0].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(43.333_336),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..container.children[1].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(ROW_HEIGHT + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..container.children[2].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(43.333_336),
                    calculated_y: Some(ROW_HEIGHT + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..container.children[3].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(ROW_HEIGHT),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(ROW_HEIGHT.mul_add(2.0, 10.0 + 10.0)),
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
        log::trace!("second container:\n{}", actual);

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_min_width: Some(25.0),
                    calculated_width: Some(50.0),
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
        log::trace!("first container:\n{}", container);

        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_min_width: Some(25.0),
                    calculated_width: Some(50.0),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        const ROW_HEIGHT: f32 = 40.0;

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(25.0),
                        calculated_y: Some(ROW_HEIGHT),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(25.0),
                        calculated_height: Some(ROW_HEIGHT),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
                            calculated_height: Some(20.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(20.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(20.0),
                                calculated_x: Some(50.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                direction: LayoutDirection::Row,
                                children: vec![
                                    Container {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(25.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(70.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..container.children[0].children[0].clone()
                            },
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(70.0),
                                calculated_x: Some(50.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                direction: LayoutDirection::Row,
                                children: vec![
                                    Container {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(70.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[0].clone()
                                    },
                                    Container {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(70.0),
                                        calculated_x: Some(25.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..container.children[0].children[1].children[1].clone()
                                    },
                                ],
                                ..container.children[0].children[1].clone()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(70.0),
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
                        calculated_y: Some(70.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_height: Some(50.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(100.0),
                            calculated_height: Some(50.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(25.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![Container {
                            calculated_width: Some(0.0),
                            calculated_height: Some(0.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            position: Some(Position::Fixed),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(100.0),
                        calculated_height: Some(25.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(25.0),
                        position: Some(Position::Relative),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        internal_margin_left: None,
                        internal_margin_right: None,
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        internal_margin_left: None,
                        internal_margin_right: None,
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(35.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(35.0),
                        calculated_padding_right: Some(30.0),
                        calculated_x: Some(35.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(35.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(35.0),
                        calculated_margin_right: Some(30.0),
                        calculated_x: Some(35.0),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
                        calculated_width: Some(40.0),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(30.0),
                        calculated_padding_right: Some(20.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(30.0),
                        calculated_x: Some(50.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(30.0),
                        calculated_x: Some(80.0),
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
        log::trace!("container:\n{}", container);

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

                        calculated_width: Some(30.0),
                        calculated_x: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(30.0),
                        calculated_x: Some(30.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(30.0),
                        calculated_x: Some(60.0),
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
        let text_width = metrics.measure_text("test", 14.0, f32::INFINITY).width();

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![
                    container.children[0].clone(),
                    Container {
                        calculated_width: Some(100.0 - text_width),
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_height: Some(20.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(35.0),
                    calculated_height: Some(35.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        calculated_width: Some(32.5),
                        calculated_height: Some(34.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_padding_left: Some(2.0),
                        calculated_padding_right: Some(3.0),
                        calculated_padding_top: Some(1.0),
                        ..container.children[0].children[0].clone()
                    }],
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
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![Container {
                        calculated_width: Some(32.5),
                        calculated_height: Some(34.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_padding_left: Some(2.0),
                        calculated_padding_right: Some(3.0),
                        calculated_padding_top: Some(1.0),
                        ..container.children[0].children[0].clone()
                    }],
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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(160.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(160.0);
        container.calculated_height = Some(100.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
                    main sx-overflow-y="auto" {
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
                        calculated_height: Some(61.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.position = Some(Position::Relative);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.position = Some(Position::Relative);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.position = Some(Position::Relative);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
                    main sx-overflow-y="auto" {}
                }
            }
        }
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
                    main sx-overflow-y="auto" {}
                }
                footer sx-height=(140) {}
            }
        }
        .try_into()
        .unwrap();

        container.calculated_width = Some(1600.0);
        container.calculated_height = Some(1000.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_height: Some(410.0),
                            ..aside.children[0].children[0].clone()
                        },
                        Container {
                            element: Element::UnorderedList,
                            children: vec![
                                Container {
                                    element: Element::ListItem,
                                    calculated_height: Some(205.0),
                                    ..aside.children[0].children[1].children[0].clone()
                                },
                                Container {
                                    element: Element::ListItem,
                                    calculated_height: Some(205.0),
                                    ..aside.children[0].children[1].children[1].clone()
                                },
                            ],
                            calculated_height: Some(410.0),
                            ..aside.children[0].children[1].clone()
                        },
                    ],
                    ..aside.children[0].clone()
                }],

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_min_width: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_min_width: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(450.0),
                    calculated_min_height: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(450.0),
                    calculated_min_height: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_min_width: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_min_width: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_min_height: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_min_height: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_min_width: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(50.0),
                    calculated_min_width: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(450.0),
                    calculated_min_height: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(450.0),
                    calculated_min_height: Some(50.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_min_width: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_width: Some(25.0),
                    calculated_min_width: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_min_height: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(500.0);
        container.justify_content = Some(JustifyContent::Start);

        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    calculated_height: Some(25.0),
                    calculated_min_height: Some(75.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);
        container.justify_content = Some(JustifyContent::Start);
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

        compare_containers(
            &container,
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(200.0),
                            calculated_height: Some(100.0),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_width: Some(200.0),
                            calculated_height: Some(100.0),
                            ..container.children[0].children[1].clone()
                        },
                    ],
                    calculated_width: Some(400.0),
                    calculated_height: Some(100.0),
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
        .try_into()
        .unwrap();

        container.calculated_width = Some(400.0);
        container.calculated_height = Some(100.0);
        CALCULATOR.calc(&mut container);
        log::trace!("container:\n{}", container);

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

    mod text {
        use super::*;

        #[test_log::test]
        fn does_calculate_text_height_properly() {
            let mut container: Container = html! {
                div { "test" }
            }
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
    }

    mod positioning {
        use hyperchad_transformer_models::{AlignItems, TextAlign};

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
    }
}
