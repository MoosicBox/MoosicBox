use bumpalo::Bump;

use crate::{BfsPaths, Container};

use super::{Calc, font::FontMetrics};

pub struct CalcV2Calculator<F: FontMetrics> {
    #[allow(unused)]
    font_metrics: F,
}

impl<F: FontMetrics> CalcV2Calculator<F> {
    pub const fn new(font_metrics: F) -> Self {
        Self { font_metrics }
    }
}

impl<F: FontMetrics> Calc for CalcV2Calculator<F> {
    fn calc(&self, container: &mut Container) -> bool {
        use pass_flex_height::Pass as _;
        use pass_flex_width::Pass as _;
        use pass_heights::Pass as _;
        use pass_positioning::Pass as _;
        use pass_widths::Pass as _;
        use pass_wrap::Pass as _;

        log::trace!("calc: container={container}");

        let bfs: BfsPaths = (&*container).into();
        let arena = Bump::new();

        self.calc_widths(&bfs, container);
        self.flex_width(&bfs, container);
        if self.wrap(&bfs, container) {
            self.flex_width(&bfs, container);
        }
        self.calc_heights(&bfs, container);
        self.flex_height(&bfs, container);
        self.position_elements(&arena, &bfs, container);

        false
    }
}

/// # Pass 1: Widths
///
/// This pass traverses the `Container` children in reverse BFS (Breadth-First Search)
/// and calculates the widths required for each of the `Container`s.
mod pass_widths {
    use crate::{
        BfsPaths, Container, Element,
        layout::{font::FontMetrics, set_float},
    };

    use super::CalcV2Calculator;

    pub trait Pass {
        fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for CalcV2Calculator<F> {
        fn calc_widths(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            moosicbox_logging::debug_or_trace!(("calc_width"), ("calc_width:\n{container}"));

            let view_width = container.calculated_width.unwrap();
            let view_height = container.calculated_height.unwrap();

            let mut changed = false;

            bfs.traverse_rev_mut(container, |parent| {
                let mut min_width = 0.0;

                for child in &mut parent.children {
                    log::trace!("calc_widths: container:\n{child}");

                    if let Some(width) = &child.width {
                        let new_width = width.calc(0.0, view_width, view_height);

                        min_width += new_width;

                        if set_float(&mut child.calculated_width, new_width).is_some() {
                            changed = true;
                        }
                    } else if let Element::Raw { value } = &child.element {
                        let bounds = self.font_metrics.measure_text(value, 14.0, f32::INFINITY);
                        let new_width = bounds.width();

                        if set_float(&mut child.calculated_width, new_width).is_some() {
                            changed = true;
                        }
                    } else if let Some(width) = child.calculated_min_width {
                        if set_float(&mut child.calculated_width, width).is_some() {
                            changed = true;
                        }
                    } else {
                        set_float(&mut child.calculated_width, 0.0);
                    }
                }

                set_float(&mut parent.calculated_min_width, min_width);
            });

            changed
        }
    }

    impl<F: FontMetrics> CalcV2Calculator<F> {}
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
        $bfs_ident:ident,
        $container_ident:ident,
        $changed_ident:ident,
        $fixed_ident:ident,
        $calculated_ident:ident,
        $axis_ident:ident,
        $cross_axis_ident:ident,
        $cell_ident:ident
    ) => {
        let root_id = $container_ident.id;
        let view_width = $container_ident.calculated_width.unwrap();
        let view_height = $container_ident.calculated_height.unwrap();
        let relative_container = std::sync::Arc::new(std::sync::RwLock::new(super::Rect::default()));

        #[allow(clippy::cognitive_complexity)]
        $bfs_ident.traverse_with_parents_mut(
            true,
            $container_ident,
            |parent| {
                if parent.id == root_id {
                    *relative_container.write().unwrap() = super::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: view_width,
                        height: view_height,
                    };
                } else if parent.position == Some(Position::Relative) {
                    *relative_container.write().unwrap() = super::Rect {
                        x: parent.calculated_x.unwrap(),
                        y: parent.calculated_y.unwrap(),
                        width: parent.calculated_width.unwrap(),
                        height: parent.calculated_height.unwrap(),
                    };
                }
            },
            |parent| {
                if parent.relative_positioned_elements().all(|x| x.$fixed_ident.is_some()) {
                    return;
                }

                let container_size = parent.$calculated_ident.unwrap();

                let mut remaining_size = container_size;
                let mut last_cell = 0;
                let mut max_cell_size = 0.0;

                for child in &mut parent.relative_positioned_elements() {
                    log::trace!("flex: calculating remaining size:\n{child}");

                    match parent.direction  {
                        LayoutDirection::$axis_ident => {
                            if let Some(size) = child.$calculated_ident {
                                log::trace!(
                                    "flex: removing size={size} from remaining_size={remaining_size} ({})",
                                    remaining_size - size
                                );
                                remaining_size -= size;
                            }
                        }
                        LayoutDirection::$cross_axis_ident => {
                            if let Some(LayoutPosition::Wrap { $cell_ident: cell, .. }) = child.calculated_position {
                                if cell != last_cell {
                                    moosicbox_assert::assert!(cell > last_cell);
                                    remaining_size -= max_cell_size;
                                    max_cell_size = child.$calculated_ident.unwrap_or_default();
                                }

                                last_cell = cell;
                            }
                        }
                    }
                }

                let cell_count = last_cell + 1;
                remaining_size -= max_cell_size;

                log::trace!("flex: remaining_size={remaining_size}");

                match parent.direction {
                    LayoutDirection::$axis_ident => {
                        #[allow(clippy::while_float)]
                        while remaining_size >= EPSILON {
                            let mut smallest= f32::INFINITY;
                            let mut target= f32::INFINITY;
                            let mut smallest_count= 0;

                            for size in parent
                                .relative_positioned_elements()
                                .filter(|x| x.$fixed_ident.is_none())
                                .filter_map(|x| x.$calculated_ident)
                            {
                                if smallest > size {
                                    target = smallest;
                                    smallest = size;
                                    smallest_count = 1;
                                } else if (smallest - size).abs() < EPSILON {
                                    smallest_count += 1;
                                }
                            }

                            moosicbox_assert::assert!(smallest_count > 0);
                            moosicbox_assert::assert!(smallest.is_finite());

                            if target.is_infinite() {
                                target = remaining_size;
                            }

                            let target_delta = target - smallest;
                            remaining_size -= target_delta;

                            #[allow(clippy::cast_precision_loss)]
                            let delta = target_delta / (smallest_count as f32);

                            log::trace!("flex: target={target} target_delta={target_delta} smallest={smallest} smallest_count={smallest_count} delta={delta} remaining_size={remaining_size}");

                            for child in parent
                                .relative_positioned_elements_mut()
                                .filter(|x| x.$fixed_ident.is_none())
                                .filter(|x| x.$calculated_ident.is_some_and(|x| (x - smallest).abs() < EPSILON))
                            {
                                let size = child.$calculated_ident.unwrap();
                                log::trace!("flex: distributing evenly split remaining_size={remaining_size} delta={delta}:\n{child}");
                                set_float(&mut child.$calculated_ident, size + delta);
                            }
                        }
                    }
                    LayoutDirection::$cross_axis_ident => {
                        for child in parent.relative_positioned_elements_mut() {
                            log::trace!("flex: setting size to remaining_size={remaining_size}:\n{child}");

                            #[allow(clippy::cast_precision_loss)]
                            if child.$fixed_ident.is_none()
                                && set_float(&mut child.$calculated_ident, remaining_size / (cell_count as f32)).is_some()
                            {
                                $changed_ident = true;
                            }
                        }
                    }
                }

                // absolute positioned

                let super::Rect { $fixed_ident: size, .. } = *relative_container.read().unwrap();

                for child in parent.absolute_positioned_elements_mut() {
                    if child.$fixed_ident.is_some() {
                        continue;
                    }

                    if set_float(&mut child.$calculated_ident, size).is_some() {
                        $changed_ident = true;
                    }
                }
            });
    };
}

mod pass_flex_width {
    use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{EPSILON, font::FontMetrics, set_float},
    };

    use super::CalcV2Calculator;

    pub trait Pass {
        fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for CalcV2Calculator<F> {
        fn flex_width(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            moosicbox_logging::debug_or_trace!(("flex_width"), ("flex_width:\n{container}"));

            let mut changed = false;

            flex_on_axis!(
                bfs,
                container,
                changed,
                width,
                calculated_width,
                Row,
                Column,
                col
            );

            changed
        }
    }

    impl<F: FontMetrics> CalcV2Calculator<F> {}
}

mod pass_heights {
    use crate::{
        BfsPaths, Container,
        layout::{font::FontMetrics, set_float},
    };

    use super::CalcV2Calculator;

    pub trait Pass {
        fn calc_heights(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for CalcV2Calculator<F> {
        fn calc_heights(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            moosicbox_logging::debug_or_trace!(("calc_heights"), ("calc_heights:\n{container}"));

            let view_width = container.calculated_width.unwrap();
            let view_height = container.calculated_height.unwrap();

            let mut changed = false;

            bfs.traverse_rev_mut(container, |parent| {
                let mut min_height = 0.0;

                for child in &mut parent.children {
                    log::trace!("calc_heights: container:\n{child}");

                    if let Some(height) = &child.height {
                        let new_height = height.calc(0.0, view_width, view_height);

                        min_height += new_height;

                        if set_float(&mut child.calculated_height, new_height).is_some() {
                            changed = true;
                        }
                    } else if let Some(height) = child.calculated_min_height {
                        if set_float(&mut child.calculated_height, height).is_some() {
                            changed = true;
                        }
                    } else {
                        set_float(&mut child.calculated_height, 0.0);
                    }
                }

                set_float(&mut parent.calculated_min_height, min_height);
            });

            changed
        }
    }

    impl<F: FontMetrics> CalcV2Calculator<F> {}
}

mod pass_flex_height {
    use hyperchad_transformer_models::{LayoutDirection, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{EPSILON, font::FontMetrics, set_float},
    };

    use super::CalcV2Calculator;

    pub trait Pass {
        fn flex_height(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for CalcV2Calculator<F> {
        fn flex_height(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            moosicbox_logging::debug_or_trace!(("flex_height"), ("flex_height:\n{container}"));

            let mut changed = false;

            flex_on_axis!(
                bfs,
                container,
                changed,
                height,
                calculated_height,
                Column,
                Row,
                row
            );

            changed
        }
    }

    impl<F: FontMetrics> CalcV2Calculator<F> {}
}

mod pass_wrap {
    use hyperchad_transformer_models::{LayoutDirection, LayoutOverflow, LayoutPosition};

    use crate::{
        BfsPaths, Container,
        layout::{font::FontMetrics, set_value},
    };

    use super::CalcV2Calculator;

    pub trait Pass {
        fn wrap(&self, bfs: &BfsPaths, container: &mut Container) -> bool;
    }

    impl<F: FontMetrics> Pass for CalcV2Calculator<F> {
        fn wrap(&self, bfs: &BfsPaths, container: &mut Container) -> bool {
            moosicbox_logging::debug_or_trace!(("wrap"), ("wrap:\n{container}"));

            let mut changed = true;

            bfs.traverse_mut(container, |parent| {
                if !matches!(parent.overflow_x, LayoutOverflow::Wrap { .. }) {
                    return;
                }

                let container_width = parent.calculated_width.unwrap();

                let direction = parent.direction;
                let mut x = 0.0;
                let mut row = 0;
                let mut col = 0;

                for child in parent.relative_positioned_elements_mut() {
                    log::trace!("wrap: positioning child ({row}, {col}):\n{child}");

                    let child_width = child.calculated_width.unwrap();
                    let mut position = LayoutPosition::Wrap { row, col };

                    if direction == LayoutDirection::Row {
                        x += child_width;

                        if x > container_width {
                            log::trace!("wrap: wrapping to next row");
                            x = 0.0;
                            col = 0;
                            row += 1;
                            position = LayoutPosition::Wrap { row, col };
                        }

                        col += 1;
                    }

                    if set_value(&mut child.calculated_position, position).is_some() {
                        changed = true;
                    }
                }
            });

            changed
        }
    }

    impl<F: FontMetrics> CalcV2Calculator<F> {}
}

mod pass_positioning {
    use std::sync::{Arc, RwLock};

    use bumpalo::Bump;
    use hyperchad_transformer_models::{LayoutDirection, LayoutOverflow, LayoutPosition, Position};

    use crate::{
        BfsPaths, Container,
        layout::{font::FontMetrics, set_float},
    };

    use super::CalcV2Calculator;

    pub trait Pass {
        fn position_elements(
            &self,
            arena: &Bump,
            bfs: &BfsPaths,
            container: &mut Container,
        ) -> bool;
    }

    impl<F: FontMetrics> Pass for CalcV2Calculator<F> {
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
            let view_width = container.calculated_width.unwrap();
            let view_height = container.calculated_height.unwrap();

            let mut changed = false;

            let relative_container = Arc::new(RwLock::new((0.0, 0.0, 0.0, 0.0)));

            #[allow(clippy::cognitive_complexity)]
            bfs.traverse_with_parents_mut(
                true,
                container,
                |parent| {
                    if parent.id == root_id {
                        *relative_container.write().unwrap() = (0.0, 0.0, view_width, view_height);
                    } else if parent.position == Some(Position::Relative) {
                        *relative_container.write().unwrap() = (
                            parent.calculated_x.unwrap(),
                            parent.calculated_y.unwrap(),
                            parent.calculated_width.unwrap(),
                            parent.calculated_height.unwrap(),
                        );
                    }
                },
                |parent| {
                    let mut x = 0.0;
                    let mut y = 0.0;

                    let direction = parent.direction;
                    let container_width = parent.calculated_width.unwrap();

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
                                let gap = remainder / ((col_count + 1) as f32);
                                gaps.push(gap);

                                if grid && col_count > max_col_count {
                                    max_col_count = col_count;
                                }

                                row_width = 0.0;
                                col_count = 0;
                                last_row = row;
                            }

                            row_width +=
                                grid_cell_size.unwrap_or_else(|| child.calculated_width.unwrap());
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

                        let mut gap = match parent.justify_content.unwrap_or_default() {
                            hyperchad_transformer_models::JustifyContent::Start => 0.0,
                            hyperchad_transformer_models::JustifyContent::Center => todo!(),
                            hyperchad_transformer_models::JustifyContent::End => todo!(),
                            hyperchad_transformer_models::JustifyContent::SpaceBetween => todo!(),
                            hyperchad_transformer_models::JustifyContent::SpaceEvenly => {
                                gaps.first().copied().unwrap_or_default()
                            }
                        };
                        let mut max_height = 0.0;
                        last_row = 0;
                        x = gap;

                        for child in parent.relative_positioned_elements_mut() {
                            let Some(LayoutPosition::Wrap { row, .. }) = child.calculated_position
                            else {
                                continue;
                            };

                            let child_width = child.calculated_width.unwrap();
                            let child_height = child.calculated_height.unwrap();

                            if row != last_row {
                                moosicbox_assert::assert!(row > last_row);

                                if !grid {
                                    // FIXME: This could break if we allow jumping rows (e.g. from row 2 to 4)
                                    gap = gaps.get(row as usize).copied().unwrap_or_default();
                                }

                                x = gap;
                                y += max_height;
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
                                    x += child.calculated_width.unwrap();
                                }
                                LayoutDirection::Column => {
                                    y += child.calculated_height.unwrap();
                                }
                            }
                        }
                    }

                    // absolute positioned

                    let (_x, _y, width, height) = *relative_container.read().unwrap();

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

    impl<F: FontMetrics> CalcV2Calculator<F> {}
}

#[cfg(test)]
mod test {
    use bumpalo::Bump;
    use maud::html;
    use pretty_assertions::{assert_eq, assert_ne};

    use crate::{
        Calculation, Container, Element, HeaderSize, Number, Position,
        layout::{
            Calc as _, EPSILON,
            font::{FontMetrics, FontMetricsBounds, FontMetricsRow},
            get_scrollbar_size,
        },
        models::{JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition},
    };

    use super::CalcV2Calculator;

    fn compare_containers(a: &Container, b: &Container) {
        assert_eq!(
            a.display_to_string(
                true,
                #[cfg(feature = "format")]
                true,
                #[cfg(feature = "syntax-highlighting")]
                false
            )
            .unwrap(),
            b.display_to_string(
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

    static CALCULATOR: CalcV2Calculator<DefaultFontMetrics> =
        CalcV2Calculator::new(DefaultFontMetrics);

    #[test_log::test]
    fn calc_can_calc_single_element_size() {
        let mut container = Container {
            children: vec![Container::default()],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
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
            div sx-dir=(LayoutDirection::Row) {
                div {} div {}
            }
            div {}
        }
        .try_into()
        .unwrap();

        container.calculated_width = Some(100.0);
        container.calculated_height = Some(40.0);

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
    #[ignore]
    fn contained_sized_width_calculates_wrapped_width_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
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
        let width = container.contained_sized_width(
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
            true,
        );
        let expected = 50.0;

        assert_ne!(width, None);
        let width = width.unwrap();
        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_sized_width_calculates_wrapped_empty_width_correctly() {
        let container = Container {
            children: vec![
                Container {
                    height: Some(Number::Integer(25)),
                    calculated_width: Some(40.0),
                    calculated_height: Some(25.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    height: Some(Number::Integer(25)),
                    calculated_width: Some(40.0),
                    calculated_height: Some(25.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    height: Some(Number::Integer(25)),
                    calculated_width: Some(40.0),
                    calculated_height: Some(25.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(40.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let width = container.contained_sized_width(
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
            true,
        );

        assert_eq!(width, None);
    }

    #[test_log::test]
    #[ignore]
    fn contained_sized_height_calculates_wrapped_height_correctly() {
        let container = Container {
            children: vec![
                Container {
                    height: Some(Number::Integer(25)),
                    calculated_width: Some(40.0),
                    calculated_height: Some(25.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    height: Some(Number::Integer(25)),
                    calculated_width: Some(40.0),
                    calculated_height: Some(25.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
                Container {
                    height: Some(Number::Integer(25)),
                    calculated_width: Some(40.0),
                    calculated_height: Some(25.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(40.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Column,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let height = container.contained_sized_height(
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
            true,
        );
        let expected = 50.0;

        assert_ne!(height, None);
        let height = height.unwrap();
        assert_eq!(
            (height - expected).abs() < EPSILON,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_sized_height_calculates_empty_height_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
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
        let height = container.contained_sized_height(
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
            true,
        );

        assert_eq!(height, None);
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_width_calculates_wrapped_width_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
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
        let width = container.contained_calculated_width();
        let expected = 50.0;

        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_height_calculates_wrapped_height_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
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
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < EPSILON,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_scroll_y_width_calculates_wrapped_height_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(20.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let width = container.contained_calculated_width();
        let expected = 50.0;

        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_scroll_y_calculates_height_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < EPSILON,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_width_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow()
     {
        let mut container = Container {
            children: vec![
                Container {
                    calculated_width: Some(50.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    calculated_width: Some(50.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    calculated_width: Some(50.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (50.0, 40.0)) {}
        let width = container.contained_calculated_width();
        let expected = 50.0 - f32::from(get_scrollbar_size());

        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn handle_overflow_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow() {
        let mut container = Container {
            children: vec![
                Container {
                    calculated_width: Some(50.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    calculated_width: Some(50.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    calculated_width: Some(50.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (50.0, 40.0)) {}
        let width = 50.0 - f32::from(get_scrollbar_size());

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(width),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(width),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(width),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(80.0),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_width_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow_and_hardsized_elements()
     {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (50.0, 40.0)) {}
        let width = container.contained_calculated_width();
        let expected = 25.0;

        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn handle_overflow_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow_and_hardsized_elements()
     {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (50.0, 40.0)) {}

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(80.0),
                        ..container.children[2].clone()
                    },
                ],
                calculated_width: Some(50.0 - f32::from(get_scrollbar_size())),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handle_overflow_auto_y_wraps_elements_properly_by_taking_into_account_scrollbar_size() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(40.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(80.0),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0 - f32::from(get_scrollbar_size())),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
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
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

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
    #[ignore]
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
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

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
                        ..Default::default()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
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

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

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
    #[ignore]
    fn handle_overflow_y_expand_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        const ROW_HEIGHT: f32 = 20.0;

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
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceBetween),
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 7.5 + 7.5),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(ROW_HEIGHT * 3.0),
                        ..container.children[9].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(80.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handles_justify_content_space_between_with_gap_and_wraps_elements_properly() {
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
            justify_content: Some(JustifyContent::SpaceBetween),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

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
                        calculated_x: Some(75.0 - 20.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0 + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(75.0 - 20.0),
                        calculated_y: Some(20.0 + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0 + 10.0 + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(60.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handles_justify_content_space_between_with_gap_and_wraps_elements_properly_and_can_recalc() {
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
            justify_content: Some(JustifyContent::SpaceBetween),
            column_gap: Some(Number::Integer(10)),
            row_gap: Some(Number::Integer(10)),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        let mut actual = container.clone();
        let expected = Container {
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
                    calculated_x: Some(75.0 - 20.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..container.children[1].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(20.0 + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..container.children[2].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(75.0 - 20.0),
                    calculated_y: Some(20.0 + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..container.children[3].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(40.0 + 10.0 + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                    ..container.children[4].clone()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(60.0),
            ..container
        };

        compare_containers(&actual, &expected);

        while actual.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 60.0)) {}

        compare_containers(&actual, &expected);
    }

    #[test_log::test]
    #[ignore]
    fn handles_justify_content_space_evenly_and_wraps_elements_properly() {
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
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

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
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
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
    #[ignore]
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows()
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
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

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
    #[ignore]
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows()
     {
        const ROW_HEIGHT: f32 = 20.0;

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
            overflow_y: LayoutOverflow::Expand,
            justify_content: Some(JustifyContent::SpaceEvenly),
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[4].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 1.0),
                        ..container.children[5].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[6].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 1 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[7].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 2 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(ROW_HEIGHT * 2.0),
                        ..container.children[8].clone()
                    },
                    Container {
                        calculated_position: Some(LayoutPosition::Wrap { row: 3, col: 0 }),
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(ROW_HEIGHT * 3.0),
                        ..container.children[9].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(80.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handles_justify_content_space_evenly_with_gap_and_wraps_elements_properly() {
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
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(43.333_336),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(20.0 + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(43.333_336),
                        calculated_y: Some(20.0 + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(11.666_667),
                        calculated_y: Some(40.0 + 10.0 + 10.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                        ..container.children[4].clone()
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(60.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handles_justify_content_space_evenly_with_gap_and_wraps_elements_properly_and_can_recalc() {
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
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 40.0)) {}

        let mut actual = container.clone();
        let expected = Container {
            children: vec![
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..container.children[0].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(43.333_336),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..container.children[1].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(20.0 + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..container.children[2].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(43.333_336),
                    calculated_y: Some(20.0 + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                    ..container.children[3].clone()
                },
                Container {
                    calculated_width: Some(20.0),
                    calculated_height: Some(20.0),
                    calculated_x: Some(11.666_667),
                    calculated_y: Some(40.0 + 10.0 + 10.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                    ..container.children[4].clone()
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(60.0),
            ..container
        };

        compare_containers(&actual, &expected);

        while actual.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (75.0, 60.0)) {}

        compare_containers(&actual, &expected);
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

    #[test_log::test]
    #[ignore]
    fn contained_calculated_expand_y_calculates_height_correctly() {
        let container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                    ..Default::default()
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            ..Default::default()
        };
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < EPSILON,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn contained_calculated_expand_y_nested_calculates_height_correctly() {
        let container = Container {
            children: vec![Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                ],
                calculated_width: Some(50.0),
                calculated_height: Some(80.0),
                ..Default::default()
            }],

            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            ..Default::default()
        };
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < EPSILON,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    #[ignore]
    fn resize_children_expand_y_nested_expands_parent_height_correctly() {
        let mut container = Container {
            children: vec![Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                ],
                calculated_width: Some(50.0),
                calculated_height: Some(80.0),
                ..Default::default()
            }],

            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap { grid: true },
            overflow_y: LayoutOverflow::Expand,
            ..Default::default()
        };
        let resized = container.resize_children(
            &Bump::new(),
            &DefaultFontMetrics,
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
        );

        assert_eq!(resized, true);
        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_height: Some(40.0),
                            ..container.children[0].children[0].clone()
                        },
                        Container {
                            calculated_height: Some(40.0),
                            ..container.children[0].children[1].clone()
                        },
                        Container {
                            calculated_height: Some(40.0),
                            ..container.children[0].children[2].clone()
                        },
                    ],
                    calculated_width: Some(50.0),
                    calculated_height: Some(80.0),
                    ..Default::default()
                }],

                calculated_width: Some(50.0),
                calculated_height: Some(80.0),
                direction: LayoutDirection::Row,
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn resize_children_resizes_when_a_new_row_was_shifted_into_view() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
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
        let resized = container.resize_children(
            &Bump::new(),
            &DefaultFontMetrics,
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
        );

        assert_eq!(resized, true);
        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_height: Some(20.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_height: Some(20.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_height: Some(20.0),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn resize_children_allows_expanding_height_for_overflow_y_scroll() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
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
        let resized = container.resize_children(
            &Bump::new(),
            &DefaultFontMetrics,
            (
                container.calculated_width.unwrap(),
                container.calculated_height.unwrap(),
            ),
        );

        assert_eq!(resized, true);
        compare_containers(
            &container.clone(),
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
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handle_overflow_wraps_single_row_overflow_content_correctly() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
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
        let mut shifted = false;
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (50.0, 40.0)) {
            shifted = true;
        }

        assert_eq!(shifted, true);
        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handle_overflow_wraps_multi_row_overflow_content_correctly() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
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
        let mut shifted = false;
        while container.handle_overflow(&Bump::new(), &DefaultFontMetrics, None, (50.0, 40.0)) {
            shifted = true;
        }

        let row_height = 40.0 / 3.0;

        assert_eq!(shifted, true);
        compare_containers(
            &container.clone(),
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
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
    fn handle_overflow_wraps_row_content_correctly_in_overflow_y_scroll() {
        let mut container = Container {
            children: vec![
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
                    ..Default::default()
                },
                Container {
                    width: Some(Number::Integer(25)),
                    calculated_width: Some(25.0),
                    calculated_height: Some(40.0),
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
        let mut shifted = false;
        while container.handle_overflow(
            &Bump::new(),
            &DefaultFontMetrics,
            None,
            (50.0 + f32::from(get_scrollbar_size()), 80.0),
        ) {
            shifted = true;
        }

        assert_eq!(shifted, true);
        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
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
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(25.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                    Container {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                ],
                ..container
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
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Squash,
                    overflow_y: LayoutOverflow::Squash,
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
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
                    ..Default::default()
                },
                Container {
                    children: vec![Container {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    }],
                    overflow_x: LayoutOverflow::Expand,
                    overflow_y: LayoutOverflow::Expand,
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
                            ..Default::default()
                        },
                    ],
                    direction: LayoutDirection::Row,
                    ..Default::default()
                },
                Container::default(),
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
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
                            ..Default::default()
                        },
                    ],
                    direction: LayoutDirection::Row,
                    ..Default::default()
                },
                Container {
                    height: Some(Number::Integer(10)),
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
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
                                        ..container.children[0].children[0].children[0].children[0]
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
                                        ..container.children[0].children[0].children[1].children[0]
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
                                        ..container.children[0].children[1].children[0].children[0]
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
                                        ..container.children[0].children[1].children[1].children[0]
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
                                        ..container.children[0].children[0].children[0].children[0]
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
                                        ..container.children[0].children[0].children[1].children[0]
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
                                        ..container.children[0].children[1].children[0].children[0]
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
                                        ..container.children[0].children[1].children[1].children[0]
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
                                        ..container.children[0].children[0].children[0].children[0]
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
                                        ..container.children[0].children[1].children[1].children[0]
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
    fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells_when_all_are_unsized() {
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
                                        ..container.children[0].children[0].children[0].children[0]
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
                                        ..container.children[0].children[0].children[1].children[0]
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
                                        ..container.children[0].children[1].children[0].children[0]
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
                                        ..container.children[0].children[1].children[1].children[0]
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
                                        ..container.children[0].children[0].children[0].children[0]
                                            .clone()
                                    },
                                    Container {
                                        children: container.children[0].children[0].children[0]
                                            .children[1]
                                            .children
                                            .clone(),
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[0].children[0].children[1]
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
                                        ..container.children[0].children[0].children[1].children[0]
                                            .clone()
                                    },
                                    Container {
                                        children: container.children[0].children[0].children[1]
                                            .children[1]
                                            .children
                                            .clone(),
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(25.0),
                                        ..container.children[0].children[0].children[1].children[1]
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
    #[ignore]
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
            &container.clone(),
            &Container {
                internal_padding_left: Some((100.0 - 30.0) / 2.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
                            calculated_width: Some(10.0),
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
    #[ignore]
    fn calc_horizontal_sibling_left_raw_still_divides_the_unsized_width() {
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
            &container.clone(),
            &Container {
                children: vec![
                    container.children[0].clone(),
                    Container {
                        calculated_width: Some(50.0),
                        calculated_x: Some(50.0),
                        ..container.children[1].clone()
                    },
                ],
                calculated_width: Some(100.0),
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
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
    #[ignore]
    fn calc_calculates_height_minus_the_vertical_padding() {
        let mut container = Container {
            children: vec![Container {
                padding_top: Some(Number::Integer(10)),
                padding_bottom: Some(Number::Integer(20)),
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
                    calculated_height: Some(20.0),
                    calculated_y: Some(0.0),
                    ..container.children[0].clone()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
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
    #[ignore]
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
                ..Default::default()
            }],

            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
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
    #[ignore]
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
                ..Default::default()
            }],

            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
    fn calc_calculates_width_minus_the_horizontal_padding_for_nested_children_with_calc_parent_sizes()
     {
        let mut container: Container = html! {
            div sx-width="100%" sx-height="100%" sx-position="relative" {
                section sx-dir="row" sx-height=("calc(100% - 140px)") {
                    aside sx-width="calc(max(240, min(280, 15%)))" {}
                    main sx-overflow-y="auto" {
                        div sx-height=(76) {
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
    fn calc_calculates_vertical_padding_on_sized_element_correctly() {
        let mut container: Container = html! {
            div sx-width="100%" sx-height="100%" sx-position="relative" {
                section sx-dir="row" sx-height=("calc(100% - 140)") {
                    aside sx-width="calc(max(240, min(280, 15%)))" sx-padding=(20) {
                        div {
                            div {}
                            ul { li {} li {} }
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
                                        ..container.children[0].children[0].children[0].children[0]
                                            .clone()
                                    },
                                    Container {
                                        calculated_width: Some(616.0),
                                        ..container.children[0].children[0].children[0].children[1]
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
                                        ..container.children[0].children[1].children[0].children[0]
                                            .clone()
                                    },
                                    Container {
                                        calculated_width: Some(596.0),
                                        ..container.children[0].children[1].children[0].children[1]
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
                                ..container.children[0].children[0].children[0].children[0].clone()
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

    #[test_log::test]
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
    fn calc_overflow_y_auto_justify_content_start_only_takes_up_sized_height() {
        let mut container: Container = html! {
            div sx-overflow-y="auto" {
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
                    calculated_height: Some(500.0),
                    ..container.children[0].clone()
                }],
                calculated_height: Some(500.0),
                ..container.clone()
            },
        );
    }
}
