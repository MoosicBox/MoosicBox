use bumpalo::Bump;

use crate::Container;

use super::{Calc, font::FontMetrics};

#[derive(Debug, Clone, Copy)]
pub struct CalculatorDefaults {
    pub font_size: f32,
    pub font_margin_top: f32,
    pub font_margin_bottom: f32,
    pub h1_font_size: f32,
    pub h1_font_margin_top: f32,
    pub h1_font_margin_bottom: f32,
    pub h2_font_size: f32,
    pub h2_font_margin_top: f32,
    pub h2_font_margin_bottom: f32,
    pub h3_font_size: f32,
    pub h3_font_margin_top: f32,
    pub h3_font_margin_bottom: f32,
    pub h4_font_size: f32,
    pub h4_font_margin_top: f32,
    pub h4_font_margin_bottom: f32,
    pub h5_font_size: f32,
    pub h5_font_margin_top: f32,
    pub h5_font_margin_bottom: f32,
    pub h6_font_size: f32,
    pub h6_font_margin_top: f32,
    pub h6_font_margin_bottom: f32,
}

pub struct Calculator<F: FontMetrics> {
    font_metrics: F,
    defaults: CalculatorDefaults,
}

impl<F: FontMetrics> Calculator<F> {
    pub const fn new(font_metrics: F, defaults: CalculatorDefaults) -> Self {
        Self {
            font_metrics,
            defaults,
        }
    }
}

impl<F: FontMetrics> Calc for Calculator<F> {
    #[allow(clippy::let_and_return)]
    fn calc(&self, container: &mut Container) -> bool {
        use passes::pass_flex_height::Pass as _;
        use passes::pass_flex_width::Pass as _;
        use passes::pass_heights::Pass as _;
        use passes::pass_margin_and_padding::Pass as _;
        use passes::pass_positioning::Pass as _;
        use passes::pass_widths::Pass as _;
        use passes::pass_wrap_horizontal::Pass as _;

        log::trace!("calc: container={container}");

        let bfs = container.bfs();
        let arena = Bump::new();

        let changed = false;

        let changed = self.calc_widths(&bfs, container) || changed;
        let changed = self.calc_margin_and_padding(&bfs, container) || changed;
        let changed = self.flex_width(&bfs, container) || changed;
        let changed = self.wrap_horizontal(&bfs, container) || changed;
        let changed = self.calc_heights(&bfs, container) || changed;
        let changed = self.flex_height(&bfs, container) || changed;
        let changed = self.position_elements(&arena, &bfs, container) || changed;

        changed
    }
}

macro_rules! calc_size_on_axis {
    (
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

        moosicbox_logging::debug_or_trace!(("{LABEL}"), ("{LABEL}:\n{}", $container));

        let root_id = $container.id;
        let defaults = $self.defaults;
        let view_width = $container.calculated_width.expect("Missing view_width");
        let view_height = $container.calculated_height.expect("Missing view_height");

        let mut changed = false;

        $bfs.traverse_rev_with_parents_ref_mut(
            true,
            Container::default(),
            $container,
            |parent, mut context| {
                if parent.id == root_id {
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
                        if set_float(&mut paste!(child.[<calculated_max_ $size>]), size).is_some() {
                            changed = true;
                        }
                    }
                    if let Some(min) = paste!(child.[<min_ $size>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = min.calc(0.0, view_width, view_height);
                        log::trace!("{LABEL}: calculated_min_size={size}");
                        if set_float(&mut paste!(child.[<calculated_min_ $size>]), size).is_some() {
                            changed = true;
                        }
                    }

                    let (mut min, mut preferred) = if let Some(size) = child.$size.as_ref().and_then(Number::as_fixed) {
                        let new_size = size.calc(0.0, view_width, view_height);

                        if set_float(&mut paste!(child.[<calculated_ $size>]), new_size).is_some() {
                            changed = true;
                        }
                        (Some(new_size), new_size)
                    } else if let (LayoutDirection::Row, crate::Element::Raw { value }) = (LayoutDirection::$axis, &child.element) {
                        log::trace!("{LABEL}: measuring text={value}");
                        let bounds = $self.font_metrics.measure_text(
                            value,
                            context.calculated_font_size.expect("Missing calculated_font_size"),
                            f32::INFINITY
                        );
                        log::trace!("{LABEL}: measured bounds={bounds:?}");
                        let new_size = bounds.$size();
                        log::trace!("{LABEL}: measured size={new_size}");

                        if set_float(&mut paste!(child.[<calculated_ $size>]), new_size).is_some() {
                            changed = true;
                        }
                        if LayoutDirection::$axis == LayoutDirection::Column {
                            (Some(new_size), new_size)
                        } else {
                            (None, new_size)
                        }
                    } else if let Some(size) = paste!(child.[<calculated_preferred_ $size>]) {
                        set_float(&mut paste!(child.[<calculated_ $size>]), size);
                        (paste!(child.[<calculated_child_min_ $size>]), size)
                    } else if let Some(size) = paste!(child.[<calculated_child_min_ $size>]) {
                        set_float(&mut paste!(child.[<calculated_ $size>]), size);
                        (Some(size), size)
                    } else {
                        set_float(&mut paste!(child.[<calculated_ $size>]), 0.0);
                        (None, 0.0)
                    };

                    if let Some(calculated_max) = paste!(child.[<calculated_max_ $size>]) {
                        if let Some(existing) = &mut paste!(child.[<calculated_ $size>]) {
                            if *existing > calculated_max {
                                *existing = calculated_max;
                                preferred = calculated_max;
                            }
                        } else {
                            set_float(&mut paste!(child.[<calculated_ $size>]), calculated_max);
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
                            set_float(&mut paste!(child.[<calculated_ $size>]), calculated_min);
                            preferred = calculated_min;
                        }
                    }

                    if let Some(margin) = paste!(child.[<margin_ $x>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = margin.calc(0.0, view_width, view_height);
                        if set_float(&mut paste!(child.[<calculated_margin_ $x>]), size).is_some() {
                            changed = true;
                        }
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some(margin) = paste!(child.[<margin_ $y>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = margin.calc(0.0, view_width, view_height);
                        if set_float(&mut paste!(child.[<calculated_margin_ $y>]), size).is_some() {
                            changed = true;
                        }
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some(padding) = paste!(child.[<padding_ $x>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = padding.calc(0.0, view_width, view_height);
                        if set_float(&mut paste!(child.[<calculated_padding_ $x>]), size).is_some() {
                            changed = true;
                        }
                        preferred += size;
                        crate::layout::increase_opt(&mut min, size);
                    }
                    if let Some(padding) = paste!(child.[<padding_ $y>]).as_ref().and_then(crate::Number::as_fixed) {
                        let size = padding.calc(0.0, view_width, view_height);
                        if set_float(&mut paste!(child.[<calculated_padding_ $y>]), size).is_some() {
                            changed = true;
                        }
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
                                changed = true;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                                changed = true;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $x>]) = Some((color, size));
                            changed = true;
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
                                changed = true;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                                changed = true;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $y>]) = Some((color, size));
                            changed = true;
                        }
                    }

                    let handle_auto_sizing = |child: &mut Container, size, output: &mut f32| {
                        enum AutoMinSizeHandling {
                            Add,
                            Max,
                        }

                        let handling = match child.position.unwrap_or_default() {
                            Position::Static | Position::Relative | Position::Sticky => match overflow {
                                LayoutOverflow::Auto
                                | LayoutOverflow::Scroll
                                | LayoutOverflow::Expand
                                | LayoutOverflow::Squash => {
                                    Some(match direction {
                                        LayoutDirection::$axis => AutoMinSizeHandling::Add,
                                        LayoutDirection::$cross_axis => AutoMinSizeHandling::Max,
                                    })
                                }
                                LayoutOverflow::Wrap { .. } => {
                                    Some(AutoMinSizeHandling::Max)
                                }
                                LayoutOverflow::Hidden => None
                            },
                            Position::Absolute | Position::Fixed => None
                        };

                        if let Some(handling) = handling {
                            match handling {
                                AutoMinSizeHandling::Add => {
                                    log::trace!("{LABEL}: AutoMinSizeHandling::Add output={output} += size={size} ({})", *output + size);
                                    *output += size;
                                }
                                AutoMinSizeHandling::Max => {
                                    log::trace!("{LABEL}: AutoMinSizeHandling::Max size={size} > output={output} ({})", if size > *output { size } else { *output });
                                    if size > *output {
                                        *output = size;
                                    }
                                }
                            }
                        }
                    };

                    paste!(child.[<calculated_preferred_ $size>]) = Some(preferred);
                    handle_auto_sizing(child, preferred, &mut preferred_size);

                    if let Some(size) = min {
                        paste!(child.[<calculated_child_min_ $size>]) = Some(size);
                        handle_auto_sizing(child, size, &mut min_size);
                    }
                }

                set_float(&mut paste!(parent.[<calculated_child_min_ $size>]), min_size);
                set_float(&mut paste!(parent.[<calculated_preferred_ $size>]), preferred_size);
            });

        changed
    }};
}

macro_rules! flex_on_axis {
    (
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

        use crate::{Element, LayoutOverflow, float_eq};

        const LABEL: &str = $label;

        moosicbox_logging::debug_or_trace!(("{LABEL}"), ("{LABEL}:\n{}", $container));

        let mut changed = false;

        let root_id = $container.id;
        let view_width = $container.calculated_width.expect("Missing view_width");
        let view_height = $container.calculated_height.expect("Missing view_height");

        #[allow(clippy::cognitive_complexity)]
        $bfs.traverse_with_parents_ref_mut(
            true,
            crate::layout::Rect::default(),
            $container,
            |parent, relative_container| {
                if parent.id == root_id {
                    crate::layout::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: view_width,
                        height: view_height,
                    }
                } else if parent.position == Some(Position::Relative) {
                    crate::layout::Rect {
                        $size: paste!(parent.[<calculated_ $size>]).expect("Missing parent calculated size"),
                        ..Default::default()
                    }
                } else {
                    relative_container
                }
            },
            |parent, relative_container| {
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
                        if set_float(&mut paste!(child.[<calculated_min_ $size>]), size).is_some() {
                            changed = true;
                        }
                    }
                    if let Some(max) = paste!(child.[<max_ $size>]).as_ref().and_then(crate::Number::as_dynamic) {
                        let size = max.calc(container_size, view_width, view_height);
                        log::trace!("{LABEL}: calculated_max_size={size}");
                        if set_float(&mut paste!(child.[<calculated_max_ $size>]), size).is_some() {
                            changed = true;
                        }

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
                                changed = true;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                                changed = true;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $x>]) = Some((color, size));
                            changed = true;
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
                                changed = true;
                            }
                            if !float_eq!(calculated.1, size) {
                                calculated.1 = size;
                                changed = true;
                            }
                        } else {
                            paste!(child.[<calculated_border_ $y>]) = Some((color, size));
                            changed = true;
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
                            if set_float(&mut paste!(child.[<calculated_ $size>]), size).is_some() {
                                changed = true;
                            }
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

                        let mut size_cross_axis = |parent: &mut Container| {
                            log::trace!("{LABEL}: size_cross_axis\n{parent}");

                            for child in parent.relative_positioned_elements_mut() {
                                if matches!(child.element, Element::Raw { .. }) {
                                    continue;
                                }

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

                                if child.$size.is_none() && set_float(&mut paste!(child.[<calculated_ $size>]), new_size).is_some() {
                                    changed = true;
                                }
                            }
                        };

                        log::trace!("{LABEL}: remaining_size={remaining_size}\n{parent}");

                        // Fit all unsized children to remaining_size
                        match direction {
                            LayoutDirection::$axis => {
                                if parent.is_flex_container() {
                                    if remaining_size <= -crate::layout::EPSILON
                                        && parent
                                            .relative_positioned_elements()
                                            .any(|x| x.$size.is_none() && paste!(x.[<can_shrink_ $size>]()))
                                    {
                                        loop {
                                            let mut largest = f32::INFINITY;
                                            let mut target = f32::INFINITY;
                                            let mut largest_count = 0_u16;

                                            for size in parent
                                                .relative_positioned_elements()
                                                .filter(|x| x.$size.is_none() && paste!(x.[<can_shrink_ $size>]()))
                                                .filter_map(|x| paste!(x.[<calculated_ $size>]))
                                            {
                                                if size > largest {
                                                    target = largest;
                                                    largest = size;
                                                    largest_count = 1;
                                                } else if float_eq!(largest, size) {
                                                    largest_count += 1;
                                                } else if target < size {
                                                    target = size;
                                                }
                                            }

                                            moosicbox_assert::assert!(largest_count > 0, "expected at least one largest item");
                                            moosicbox_assert::assert!(largest.is_finite(), "expected largest to be finite");

                                            let largest_countf = f32::from(largest_count);

                                            let last_iteration = if target.is_infinite() {
                                                log::trace!("{LABEL}: last iteration remaining_size={remaining_size}");
                                                target = largest + if largest_count == 1 {
                                                    remaining_size
                                                } else {
                                                    remaining_size / largest_countf
                                                };
                                                remaining_size = 0.0;
                                                true
                                            } else if target > remaining_size {
                                                log::trace!("{LABEL}: target={target} > remaining_size={remaining_size}");
                                                target = if largest_count == 1 {
                                                    remaining_size
                                                } else {
                                                    remaining_size / largest_countf
                                                };
                                                remaining_size = 0.0;
                                                true
                                            } else {
                                                remaining_size -= (target - largest) * largest_countf;
                                                float_eq!(remaining_size, 0.0)
                                            };

                                            log::trace!("{LABEL}: target={target} largest={largest} largest_count={largest_count} remaining_size={remaining_size} container_size={container_size}");

                                            moosicbox_assert::assert!(target.is_finite(), "expected target to be finite");

                                            let mut dynamic_child_size = false;

                                            for child in parent
                                                .relative_positioned_elements_mut()
                                                .filter(|x| x.$size.is_none() && paste!(x.[<can_shrink_ $size>]()))
                                                .filter(|x| paste!(x.[<calculated_ $size>]).is_some_and(|x| float_eq!(x, largest)))
                                            {
                                                let mut clipped = false;
                                                let mut target = target;

                                                if let Some(max) = paste!(child.[<calculated_max_ $size>]) {
                                                    log::trace!("{LABEL}: calculated_max={max}");
                                                    let max = max - paste!(child.[<padding_ $unit>]()).unwrap_or_default() - paste!(child.[<margin_ $unit>]()).unwrap_or_default();
                                                    log::trace!("{LABEL}: calculated_max={max} without padding/margins");
                                                    if target > max {
                                                        remaining_size -= max - target;
                                                        target = max;
                                                    }
                                                }
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
                                                set_float(&mut paste!(child.[<calculated_ $size>]), target);
                                                log::trace!("{LABEL}: increasing child size prev={prev} to target={target}:\n{child}");
                                            }

                                            if last_iteration || !dynamic_child_size {
                                                break;
                                            }
                                        }
                                    } else if !float_eq!(remaining_size, 0.0)
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
                                                remaining_size -= (target - smallest) * smallest_countf;
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
                                                set_float(&mut paste!(child.[<calculated_ $size>]), target);
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
                                if parent.align_items.is_none() {
                                    size_cross_axis(parent);
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
                        if set_float(&mut paste!(child.[<calculated_ $size>]), size).is_some() {
                            changed = true;
                        }
                    } else if set_float(&mut paste!(child.[<calculated_ $size>]), remaining_container_size).is_some() {
                        changed = true;
                    }
                }

                for child in parent.fixed_positioned_elements_mut() {
                    let mut remaining_container_size = *relative_size;

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
                        if set_float(&mut paste!(child.[<calculated_ $size>]), size).is_some() {
                            changed = true;
                        }
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

            moosicbox_logging::debug_or_trace!(("{LABEL}"), ("{LABEL}:\n{}", $container));

            let view_width = $container.calculated_width.expect("Missing view_width");
            let view_height = $container.calculated_height.expect("Missing view_height");

            let mut changed = true;

            $bfs.traverse_mut($container, |parent| {
                let container_width = parent.calculated_width.expect("Missing parent calculated_width");

                for child in &mut parent.children {
                    $each_child(child, container_width, view_width, view_height);
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

                    if set_value(&mut child.calculated_position, position).is_some() {
                        changed = true;
                    }
                }
            });

            changed
        }
    }};
}

mod passes {
    use crate::Container;

    /// # Pass 1: Widths
    ///
    /// This pass traverses the `Container` children in reverse BFS (Breadth-First Search)
    /// and calculates the widths required for each of the `Container`s.
    pub mod pass_widths {
        use paste::paste;

        use crate::{
            BfsPaths, Container, Element, HeaderSize, Number,
            layout::{
                calc::{Calculator, CalculatorDefaults},
                font::FontMetrics,
                set_float,
            },
        };

        pub trait Pass {
            fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
        }

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

                if self.margin_top.is_none() {
                    if let Element::Heading { size } = self.element {
                        self.calculated_margin_top =
                            Some(default_heading_prop!(size, font_margin_top));
                    }
                }

                if self.margin_bottom.is_none() {
                    if let Element::Heading { size } = self.element {
                        self.calculated_margin_bottom =
                            Some(default_heading_prop!(size, font_margin_bottom));
                    }
                }
            }

            fn calc_fixed_properties(&mut self, view_width: f32, view_height: f32) -> bool {
                let mut changed = false;

                macro_rules! update_prop {
                    ($value:expr, $prop:ident, $basis:expr $(,)?) => {{
                        let value = $value;
                        let size = value.calc($basis, view_width, view_height);
                        if set_float(&mut paste!(self.[<calculated_ $prop>]), size).is_some() {
                            log::trace!("calc_fixed_properties: updated from={value} to calculated_{}={size}", stringify!($prop));
                            changed = true;
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

                changed
            }
        }

        impl<F: FontMetrics> Pass for Calculator<F> {
            fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
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
                )
            }
        }
    }

    pub mod pass_margin_and_padding {
        use paste::paste;

        use crate::{
            BfsPaths, Container,
            layout::{calc::Calculator, font::FontMetrics, set_float},
        };

        pub trait Pass {
            fn calc_margin_and_padding(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
        }

        impl<F: FontMetrics> Pass for Calculator<F> {
            #[allow(clippy::too_many_lines)]
            fn calc_margin_and_padding(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
                log::trace!("calc_margin_and_padding:\n{container}");

                let view_width = container.calculated_width.expect("Missing view_width");
                let view_height = container.calculated_height.expect("Missing view_height");

                let mut changed = false;

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
                        macro_rules! update_size {
                            ($prop:ident $(,)?) => {
                                if let Some(value) =
                                    child.$prop.as_ref().and_then(crate::Number::as_dynamic)
                                {
                                    let size = value.calc(container_width, view_width, view_height);
                                    if set_float(&mut paste!(child.[<calculated_ $prop>]), size).is_some() {
                                        log::trace!("calc_margin_and_padding: updated from={value} to calculated_{}={size}", stringify!($prop));
                                        changed = true;
                                    }
                                }
                            };
                        }

                        // dynamic borders
                        update_size!(border_top_left_radius);
                        update_size!(border_top_right_radius);
                        update_size!(border_bottom_left_radius);
                        update_size!(border_bottom_right_radius);

                        // dynamic margin
                        update_size!(margin_left);
                        update_size!(margin_right);
                        update_size!(margin_top);
                        update_size!(margin_bottom);

                        // dynamic padding
                        update_size!(padding_left);
                        update_size!(padding_right);
                        update_size!(padding_top);
                        update_size!(padding_bottom);
                    }
                });

                changed
            }
        }
    }

    pub mod pass_flex_width {
        use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

        use crate::{
            BfsPaths, Container,
            layout::{calc::Calculator, font::FontMetrics, set_float},
        };

        pub trait Pass {
            fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
        }

        impl<F: FontMetrics> Pass for Calculator<F> {
            #[allow(clippy::too_many_lines)]
            fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
                flex_on_axis!(
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
                )
            }
        }
    }

    pub mod pass_wrap_horizontal {
        use crate::{
            BfsPaths, Container, Element,
            layout::{calc::Calculator, font::FontMetrics, set_float, set_value},
        };

        pub trait Pass {
            fn wrap_horizontal(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
        }

        impl<F: FontMetrics> Pass for Calculator<F> {
            fn wrap_horizontal(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
                let each_child = |container: &mut Container,
                                  container_width,
                                  _view_width,
                                  _view_height| {
                    let Element::Raw { value } = &container.element else {
                        return;
                    };

                    let font_size = container.calculated_font_size.expect("Missing font_size");

                    log::trace!(
                        "wrap_horizontal: measuring text={value} container_width={container_width}"
                    );
                    let bounds = self
                        .font_metrics
                        .measure_text(value, font_size, container_width);
                    log::trace!("wrap_horizontal: measured bounds={bounds:?}");
                    let new_width = bounds.width();
                    let new_height = bounds.height();
                    log::trace!("wrap_horizontal: measured width={new_width} height={new_height}");

                    set_float(&mut container.calculated_preferred_width, new_width);
                    set_float(&mut container.calculated_width, new_width);
                    set_float(&mut container.calculated_preferred_height, new_height);
                    set_float(&mut container.calculated_height, new_height);
                };

                wrap_on_axis!(
                    "wrap", Row, bfs, container, width, overflow_x, column_gap, each_child,
                )
            }
        }
    }

    pub mod pass_heights {
        use crate::{
            BfsPaths, Container, Number,
            layout::{calc::Calculator, font::FontMetrics, set_float},
        };

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
                    Column,
                    Row,
                    top,
                    bottom,
                    y,
                    (|_, _, _, _, _| {}),
                    (|_, _, _, _, _| {}),
                )
            }
        }
    }

    pub mod pass_flex_height {
        use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

        use crate::{
            BfsPaths, Container,
            layout::{calc::Calculator, font::FontMetrics, set_float},
        };

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
                    Column,
                    Row,
                    row,
                    top,
                    bottom,
                    y,
                )
            }
        }
    }

    pub mod pass_positioning {
        use bumpalo::Bump;
        use hyperchad_transformer_models::{
            AlignItems, JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition, Position,
            TextAlign, Visibility,
        };

        use crate::{
            BfsPaths, Container, Element, float_lte,
            layout::{calc::Calculator, font::FontMetrics, set_float},
        };

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
                let root_text_align = container.text_align;
                let view_width = container.calculated_width.expect("Missing view_width");
                let view_height = container.calculated_height.expect("Missing view_height");

                let mut changed = false;

                #[allow(clippy::cognitive_complexity)]
                bfs.traverse_with_parents_ref_mut(
                    true,
                    Container::default(),
                    container,
                    |parent, mut relative_container| {
                        if parent.id == root_id {
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
                                    set_float(&mut child.calculated_offset_x, x);
                                }

                                #[cfg(feature = "layout-offset")]
                                {
                                    set_float(&mut child.calculated_offset_x, if col > 0 { gap } else { x });
                                    set_float(&mut child.calculated_offset_y, if row > 0 { row_gap } else { 0.0 });
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
                            };

                            if let Some(text_align) = relative_container.text_align {
                                if visible_elements!().all(|x| matches!(x.element, Element::Raw { .. }))
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
                                }

                                log::trace!("position_elements: setting position ({x}, {y}) i={i}:\n{child}");
                                if set_float(&mut child.calculated_x, x).is_some() {
                                    changed = true;
                                }
                                if set_float(&mut child.calculated_y, y).is_some() {
                                    changed = true;
                                }

                                match direction {
                                    LayoutDirection::Row => {
                                        #[cfg(feature = "layout-offset")]
                                        {
                                            set_float(&mut child.calculated_offset_x, if i == 0 { start_x } else { col_gap });
                                            set_float(&mut child.calculated_offset_y, y);
                                        }
                                        x += child.bounding_calculated_width().unwrap() + col_gap;
                                        y = start_y;
                                    }
                                    LayoutDirection::Column => {
                                        #[cfg(feature = "layout-offset")]
                                        {
                                            set_float(&mut child.calculated_offset_x, x);
                                            set_float(&mut child.calculated_offset_y, if i == 0 { start_y } else { row_gap });
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
                            ($iter:expr) => {
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

                                    if set_float(&mut child.calculated_x, x).is_some() {
                                        changed = true;
                                    }
                                    if set_float(&mut child.calculated_y, y).is_some() {
                                        changed = true;
                                    }
                                }
                            };
                        }

                        position_absolute!(parent.absolute_positioned_elements_mut());
                        position_absolute!(parent.fixed_positioned_elements_mut());
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

        fn is_shrinkable(&self) -> bool {
            self.is_span()
        }

        fn can_shrink_width(&self) -> bool {
            self.is_shrinkable()
                && self.calculated_width.unwrap_or_default()
                    - self.calculated_min_width.unwrap_or_default()
                    >= crate::layout::EPSILON
        }

        fn can_shrink_height(&self) -> bool {
            self.is_shrinkable()
                && self.calculated_height.unwrap_or_default()
                    - self.calculated_min_height.unwrap_or_default()
                    >= crate::layout::EPSILON
        }
    }
}

impl Container {
    #[must_use]
    pub fn margin_x(&self) -> Option<f32> {
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
    pub fn margin_y(&self) -> Option<f32> {
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
    pub fn padding_x(&self) -> Option<f32> {
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
    pub fn padding_y(&self) -> Option<f32> {
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
    pub fn borders_x(&self) -> Option<f32> {
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
    pub fn borders_y(&self) -> Option<f32> {
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
            self.borders_x().map_or(x, |borders| {
                let x = x - borders;
                if x < 0.0 { 0.0 } else { x }
            })
        })
    }

    #[must_use]
    pub fn calculated_height_minus_borders(&self) -> Option<f32> {
        self.calculated_height.map(|x| {
            self.borders_y().map_or(x, |borders| {
                let x = x - borders;
                if x < 0.0 { 0.0 } else { x }
            })
        })
    }

    #[must_use]
    pub fn bounding_calculated_width(&self) -> Option<f32> {
        self.calculated_width.map(|width| {
            width
                + self.padding_x().unwrap_or(0.0)
                + self.scrollbar_right.unwrap_or(0.0)
                + self.margin_x().unwrap_or(0.0)
        })
    }

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
        log::trace!("first container:\n{}", container);

        CALCULATOR.calc(&mut container);
        log::trace!("second container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
        log::trace!("container:\n{}", container);

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
                    main sx-overflow-y="auto" sx-flex=(1) {}
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
                    main sx-overflow-y="auto" sx-flex=(1) {}
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

    #[test_log::test]
    fn calc_only_takes_height_necessary_for_flex_cross_axis_column_container_contents_when_align_items_is_set()
     {
        let mut container: Container = html! {
            div sx-dir=(LayoutDirection::Row) sx-align-items=(AlignItems::Center) sx-height=(50) {
                div { div { "one" } div { "two" } }
                div { "three" }
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

        #[test_log::test]
        fn does_use_preferred_width_for_nested_text() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-align-items=(AlignItems::Start) {
                    "test" span { "two" }
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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
                        .try_into()
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
                        .try_into()
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
                        .try_into()
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
                        .try_into()
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
                        .try_into()
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
                        .try_into()
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
                        .try_into()
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("full container:\n{}", container);
            container = container.children[0].clone();
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(200.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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

        #[test_log::test]
        fn does_size_dynamic_width_correctly() {
            let mut container: Container = html! {
                div sx-position=(Position::Fixed) sx-width="100%" {}
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
            .try_into()
            .unwrap();

            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(1000.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.calculated_width = Some(1000.0);
            container.calculated_height = Some(500.0);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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
            log::trace!("container:\n{}", container);

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

        #[test_log::test]
        fn does_center_vertical_row_children() {
            let mut container: Container = html! {
                div sx-dir=(LayoutDirection::Row) sx-width=(100) sx-height=(100) sx-align-items=(AlignItems::Center) {
                    div sx-height=(70) {}
                    div sx-height=(48) {}
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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            container.column_gap = Some(Number::Integer(5));

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Row;
            container.calculated_width = Some(400.0);
            container.calculated_height = Some(100.0);
            container.column_gap = Some(Number::Integer(5));
            container.justify_content = Some(JustifyContent::Center);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);
            container.row_gap = Some(Number::Integer(5));

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            .try_into()
            .unwrap();

            container.direction = LayoutDirection::Column;
            container.calculated_width = Some(100.0);
            container.calculated_height = Some(400.0);
            container.row_gap = Some(Number::Integer(5));
            container.justify_content = Some(JustifyContent::Center);

            CALCULATOR.calc(&mut container);
            log::trace!("container:\n{}", container);

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
            #[ignore]
            fn does_include_gap_in_offset_y_overflow_y_wrap() {
                let mut container: Container = html! {
                    div sx-overflow-y=(LayoutOverflow::Wrap { grid: false }) sx-gap=(10) {
                        div sx-height=(50) {}
                        div sx-height=(50) {}
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
            #[ignore]
            fn does_include_gap_in_offset_x_overflow_y_wrap() {
                let mut container: Container = html! {
                    div sx-height=(50) sx-overflow-y=(LayoutOverflow::Wrap { grid: false }) sx-gap=(10) {
                        div sx-height=(50) {}
                        div sx-height=(50) {}
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
