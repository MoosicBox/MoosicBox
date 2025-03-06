use bumpalo::Bump;
use hyperchad_transformer_models::{
    JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition,
};
use itertools::Itertools;

use crate::{
    Container, Element, Number, Position, TableIter, TableIterMut,
    absolute_positioned_elements_mut, fixed_positioned_elements_mut,
    layout::{EPSILON, get_scrollbar_size, order_float},
    relative_positioned_elements, relative_positioned_elements_mut,
};

use super::{Calc, font::FontMetrics, increase_opt};

impl Container {
    fn calc_inner(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        relative_size: Option<(f32, f32)>,
        root_size: (f32, f32),
    ) -> bool {
        log::trace!("calc_inner");

        if self.is_hidden() {
            return false;
        }

        self.internal_margin_left = None;
        self.internal_margin_right = None;
        self.internal_margin_top = None;
        self.internal_margin_bottom = None;

        self.internal_padding_left = None;
        self.internal_padding_right = None;
        self.internal_padding_top = None;
        self.internal_padding_bottom = None;

        let (Some(container_width), Some(container_height)) =
            (self.calculated_width, self.calculated_height)
        else {
            moosicbox_assert::die_or_panic!(
                "calc_inner requires calculated_width and calculated_height to be set"
            );
        };

        moosicbox_assert::assert!(
            container_width >= 0.0,
            "container_width ({container_width}) must be >= 0.0"
        );
        moosicbox_assert::assert!(
            container_height >= 0.0,
            "container_height ({container_height}) must be >= 0.0"
        );

        for element in &mut self.children {
            element.calc_styling(container_width, container_height, root_size);
        }

        if self.element == Element::Table {
            self.calc_table(arena, font_metrics, relative_size, root_size)
        } else {
            self.calc_inner_container(arena, font_metrics, relative_size, root_size)
        }
    }

    fn calc_styling(&mut self, container_width: f32, container_height: f32, root_size: (f32, f32)) {
        self.calc_margin(container_width, container_height, root_size);
        self.calc_padding(container_width, container_height, root_size);
        self.calc_borders(container_width, container_height, root_size);
        self.calc_opacity(root_size);
        self.calc_hardsized_elements();
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn calc_table(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        relative_size: Option<(f32, f32)>,
        root_size: (f32, f32),
    ) -> bool {
        fn size_cells<'a>(
            iter: impl Iterator<Item = &'a mut Container>,
            col_sizes: &mut Vec<(Option<f32>, Option<f32>)>,
            cols: &mut Vec<&'a mut Container>,
            root_size: (f32, f32),
        ) -> f32 {
            let mut col_count = 0;

            let sized_cols = iter.enumerate().map(|(i, x)| {
                col_count += 1;

                let width = x.width.as_ref().and(x.calculated_width);
                let height = x.height.as_ref().and(x.calculated_height);
                let contained_width = x.contained_sized_width(root_size, true);
                let contained_height = x.contained_sized_height(root_size, true);

                if i >= cols.len() {
                    cols.push(x);
                } else {
                    cols[i] = x;
                }

                ((width, contained_width), (height, contained_height))
            });

            let mut max_height = None;

            for (i, ((width, contained_width), (height, contained_height))) in
                sized_cols.enumerate()
            {
                if let Some(width) = width {
                    while i >= col_sizes.len() {
                        col_sizes.push((None, None));
                    }

                    if let Some(col) = col_sizes[i].0 {
                        if width > col {
                            col_sizes[i].0.replace(width);
                        }
                    } else {
                        col_sizes[i].0 = Some(width);
                    }
                } else if let Some(contained_width) = contained_width {
                    while i >= col_sizes.len() {
                        col_sizes.push((None, None));
                    }

                    if let Some(col) = col_sizes[i].1 {
                        if contained_width > col {
                            col_sizes[i].1.replace(contained_width);
                        }
                    } else {
                        col_sizes[i].1 = Some(contained_width);
                    }
                }
                if let Some(height) = height {
                    if let Some(max) = max_height {
                        if height > max {
                            max_height.replace(height);
                        }
                    } else {
                        max_height = Some(height);
                    }
                } else if let Some(contained_height) = contained_height {
                    if let Some(max) = max_height {
                        if contained_height > max {
                            max_height.replace(contained_height);
                        }
                    } else {
                        max_height = Some(contained_height);
                    }
                }
            }

            let row_height = max_height.unwrap_or(25.0);

            for container in cols {
                container.calculated_height.replace(row_height);
            }

            row_height
        }

        moosicbox_assert::assert_or_panic!(self.element == Element::Table, "Not a table");

        moosicbox_logging::debug_or_trace!(("calc_table"), ("calc_table:\n{self}"));

        let (container_width, container_height) = {
            let (Some(container_width), Some(container_height)) = (
                self.calculated_width_minus_borders(),
                self.calculated_height_minus_borders(),
            ) else {
                moosicbox_assert::die_or_panic!(
                    "calc_table requires calculated_width and calculated_height to be set"
                );
            };

            self.calc_hardsized_elements();

            (container_width, container_height)
        };

        // calc max sized cell sizes
        let (body_height, heading_height) = {
            let col_count = {
                let TableIter { rows, headings } = self.table_iter();

                let heading_count = headings.map_or(0, Iterator::count);
                let body_count = rows.map(Iterator::count).max().unwrap_or(0);

                std::cmp::max(heading_count, body_count)
            };

            let mut body_height = 0.0;
            let mut heading_height = None;
            let mut col_sizes = vec![(None, None); col_count];
            let mut cols = Vec::with_capacity(col_count);

            // Initial cell size
            {
                #[allow(clippy::cast_precision_loss)]
                let evenly_split_size = container_width / (col_count as f32);

                let TableIterMut { rows, headings } = self.table_iter_mut();

                if let Some(headings) = headings {
                    for heading in headings {
                        #[allow(clippy::manual_inspect)]
                        let heading = heading.map(|x| {
                            if x.height.is_some() {
                                x.calc_sized_element_height(container_height, root_size);
                            } else if x.calculated_height.is_none() {
                                x.calculated_height = Some(x.contained_calculated_height());
                            }
                            if x.width.is_some() {
                                x.calc_sized_element_width(container_width, root_size);
                            } else if x.calculated_width.is_none() {
                                x.calculated_width = Some(evenly_split_size);
                                x.calc_unsized_element_size(
                                    arena,
                                    font_metrics,
                                    relative_size,
                                    root_size,
                                    evenly_split_size,
                                );
                            }
                            x
                        });
                        let height = size_cells(heading, &mut col_sizes, &mut cols, root_size);
                        heading_height.replace(heading_height.map_or(height, |x| x + height));
                        log::trace!("calc_table: increased heading_height={heading_height:?}");
                    }
                }

                for row in rows {
                    #[allow(clippy::manual_inspect)]
                    let row = row.map(|x| {
                        if x.height.is_some() {
                            x.calc_sized_element_height(container_height, root_size);
                        } else if x.calculated_height.is_none() {
                            x.calculated_height = Some(x.contained_calculated_height());
                        }
                        if x.width.is_some() {
                            x.calc_sized_element_width(container_width, root_size);
                        } else if x.calculated_width.is_none() {
                            x.calculated_width = Some(evenly_split_size);
                            x.calc_unsized_element_size(
                                arena,
                                font_metrics,
                                relative_size,
                                root_size,
                                evenly_split_size,
                            );
                        }
                        x
                    });
                    body_height += size_cells(row, &mut col_sizes, &mut cols, root_size);
                    log::trace!("calc_table: increased body_height={body_height}");
                }
            }

            {
                let TableIterMut { rows, headings } = self.table_iter_mut();

                for row in rows {
                    for element in row {
                        element.calc_styling(container_width, container_height, root_size);
                    }
                }

                if let Some(headings) = headings {
                    for row in headings {
                        for element in row {
                            element.calc_styling(container_width, container_height, root_size);
                        }
                    }
                }
            }

            // Set unsized cells to remainder size
            let TableIterMut { rows, headings } = self.table_iter_mut();

            let unsized_col_count = col_sizes.iter().filter(|(x, _y)| x.is_none()).count();
            let sized_width: f32 = col_sizes.iter().filter_map(|(x, _y)| *x).sum();

            #[allow(clippy::cast_precision_loss)]
            let evenly_split_remaining_size = if unsized_col_count == 0 {
                0.0
            } else {
                (container_width - sized_width) / (unsized_col_count as f32)
            };

            let col_sizes = col_sizes
                .into_iter()
                .map(|(calculated_width, contained_width)| {
                    calculated_width.or_else(|| {
                        if let Some(width) = contained_width {
                            if width > evenly_split_remaining_size {
                                return Some(width);
                            }
                        }
                        None
                    })
                })
                .collect::<Vec<_>>();

            let unsized_col_count = col_sizes.iter().filter(|x| x.is_none()).count();
            let sized_width: f32 = col_sizes.iter().filter_map(|x| *x).sum();

            #[allow(clippy::cast_precision_loss)]
            let evenly_split_remaining_size = if unsized_col_count == 0 {
                0.0
            } else {
                (container_width - sized_width) / (unsized_col_count as f32)
            };

            #[allow(clippy::cast_precision_loss)]
            let evenly_split_increase_size = if unsized_col_count == 0 {
                (container_width - sized_width) / (col_count as f32)
            } else {
                0.0
            };

            log::debug!(
                "calc_table: col_sizes={col_sizes:?} evenly_split_remaining_size={evenly_split_remaining_size} evenly_split_increase_size={evenly_split_increase_size}"
            );

            if let Some(headings) = headings {
                for heading in headings {
                    for (th, size) in heading.zip(&col_sizes) {
                        log::trace!("calc_table: sizing head th size={size:?}");
                        let width = size.as_ref().map_or(evenly_split_remaining_size, |size| {
                            *size + evenly_split_increase_size
                        });
                        let width = std::cmp::max_by(
                            0.0,
                            width - th.padding_and_margins(LayoutDirection::Row).unwrap_or(0.0),
                            order_float,
                        );
                        log::trace!("calc_table: sizing head th width={width}");
                        th.calculated_width = Some(width);
                    }
                }
            }

            for row in rows {
                for (td, size) in row.zip(&col_sizes) {
                    log::trace!("calc_table: sizing body td size={size:?}");
                    let width = size.as_ref().map_or(evenly_split_remaining_size, |size| {
                        *size + evenly_split_increase_size
                    });
                    let width = std::cmp::max_by(
                        0.0,
                        width - td.padding_and_margins(LayoutDirection::Row).unwrap_or(0.0),
                        order_float,
                    );
                    log::trace!("calc_table: sizing body td width={width}");
                    td.calculated_width = Some(width);
                }
            }

            (body_height, heading_height)
        };

        self.calculated_height
            .replace(heading_height.unwrap_or(0.0) + body_height);

        for element in relative_positioned_elements_mut(&mut self.children) {
            element.calc_borders(container_width, container_height, root_size);
            element.calc_opacity(root_size);
            match &element.element {
                Element::THead => {
                    if element.width.is_none() {
                        element.calculated_width.replace(container_width);
                    }
                    if element.height.is_none() {
                        element
                            .calculated_height
                            .replace(heading_height.unwrap_or(0.0));
                    }

                    for element in relative_positioned_elements_mut(&mut element.children) {
                        element.calc_borders(container_width, container_height, root_size);
                        element.calc_opacity(root_size);
                        if element.width.is_none() {
                            element.calculated_width.replace(container_width);
                        }
                        if element.height.is_none() {
                            element.calculated_height.replace(
                                relative_positioned_elements(&element.children)
                                    .find_map(|x| x.calculated_height)
                                    .unwrap_or(0.0),
                            );
                        }
                    }
                }
                Element::TBody => {
                    if element.width.is_none() {
                        element.calculated_width.replace(container_width);
                    }
                    if element.height.is_none() {
                        element.calculated_height.replace(body_height);
                    }

                    for element in relative_positioned_elements_mut(&mut element.children) {
                        element.calc_borders(container_width, container_height, root_size);
                        element.calc_opacity(root_size);
                        if element.width.is_none() {
                            element.calculated_width.replace(container_width);
                        }
                        if element.height.is_none() {
                            element.calculated_height.replace(
                                relative_positioned_elements(&element.children)
                                    .find_map(|x| x.calculated_height)
                                    .unwrap_or(0.0),
                            );
                        }
                    }
                }
                Element::TR => {
                    if element.width.is_none() {
                        element.calculated_width.replace(container_width);
                    }
                    if element.height.is_none() {
                        element.calculated_height.replace(
                            relative_positioned_elements(&element.children)
                                .find_map(|x| x.calculated_height)
                                .unwrap_or(0.0),
                        );
                    }
                }
                _ => {
                    moosicbox_assert::die_or_panic!("Invalid table element: {element}");
                }
            }
        }

        let TableIterMut { rows, headings } = self.table_iter_mut();

        if let Some(headings) = headings {
            for heading in headings {
                for th in heading {
                    th.calc_inner(arena, font_metrics, relative_size, root_size);
                }
            }
        }

        for row in rows {
            for td in row {
                td.calc_inner(arena, font_metrics, relative_size, root_size);
            }
        }

        true
    }
}

pub struct CalcCalculator<F: FontMetrics> {
    font_metrics: F,
}

impl<F: FontMetrics> CalcCalculator<F> {
    pub const fn new(font_metrics: F) -> Self {
        Self { font_metrics }
    }
}

impl<F: FontMetrics> Calc for CalcCalculator<F> {
    fn calc(&self, container: &mut Container) -> bool {
        log::trace!("calc: container={container}");

        let (Some(root_width), Some(root_height)) =
            (container.calculated_width, container.calculated_height)
        else {
            moosicbox_assert::die_or_panic!(
                "calc requires calculated_width and calculated_height to be set"
            );
        };

        log::debug!("calc: root_width={root_width} root_height={root_height}");

        let arena = Bump::new();

        container.calc_inner(&arena, &self.font_metrics, None, (root_width, root_height))
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Container {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn calc_inner_container(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        relative_size: Option<(f32, f32)>,
        root_size: (f32, f32),
    ) -> bool {
        log::trace!("calc_inner_container: processing self\n{self}");

        let direction = self.direction;
        let overflow_x = self.overflow_x;
        let overflow_y = self.overflow_y;

        let (Some(container_width), Some(container_height)) =
            (self.calculated_width, self.calculated_height)
        else {
            moosicbox_assert::die_or_panic!(
                "calc_inner_container requires calculated_width and calculated_height to be set"
            );
        };

        moosicbox_assert::assert!(
            container_width >= 0.0,
            "container_width ({container_width}) must be >= 0.0"
        );
        moosicbox_assert::assert!(
            container_height >= 0.0,
            "container_height ({container_height}) must be >= 0.0"
        );

        Self::calc_element_sizes(
            arena,
            font_metrics,
            self.relative_positioned_elements_mut(),
            direction,
            overflow_x,
            overflow_y,
            container_width,
            container_height,
            root_size,
        );

        let relative_size = self.get_relative_size().or(relative_size);

        for element in self.relative_positioned_elements_mut() {
            element.calc_inner(arena, font_metrics, relative_size, root_size);
        }

        if let Some((width, height)) = relative_size {
            Self::calc_child_margins_and_padding(
                self.absolute_positioned_elements_mut(),
                width,
                height,
                root_size,
            );

            Self::calc_element_sizes(
                arena,
                font_metrics,
                self.absolute_positioned_elements_mut(),
                direction,
                overflow_x,
                overflow_y,
                container_width,
                container_height,
                root_size,
            );

            for container in self.absolute_positioned_elements_mut() {
                container.calc_inner(arena, font_metrics, relative_size, root_size);
            }
        }

        // Fixed position elements
        {
            Self::calc_child_margins_and_padding(
                self.fixed_positioned_elements_mut(),
                root_size.0,
                root_size.1,
                root_size,
            );

            Self::calc_element_sizes(
                arena,
                font_metrics,
                self.fixed_positioned_elements_mut(),
                direction,
                overflow_x,
                overflow_y,
                root_size.0,
                root_size.1,
                root_size,
            );

            for container in self.fixed_positioned_elements_mut() {
                container.calc_inner(arena, font_metrics, relative_size, root_size);
            }
        }

        let mut attempt = 0;
        while self.handle_overflow(arena, font_metrics, relative_size, root_size) {
            attempt += 1;

            {
                static MAX_HANDLE_OVERFLOW: usize = 100;

                fn truncated(mut value: String, len: usize) -> String {
                    value.truncate(len);
                    value
                }

                moosicbox_assert::assert_or_panic!(
                    attempt < MAX_HANDLE_OVERFLOW,
                    "Max number of handle_overflow attempts encountered on {} children self={}",
                    self.children.len(),
                    truncated(format!("{self:?}"), 50000),
                );
            }

            log::trace!("handle_overflow: attempt {}", attempt + 1);
        }

        attempt > 0
    }

    #[allow(clippy::too_many_arguments)]
    fn calc_element_sizes<'a>(
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        elements: impl Iterator<Item = &'a mut Self>,
        direction: LayoutDirection,
        overflow_x: LayoutOverflow,
        overflow_y: LayoutOverflow,
        container_width: f32,
        container_height: f32,
        root_size: (f32, f32),
    ) {
        let mut elements = elements.peekable();

        if elements.peek().is_none() {
            return;
        }

        let is_grid = match direction {
            LayoutDirection::Row => overflow_x == LayoutOverflow::Wrap,
            LayoutDirection::Column => overflow_y == LayoutOverflow::Wrap,
        };

        log::trace!("calc_element_sizes: is_grid={is_grid}");

        if is_grid {
            Self::calc_element_sizes_by_rowcol(
                arena,
                elements,
                direction,
                container_width,
                container_height,
                |elements, container_width, container_height| {
                    Self::size_elements(
                        font_metrics,
                        elements,
                        direction,
                        container_width,
                        container_height,
                        root_size,
                    );
                },
            );
        } else {
            let mut elements = elements.peekable();

            if elements.peek().is_none() {
                log::trace!("calc_element_sizes: no elements to size");
            } else {
                let mut elements = elements.collect_vec();
                let mut padding_x = 0.0;
                let mut padding_y = 0.0;

                for element in elements.iter().map(|x| &**x) {
                    match direction {
                        LayoutDirection::Row => {
                            if let Some(fluff) = element.padding_and_margins(LayoutDirection::Row) {
                                log::trace!("calc_element_sizes: container_width -= {fluff}");
                                padding_x = fluff;
                            }
                        }
                        LayoutDirection::Column => {
                            if let Some(fluff) =
                                element.padding_and_margins(LayoutDirection::Column)
                            {
                                log::trace!("calc_element_sizes: container_height -= {fluff}");
                                padding_y = fluff;
                            }
                        }
                    }
                }

                Self::size_elements(
                    font_metrics,
                    &mut elements,
                    direction,
                    container_width - padding_x,
                    container_height - padding_y,
                    root_size,
                );
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn size_elements(
        font_metrics: &dyn FontMetrics,
        elements: &mut [&mut Self],
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
        root_size: (f32, f32),
    ) {
        let remainder = {
            #[cfg(feature = "profiling")]
            profiling::scope!("rowcol sized elements");

            let sized_elements = elements.iter_mut().filter(|x| match direction {
                LayoutDirection::Row => x.width.is_some(),
                LayoutDirection::Column => x.height.is_some(),
            });

            let mut remainder = match direction {
                LayoutDirection::Row => container_width,
                LayoutDirection::Column => container_height,
            };

            log::trace!(
                "size_elements: container_width={container_width} container_height={container_height}"
            );
            for container in sized_elements.map(|x| &mut **x) {
                remainder -= container.calc_sized_element_size(
                    direction,
                    container_width,
                    container_height,
                    root_size,
                );
            }

            remainder
        };

        {
            #[cfg(feature = "profiling")]
            profiling::scope!("rowcol unsized elements");

            let unsized_elements_count = elements
                .iter()
                .filter(|x| match direction {
                    LayoutDirection::Row => x.width.is_none(),
                    LayoutDirection::Column => x.height.is_none(),
                })
                .count();

            if unsized_elements_count == 0 {
                log::trace!("size_elements: no unsized elements to size");
                return;
            }

            let unsized_elements = elements.iter_mut().filter(|x| match direction {
                LayoutDirection::Row => x.width.is_none(),
                LayoutDirection::Column => x.height.is_none(),
            });

            #[allow(clippy::cast_precision_loss)]
            let evenly_split_remaining_size = remainder / (unsized_elements_count as f32);

            log::trace!(
                "size_elements: setting {} to evenly_split_remaining_size={evenly_split_remaining_size} unsized_elements_count={unsized_elements_count}",
                if direction == LayoutDirection::Row {
                    "width"
                } else {
                    "height"
                },
            );

            for container in unsized_elements.map(|x| &mut **x) {
                container.size_unsized_element(
                    font_metrics,
                    container_width,
                    container_height,
                    root_size,
                    direction,
                    evenly_split_remaining_size,
                );
            }
        }
    }

    fn size_unsized_element(
        &mut self,
        _font_metrics: &dyn FontMetrics,
        container_width: f32,
        container_height: f32,
        root_size: (f32, f32),
        direction: LayoutDirection,
        size: f32,
    ) {
        if self.is_fixed() {
            self.calculated_width.replace(0.0);
            self.calculated_height.replace(0.0);
            return;
        }

        // if let Element::Raw { value } = &self.element {
        //     let metrics = font_metrics.measure_text(value, 14.0, container_width);
        //     self.calculated_width.replace(metrics.width());
        //     self.calculated_height.replace(metrics.height());
        //     log::warn!("Calculated text: {value}, {metrics:?}");
        //     return;
        // }

        match direction {
            LayoutDirection::Row => {
                let container_height = container_height
                    - self
                        .padding_and_margins(LayoutDirection::Column)
                        .unwrap_or(0.0);
                let height = self.height.as_ref().map_or(container_height, |x| {
                    x.calc(container_height, root_size.0, root_size.1)
                });
                moosicbox_assert::assert!(height >= 0.0);
                self.calculated_height.replace(height);
                moosicbox_assert::assert!(size >= 0.0);
                self.calculated_width.replace(size);
            }
            LayoutDirection::Column => {
                let container_width = container_width
                    - self
                        .padding_and_margins(LayoutDirection::Row)
                        .unwrap_or(0.0);
                let width = self.width.as_ref().map_or(container_width, |x| {
                    x.calc(container_width, root_size.0, root_size.1)
                });
                moosicbox_assert::assert!(width >= 0.0);
                self.calculated_width.replace(width);
                moosicbox_assert::assert!(size >= 0.0);
                self.calculated_height.replace(size);
            }
        }
    }

    fn padding_and_margins(&self, direction: LayoutDirection) -> Option<f32> {
        let mut padding_and_margins = None;

        match direction {
            LayoutDirection::Row => {
                if let Some(padding) = self.horizontal_padding() {
                    padding_and_margins = Some(padding);
                }
                if let Some(scrollbar_size) = self.scrollbar_right {
                    padding_and_margins.replace(
                        padding_and_margins.map_or(scrollbar_size, |x| x + scrollbar_size),
                    );
                }
                if let Some(margins) = self.horizontal_margin() {
                    padding_and_margins
                        .replace(padding_and_margins.map_or(margins, |x| x + margins));
                }
            }
            LayoutDirection::Column => {
                if let Some(padding) = self.vertical_padding() {
                    padding_and_margins = Some(padding);
                }
                if let Some(scrollbar_size) = self.scrollbar_bottom {
                    padding_and_margins.replace(
                        padding_and_margins.map_or(scrollbar_size, |x| x + scrollbar_size),
                    );
                }
                if let Some(margins) = self.vertical_margin() {
                    padding_and_margins
                        .replace(padding_and_margins.map_or(margins, |x| x + margins));
                }
            }
        }

        padding_and_margins
    }

    fn calc_margin(&mut self, container_width: f32, container_height: f32, root_size: (f32, f32)) {
        if let Some(size) = &self.margin_top {
            self.calculated_margin_top =
                Some(size.calc(container_height, root_size.0, root_size.1));
        }
        if let Some(size) = &self.margin_bottom {
            self.calculated_margin_bottom =
                Some(size.calc(container_height, root_size.0, root_size.1));
        }
        if let Some(size) = &self.margin_left {
            self.calculated_margin_left =
                Some(size.calc(container_width, root_size.0, root_size.1));
        }
        if let Some(size) = &self.margin_right {
            self.calculated_margin_right =
                Some(size.calc(container_width, root_size.0, root_size.1));
        }
    }

    fn calc_padding(&mut self, container_width: f32, container_height: f32, root_size: (f32, f32)) {
        if let Some(size) = &self.padding_top {
            self.calculated_padding_top =
                Some(size.calc(container_height, root_size.0, root_size.1));
        }
        if let Some(size) = &self.padding_bottom {
            self.calculated_padding_bottom =
                Some(size.calc(container_height, root_size.0, root_size.1));
        }
        if let Some(size) = &self.padding_left {
            self.calculated_padding_left =
                Some(size.calc(container_width, root_size.0, root_size.1));
        }
        if let Some(size) = &self.padding_right {
            self.calculated_padding_right =
                Some(size.calc(container_width, root_size.0, root_size.1));
        }
    }

    fn calc_borders(&mut self, container_width: f32, container_height: f32, root_size: (f32, f32)) {
        if let Some((color, size)) = &self.border_top {
            self.calculated_border_top = Some((
                *color,
                size.calc(container_height, root_size.0, root_size.1),
            ));
        }
        if let Some((color, size)) = &self.border_bottom {
            self.calculated_border_bottom = Some((
                *color,
                size.calc(container_height, root_size.0, root_size.1),
            ));
        }
        if let Some((color, size)) = &self.border_left {
            self.calculated_border_left =
                Some((*color, size.calc(container_width, root_size.0, root_size.1)));
        }
        if let Some((color, size)) = &self.border_right {
            self.calculated_border_right =
                Some((*color, size.calc(container_width, root_size.0, root_size.1)));
        }
        if let Some(radius) = &self.border_top_left_radius {
            self.calculated_border_top_left_radius =
                Some(radius.calc(container_width, root_size.0, root_size.1));
        }
        if let Some(radius) = &self.border_top_right_radius {
            self.calculated_border_top_right_radius =
                Some(radius.calc(container_width, root_size.0, root_size.1));
        }
        if let Some(radius) = &self.border_bottom_left_radius {
            self.calculated_border_bottom_left_radius =
                Some(radius.calc(container_width, root_size.0, root_size.1));
        }
        if let Some(radius) = &self.border_bottom_right_radius {
            self.calculated_border_bottom_right_radius =
                Some(radius.calc(container_width, root_size.0, root_size.1));
        }
    }

    fn calc_opacity(&mut self, root_size: (f32, f32)) {
        if let Some(opacity) = &self.opacity {
            self.calculated_opacity = Some(opacity.calc(1.0, root_size.0, root_size.1));
        }
    }

    fn calc_hardsized_elements(&mut self) {
        for element in self.visible_elements_mut() {
            element.calc_hardsized_elements();

            if let Some(width) = &element.width {
                match width {
                    Number::Real(x) => {
                        log::trace!(
                            "calc_hardsized_children: setting calculated_width={x}\n{element}"
                        );
                        moosicbox_assert::assert!(*x >= 0.0);
                        element.calculated_width.replace(*x);
                    }
                    Number::Integer(x) => {
                        log::trace!(
                            "calc_hardsized_children: setting calculated_width={x}\n{element}"
                        );
                        #[allow(clippy::cast_precision_loss)]
                        element.calculated_width.replace(*x as f32);
                    }
                    Number::RealPercent(..)
                    | Number::IntegerPercent(..)
                    | Number::Calc(..)
                    | Number::RealVw(..)
                    | Number::IntegerVw(..)
                    | Number::RealVh(..)
                    | Number::IntegerVh(..)
                    | Number::RealDvw(..)
                    | Number::IntegerDvw(..)
                    | Number::RealDvh(..)
                    | Number::IntegerDvh(..) => {}
                }
            }
            if let Some(height) = &element.height {
                match height {
                    Number::Real(x) => {
                        log::trace!(
                            "calc_hardsized_children: setting calculated_height={x}\n{element}"
                        );
                        moosicbox_assert::assert!(*x >= 0.0);
                        element.calculated_height.replace(*x);
                    }
                    Number::Integer(x) => {
                        log::trace!(
                            "calc_hardsized_children: setting calculated_height={x}\n{element}"
                        );
                        #[allow(clippy::cast_precision_loss)]
                        element.calculated_height.replace(*x as f32);
                    }
                    Number::RealPercent(..)
                    | Number::IntegerPercent(..)
                    | Number::Calc(..)
                    | Number::RealVw(..)
                    | Number::IntegerVw(..)
                    | Number::RealVh(..)
                    | Number::IntegerVh(..)
                    | Number::RealDvw(..)
                    | Number::IntegerDvw(..)
                    | Number::RealDvh(..)
                    | Number::IntegerDvh(..) => {}
                }
            }
        }
    }

    fn calc_sized_element_width(&mut self, container_width: f32, root_size: (f32, f32)) -> f32 {
        let width = self
            .width
            .as_ref()
            .unwrap()
            .calc(container_width, root_size.0, root_size.1);
        moosicbox_assert::assert!(width >= 0.0);
        self.calculated_width.replace(width);
        width
    }

    fn calc_sized_element_height(&mut self, container_height: f32, root_size: (f32, f32)) -> f32 {
        let height = self
            .height
            .as_ref()
            .unwrap()
            .calc(container_height, root_size.0, root_size.1);
        self.calculated_height.replace(height);
        height
    }

    fn calc_sized_element_size(
        &mut self,
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
        root_size: (f32, f32),
    ) -> f32 {
        match direction {
            LayoutDirection::Row => {
                let container_height = container_height
                    - self
                        .padding_and_margins(LayoutDirection::Column)
                        .unwrap_or(0.0);
                let width =
                    self.width
                        .as_ref()
                        .unwrap()
                        .calc(container_width, root_size.0, root_size.1);
                let height = self.height.as_ref().map_or(container_height, |x| {
                    x.calc(container_height, root_size.0, root_size.1)
                });
                moosicbox_assert::assert!(width >= 0.0);
                self.calculated_width.replace(width);
                moosicbox_assert::assert!(height >= 0.0);
                self.calculated_height.replace(height);
                log::trace!("calc_sized_element_size (Row): width={width} height={height}");
                width
            }
            LayoutDirection::Column => {
                let container_width = container_width
                    - self
                        .padding_and_margins(LayoutDirection::Row)
                        .unwrap_or(0.0);
                let width = self.width.as_ref().map_or(container_width, |x| {
                    x.calc(container_width, root_size.0, root_size.1)
                });
                let height =
                    self.height
                        .as_ref()
                        .unwrap()
                        .calc(container_height, root_size.0, root_size.1);
                moosicbox_assert::assert!(width >= 0.0);
                self.calculated_width.replace(width);
                moosicbox_assert::assert!(height >= 0.0);
                self.calculated_height.replace(height);
                log::trace!("calc_sized_element_size (Column): width={width} height={height}");
                height
            }
        }
    }

    fn calc_child_margins_and_padding<'a>(
        elements: impl Iterator<Item = &'a mut Self>,
        container_width: f32,
        container_height: f32,
        root_size: (f32, f32),
    ) {
        for element in elements {
            element.calc_margin(container_width, container_height, root_size);
            element.calc_padding(container_width, container_height, root_size);
        }
    }

    fn calc_element_sizes_by_rowcol<'a>(
        arena: &Bump,
        elements: impl Iterator<Item = &'a mut Self>,
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
        mut func: impl FnMut(&mut [&mut Self], f32, f32),
    ) {
        let mut elements = elements.peekable();

        if elements.peek().is_none() {
            return;
        }

        let mut rowcol_index = 0;
        let mut padding_and_margins_x = 0.0;
        let mut padding_and_margins_y = 0.0;
        let buf = &mut bumpalo::collections::Vec::new_in(arena);

        for element in elements {
            log::trace!("calc_element_sizes_by_rowcol: element={element}");
            let current_rowcol_index = element
                .calculated_position
                .as_ref()
                .and_then(|x| match direction {
                    LayoutDirection::Row => x.row(),
                    LayoutDirection::Column => x.column(),
                })
                .unwrap_or(rowcol_index);

            log::trace!(
                "calc_element_sizes_by_rowcol: current_rowcol_index={current_rowcol_index} rowcol_index={rowcol_index}"
            );
            if current_rowcol_index == rowcol_index {
                if let Some(fluff) = element.padding_and_margins(LayoutDirection::Row) {
                    if direction == LayoutDirection::Row {
                        padding_and_margins_x += fluff;
                    } else if fluff > padding_and_margins_x {
                        padding_and_margins_x = fluff;
                    }
                    log::trace!(
                        "calc_element_sizes_by_rowcol: increased padding_and_margins_x={padding_and_margins_x}"
                    );
                }
                if let Some(fluff) = element.padding_and_margins(LayoutDirection::Column) {
                    if direction == LayoutDirection::Column {
                        padding_and_margins_y += fluff;
                    } else if fluff > padding_and_margins_y {
                        padding_and_margins_y = fluff;
                    }
                    log::trace!(
                        "calc_element_sizes_by_rowcol: increased padding_and_margins_y={padding_and_margins_y}"
                    );
                }
                buf.push(element);
                continue;
            }

            log::trace!(
                "calc_element_sizes_by_rowcol: container_width -= {padding_and_margins_x} container_height -= {padding_and_margins_y}"
            );
            let container_width = container_width - padding_and_margins_x;
            let container_height = container_height - padding_and_margins_y;

            func(buf, container_width, container_height);

            rowcol_index = current_rowcol_index;

            if let Some(fluff) = element.padding_and_margins(LayoutDirection::Row) {
                padding_and_margins_x += fluff;
                log::trace!(
                    "calc_element_sizes_by_rowcol: increased padding_and_margins_x={padding_and_margins_x}"
                );
            }
            if let Some(fluff) = element.padding_and_margins(LayoutDirection::Column) {
                padding_and_margins_y += fluff;
                log::trace!(
                    "calc_element_sizes_by_rowcol: increased padding_and_margins_y={padding_and_margins_y}"
                );
            }

            log::trace!(
                "calc_element_sizes_by_rowcol: next rowcol_index={rowcol_index} padding_and_margins_x={padding_and_margins_x} padding_and_margins_y={padding_and_margins_y}"
            );

            buf.push(element);
        }

        if buf.is_empty() {
            log::trace!("calc_element_sizes_by_rowcol: no more items in last buf to process");
            return;
        }

        log::trace!(
            "calc_element_sizes_by_rowcol: container_width -= {padding_and_margins_x} container_height -= {padding_and_margins_y}"
        );
        let container_width = container_width - padding_and_margins_x;
        let container_height = container_height - padding_and_margins_y;

        log::trace!("calc_element_sizes_by_rowcol: processing last buf");
        func(buf, container_width, container_height);
    }

    fn calc_unsized_element_size(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        relative_size: Option<(f32, f32)>,
        root_size: (f32, f32),
        remainder: f32,
    ) {
        let (Some(container_width), Some(container_height)) = (
            self.calculated_width_minus_borders(),
            self.calculated_height_minus_borders(),
        ) else {
            moosicbox_assert::die_or_panic!(
                "calc_unsized_element_size requires calculated_width and calculated_height to be set"
            );
        };
        Self::calc_unsized_element_sizes(
            arena,
            font_metrics,
            relative_size,
            root_size,
            relative_positioned_elements_mut(&mut self.children),
            self.direction,
            container_width,
            container_height,
            remainder,
        );
    }

    #[allow(clippy::cognitive_complexity, clippy::too_many_arguments)]
    fn calc_unsized_element_sizes<'a>(
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        relative_size: Option<(f32, f32)>,
        root_size: (f32, f32),
        elements: impl Iterator<Item = &'a mut Self>,
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
        remainder: f32,
    ) {
        let mut elements = elements.peekable();
        if elements.peek().is_none() {
            return;
        }

        moosicbox_assert::assert!(
            container_width >= 0.0,
            "container_width ({container_width}) must be >= 0.0"
        );
        moosicbox_assert::assert!(
            container_height >= 0.0,
            "container_height ({container_height}) must be >= 0.0"
        );
        moosicbox_assert::assert!(remainder >= 0.0, "remainder ({remainder}) must be >= 0.0");

        let mut elements = elements.collect_vec();

        #[allow(clippy::cast_precision_loss)]
        let evenly_split_remaining_size = remainder / (elements.len() as f32);

        moosicbox_logging::debug_or_trace!(
            (
                "calc_unsized_element_sizes: setting {} to evenly_split_remaining_size={evenly_split_remaining_size}",
                if direction == LayoutDirection::Row {
                    "width"
                } else {
                    "height"
                },
            ),
            (
                "calc_unsized_element_sizes: setting {} to evenly_split_remaining_size={evenly_split_remaining_size}{}",
                if direction == LayoutDirection::Row {
                    "width"
                } else {
                    "height"
                },
                if elements.is_empty() {
                    String::new()
                } else {
                    format!(
                        "\n{}",
                        elements
                            .iter()
                            .map(|x| format!("{x}"))
                            .collect_vec()
                            .join("\n")
                    )
                }
            )
        );

        for element in &mut *elements {
            match direction {
                LayoutDirection::Row => {
                    let height = element.height.as_ref().map_or(container_height, |x| {
                        x.calc(container_height, root_size.0, root_size.1)
                    });
                    moosicbox_assert::assert!(height >= 0.0);
                    element.calculated_height.replace(height);

                    let width = evenly_split_remaining_size;
                    moosicbox_assert::assert!(width >= 0.0);
                    element.calculated_width.replace(width);
                }
                LayoutDirection::Column => {
                    let width = element.width.as_ref().map_or(container_width, |x| {
                        x.calc(container_width, root_size.0, root_size.1)
                    });
                    moosicbox_assert::assert!(width >= 0.0);
                    element.calculated_width.replace(width);

                    let height = evenly_split_remaining_size;
                    moosicbox_assert::assert!(height >= 0.0);
                    element.calculated_height.replace(height);
                }
            }
        }

        for element in elements {
            element.calc_inner(arena, font_metrics, relative_size, root_size);
        }
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn handle_overflow(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        relative_size: Option<(f32, f32)>,
        root_size: (f32, f32),
    ) -> bool {
        log::trace!("handle_overflow: processing self\n{self}");
        let mut layout_shifted = false;

        let direction = self.direction;
        let overflow = self.overflow_x;
        let container_width = self.calculated_width_minus_borders().unwrap_or(0.0);
        let container_height = self.calculated_height_minus_borders().unwrap_or(0.0);

        let mut x = 0.0;
        let mut y = 0.0;
        let mut max_width = 0.0;
        let mut max_height = 0.0;
        let mut row = 0;
        let mut col = 0;

        let gap_x = self
            .column_gap
            .as_ref()
            .map(|x| x.calc(container_width, root_size.0, root_size.1));
        let gap_y = self
            .row_gap
            .as_ref()
            .map(|x| x.calc(container_height, root_size.0, root_size.1));

        let relative_size = self.get_relative_size().or(relative_size);

        for container in self.relative_positioned_elements_mut().inspect(|element| {
            log::trace!("handle_overflow: processing child element\n{element}");
        }) {
            // TODO:
            // need to handle non container elements that have a width/height that is the split
            // remainder of the container width/height
            container.handle_overflow(arena, font_metrics, relative_size, root_size);
            let width = container.calculated_width_minus_borders().unwrap_or(0.0);
            let height = container.calculated_height_minus_borders().unwrap_or(0.0);

            let mut current_row = row;
            let mut current_col = col;

            match overflow {
                LayoutOverflow::Auto
                | LayoutOverflow::Scroll
                | LayoutOverflow::Expand
                | LayoutOverflow::Hidden
                | LayoutOverflow::Squash => {
                    match direction {
                        LayoutDirection::Row => {
                            x += width;
                        }
                        LayoutDirection::Column => {
                            y += height;
                        }
                    }

                    container
                        .calculated_position
                        .replace(LayoutPosition::default());
                }
                LayoutOverflow::Wrap => {
                    match direction {
                        LayoutDirection::Row => {
                            let next_row = x > 0.0 && x + width > container_width;
                            log::trace!(
                                "handle_overflow: {x} > 0.0 && {x} + {width} > {container_width} = {next_row}"
                            );
                            if next_row {
                                x = 0.0;
                                y += max_height;
                                max_height = 0.0;
                                row += 1;
                                col = 0;
                                current_row = row;
                                current_col = col;
                            }
                            x += width;
                            if let Some(gap) = gap_x {
                                x += gap;
                            }
                            col += 1;
                        }
                        LayoutDirection::Column => {
                            let next_col = y > 0.0 && y + height > container_height;
                            log::trace!(
                                "handle_overflow: {y} > 0.0 && {y} + {height} > {container_height} = {next_col}"
                            );
                            if next_col {
                                y = 0.0;
                                x += max_width;
                                max_width = 0.0;
                                col += 1;
                                row = 0;
                                current_row = row;
                                current_col = col;
                            }
                            y += height;
                            if let Some(gap) = gap_y {
                                y += gap;
                            }
                            row += 1;
                        }
                    }

                    let updated = if let Some(LayoutPosition::Wrap {
                        row: old_row,
                        col: old_col,
                    }) = container.calculated_position
                    {
                        if current_row != old_row || current_col != old_col {
                            log::trace!(
                                "handle_overflow: layout_shifted because current_row != old_row || current_col != old_col ({current_row} != {old_row} || {current_col} != {old_col})"
                            );
                            layout_shifted = true;
                            true
                        } else {
                            false
                        }
                    } else {
                        true
                    };

                    if updated {
                        log::trace!(
                            "handle_overflow: setting element row/col ({current_row}, {current_col})"
                        );
                        container.calculated_position.replace(LayoutPosition::Wrap {
                            row: current_row,
                            col: current_col,
                        });
                    }
                }
            }

            max_height = if max_height > height {
                max_height
            } else {
                height
            };
            max_width = if max_width > width { max_width } else { width };
        }

        if self.resize_children(arena, font_metrics, root_size) {
            log::trace!("handle_overflow: layout_shifted because children were resized");
            layout_shifted = true;
        }

        self.position_children(relative_size, root_size);

        layout_shifted
    }

    pub fn increase_margin_left(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_margin_left, value)
    }

    pub fn increase_margin_right(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_margin_right, value)
    }

    pub fn increase_margin_top(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_margin_top, value)
    }

    pub fn increase_margin_bottom(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_margin_bottom, value)
    }

    pub fn increase_padding_left(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_padding_left, value)
    }

    pub fn increase_padding_right(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_padding_right, value)
    }

    pub fn increase_padding_top(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_padding_top, value)
    }

    pub fn increase_padding_bottom(&mut self, value: f32) -> f32 {
        increase_opt(&mut self.internal_padding_bottom, value)
    }

    /// # Panics
    ///
    /// * If size is not calculated
    #[must_use]
    pub fn get_relative_size(&self) -> Option<(f32, f32)> {
        if self.position == Some(Position::Relative) {
            Some((
                self.calculated_width.unwrap(),
                self.calculated_height.unwrap(),
            ))
        } else {
            None
        }
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn position_children(&mut self, relative_size: Option<(f32, f32)>, root_size: (f32, f32)) {
        log::trace!("position_children");

        let (Some(container_width), Some(container_height)) =
            (self.calculated_width, self.calculated_height)
        else {
            moosicbox_assert::die_or_panic!("position_children: missing width and/or height");
        };

        let mut x = 0.0;
        let mut y = 0.0;
        let mut max_width = 0.0;
        let mut max_height = 0.0;
        let mut horizontal_margin = None;
        let mut vertical_margin = None;

        let columns = self.columns();
        let rows = self.rows();
        let mut remainder_width = 0.0;
        let mut remainder_height = 0.0;
        let mut child_horizontal_offset = 0.0;
        let mut child_vertical_offset = 0.0;

        // TODO: Handle variable amount of items in rows/cols (i.e., non-uniform row/cols wrapping)
        match self.justify_content.unwrap_or_default() {
            #[allow(clippy::cast_precision_loss)]
            JustifyContent::Start => match self.direction {
                LayoutDirection::Row => {
                    remainder_width = container_width - self.contained_calculated_width();
                    child_horizontal_offset = 0.0;
                }
                LayoutDirection::Column => {
                    remainder_height = container_height - self.contained_calculated_height();
                    child_vertical_offset = 0.0;
                }
            },
            #[allow(clippy::cast_precision_loss)]
            JustifyContent::Center => match self.direction {
                LayoutDirection::Row => {
                    remainder_width = container_width - self.contained_calculated_width();
                    child_horizontal_offset = remainder_width / 2.0;
                }
                LayoutDirection::Column => {
                    remainder_height = container_height - self.contained_calculated_height();
                    child_vertical_offset = remainder_height / 2.0;
                }
            },
            #[allow(clippy::cast_precision_loss)]
            JustifyContent::End => match self.direction {
                LayoutDirection::Row => {
                    remainder_width = container_width - self.contained_calculated_width();
                    child_horizontal_offset = remainder_width;
                }
                LayoutDirection::Column => {
                    remainder_height = container_height - self.contained_calculated_height();
                    child_vertical_offset = remainder_height;
                }
            },
            #[allow(clippy::cast_precision_loss)]
            JustifyContent::SpaceBetween => match self.direction {
                LayoutDirection::Row => {
                    remainder_width = container_width - self.contained_calculated_width();
                    let margin = remainder_width / ((columns - 1) as f32);
                    horizontal_margin = Some(margin);
                }
                LayoutDirection::Column => {
                    remainder_height = container_height - self.contained_calculated_height();
                    let margin = remainder_height / ((rows - 1) as f32);
                    vertical_margin = Some(margin);
                }
            },
            #[allow(clippy::cast_precision_loss)]
            JustifyContent::SpaceEvenly => match self.direction {
                LayoutDirection::Row => {
                    remainder_width = container_width - self.contained_calculated_width();
                    let margin = remainder_width / ((columns + 1) as f32);
                    horizontal_margin = Some(margin);
                }
                LayoutDirection::Column => {
                    remainder_height = container_height - self.contained_calculated_height();
                    let margin = remainder_height / ((rows + 1) as f32);
                    vertical_margin = Some(margin);
                }
            },
        }

        let mut first_horizontal_margin = horizontal_margin;
        let mut first_vertical_margin = vertical_margin;

        if let Some(gap) = &self.column_gap {
            let gap_x = gap.calc(container_width, root_size.0, root_size.1);

            if let Some(margin) = horizontal_margin {
                if gap_x > margin {
                    horizontal_margin.replace(gap_x);

                    if self.justify_content == Some(JustifyContent::SpaceEvenly) {
                        #[allow(clippy::cast_precision_loss)]
                        first_horizontal_margin
                            .replace(gap_x.mul_add(-((columns - 1) as f32), remainder_width) / 2.0);
                    }
                }
            } else {
                horizontal_margin = Some(gap_x);
            }
        }

        if let Some(gap) = &self.row_gap {
            let gap_y = gap.calc(container_height, root_size.0, root_size.1);
            if let Some(margin) = vertical_margin {
                if gap_y > margin {
                    vertical_margin.replace(gap_y);

                    if self.justify_content == Some(JustifyContent::SpaceEvenly) {
                        #[allow(clippy::cast_precision_loss)]
                        first_vertical_margin
                            .replace(gap_y.mul_add(-((rows - 1) as f32), remainder_height) / 2.0);
                    }
                }
            } else {
                vertical_margin = Some(gap_y);
            }
        }

        if child_horizontal_offset > 0.0 {
            self.internal_padding_left = Some(child_horizontal_offset);
        }
        if child_vertical_offset > 0.0 {
            self.internal_padding_top = Some(child_vertical_offset);
        }

        for element in relative_positioned_elements_mut(&mut self.children) {
            element.internal_margin_left.take();
            element.internal_margin_top.take();

            let (Some(width), Some(height), Some(position)) = (
                element.bounding_calculated_width(),
                element.bounding_calculated_height(),
                element.calculated_position.as_ref(),
            ) else {
                moosicbox_assert::die_or_warn!(
                    "position_children: missing width, height, and/or position. continuing on to next element"
                );
                continue;
            };

            log::trace!(
                "position_children: x={x} y={y} width={width} height={height} position={position:?} child=\n{element}"
            );

            if let LayoutPosition::Wrap { row, col } = position {
                if self.justify_content == Some(JustifyContent::SpaceEvenly) || *col > 0 {
                    let hmargin = if *col == 0 {
                        first_horizontal_margin
                    } else {
                        horizontal_margin
                    };
                    if let Some(margin) = hmargin {
                        if self.direction == LayoutDirection::Row || *row == 0 {
                            x += margin;
                        }
                        element.internal_margin_left.replace(margin);
                    }
                }
                if self.justify_content == Some(JustifyContent::SpaceEvenly) || *row > 0 {
                    let vmargin = if *row == 0 {
                        first_vertical_margin
                    } else {
                        vertical_margin
                    };
                    if let Some(margin) = vmargin {
                        if self.direction == LayoutDirection::Column || *col == 0 {
                            y += margin;
                        }
                        element.internal_margin_top.replace(margin);
                    }
                }
            }

            element.calculated_x.replace(x);
            element.calculated_y.replace(y);

            match self.direction {
                LayoutDirection::Row => {
                    match position {
                        LayoutPosition::Wrap { col, .. } => {
                            if *col == 0 {
                                x = if self.justify_content == Some(JustifyContent::SpaceEvenly) {
                                    horizontal_margin.unwrap_or(0.0)
                                } else {
                                    0.0
                                };
                                y += max_height;
                                max_height = 0.0;
                                element.calculated_x.replace(x);
                                element.calculated_y.replace(y);
                            }
                        }
                        LayoutPosition::Default => {}
                    }
                    x += width;
                }
                LayoutDirection::Column => {
                    match position {
                        LayoutPosition::Wrap { row, .. } => {
                            if *row == 0 {
                                y = if self.justify_content == Some(JustifyContent::SpaceEvenly) {
                                    vertical_margin.unwrap_or(0.0)
                                } else {
                                    0.0
                                };
                                x += max_width;
                                max_width = 0.0;
                                element.calculated_x.replace(x);
                                element.calculated_y.replace(y);
                            }
                        }
                        LayoutPosition::Default => {}
                    }
                    y += height;
                }
            }

            max_height = if max_height > height {
                max_height
            } else {
                height
            };
            max_width = if max_width > width { max_width } else { width };
        }

        for element in absolute_positioned_elements_mut(&mut self.children) {
            if let Some((width, height)) = relative_size {
                if let Some(left) = &element.left {
                    element.calculated_x = Some(left.calc(width, root_size.0, root_size.1));
                }
                if let Some(right) = &element.right {
                    let offset = right.calc(width, root_size.0, root_size.1);
                    let bounding_width = element.bounding_calculated_width().unwrap();
                    element.calculated_x = Some(width - offset - bounding_width);
                    log::trace!(
                        "position_children: absolute position right={right} calculated_x={} width={width} offset={offset} bounding_width={bounding_width}",
                        element.calculated_x.unwrap()
                    );
                }
                if let Some(top) = &element.top {
                    element.calculated_y = Some(top.calc(height, root_size.0, root_size.1));
                }
                if let Some(bottom) = &element.bottom {
                    let offset = bottom.calc(height, root_size.0, root_size.1);
                    let bounding_height = element.bounding_calculated_height().unwrap();
                    element.calculated_y = Some(height - offset - bounding_height);
                    log::trace!(
                        "position_children: absolute position bottom={bottom} calculated_y={} height={height} offset={offset} bounding_height={bounding_height}",
                        element.calculated_y.unwrap()
                    );
                }

                if element.calculated_x.is_none() {
                    element.calculated_x = Some(0.0);
                }
                if element.calculated_y.is_none() {
                    element.calculated_y = Some(0.0);
                }
            } else {
                element.calculated_x = Some(0.0);
                element.calculated_y = Some(0.0);
            }
        }

        for element in fixed_positioned_elements_mut(&mut self.children) {
            let (width, height) = root_size;

            if let Some(left) = &element.left {
                element.calculated_x = Some(left.calc(width, root_size.0, root_size.1));
            }
            if let Some(right) = &element.right {
                let offset = right.calc(width, root_size.0, root_size.1);
                let bounding_width = element.bounding_calculated_width().unwrap();
                element.calculated_x = Some(width - offset - bounding_width);
                log::trace!(
                    "position_children: fixed position right={right} calculated_x={} width={width} offset={offset} bounding_width={bounding_width}",
                    element.calculated_x.unwrap()
                );
            }
            if let Some(top) = &element.top {
                element.calculated_y = Some(top.calc(height, root_size.0, root_size.1));
            }
            if let Some(bottom) = &element.bottom {
                let offset = bottom.calc(height, root_size.0, root_size.1);
                let bounding_height = element.bounding_calculated_height().unwrap();
                element.calculated_y = Some(height - offset - bounding_height);
                log::trace!(
                    "position_children: fixed position bottom={bottom} calculated_y={} height={height} offset={offset} bounding_height={bounding_height}",
                    element.calculated_y.unwrap()
                );
            }

            if element.calculated_x.is_none() {
                element.calculated_x = Some(0.0);
            }
            if element.calculated_y.is_none() {
                element.calculated_y = Some(0.0);
            }
        }
    }

    pub fn contained_sized_width(&self, root_size: (f32, f32), recurse: bool) -> Option<f32> {
        let Some(calculated_width) = self.calculated_width else {
            moosicbox_assert::die_or_panic!(
                "calculated_width is required to get the contained_sized_width"
            );
        };

        match self.direction {
            LayoutDirection::Row => self
                .relative_positioned_elements()
                .chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { row, .. } => Some(row),
                        LayoutPosition::Default => None,
                    })
                })
                .into_iter()
                .filter_map(|(_, elements)| {
                    let mut widths = elements
                        .filter_map(|x| {
                            x.width
                                .as_ref()
                                .map(|x| x.calc(calculated_width, root_size.0, root_size.1))
                                .or_else(|| {
                                    if recurse {
                                        x.contained_sized_width(root_size, recurse)
                                    } else {
                                        None
                                    }
                                })
                        })
                        .peekable();

                    if widths.peek().is_some() {
                        Some(widths.sum())
                    } else {
                        None
                    }
                })
                .max_by(order_float),
            LayoutDirection::Column => {
                let columns = self.relative_positioned_elements().chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { col, .. } => Some(col),
                        LayoutPosition::Default => None,
                    })
                });

                let mut widths = columns
                    .into_iter()
                    .filter_map(|(_, elements)| {
                        elements
                            .filter_map(|x| {
                                x.width
                                    .as_ref()
                                    .map(|x| x.calc(calculated_width, root_size.0, root_size.1))
                                    .or_else(|| {
                                        if recurse {
                                            x.contained_sized_width(root_size, recurse)
                                        } else {
                                            None
                                        }
                                    })
                            })
                            .max_by(order_float)
                    })
                    .peekable();

                if widths.peek().is_some() {
                    Some(widths.sum())
                } else {
                    None
                }
            }
        }
    }

    pub fn contained_sized_height(&self, root_size: (f32, f32), recurse: bool) -> Option<f32> {
        let Some(calculated_height) = self.calculated_height else {
            moosicbox_assert::die_or_panic!(
                "calculated_height is required to get the contained_sized_height"
            );
        };

        match self.direction {
            LayoutDirection::Row => {
                let rows = self.relative_positioned_elements().chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { row, .. } => Some(row),
                        LayoutPosition::Default => None,
                    })
                });

                let mut heights = rows
                    .into_iter()
                    .filter_map(|(_, elements)| {
                        elements
                            .filter_map(|x| {
                                x.height
                                    .as_ref()
                                    .map(|x| x.calc(calculated_height, root_size.0, root_size.1))
                                    .or_else(|| {
                                        if recurse {
                                            x.contained_sized_height(root_size, recurse)
                                        } else {
                                            None
                                        }
                                    })
                            })
                            .max_by(order_float)
                    })
                    .peekable();

                if heights.peek().is_some() {
                    Some(heights.sum())
                } else {
                    None
                }
            }
            LayoutDirection::Column => self
                .relative_positioned_elements()
                .chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { col, .. } => Some(col),
                        LayoutPosition::Default => None,
                    })
                })
                .into_iter()
                .filter_map(|(_, elements)| {
                    let mut heights = elements
                        .filter_map(|x| {
                            x.height
                                .as_ref()
                                .map(|x| x.calc(calculated_height, root_size.0, root_size.1))
                                .or_else(|| {
                                    if recurse {
                                        x.contained_sized_height(root_size, recurse)
                                    } else {
                                        None
                                    }
                                })
                        })
                        .peekable();

                    if heights.peek().is_some() {
                        Some(heights.sum())
                    } else {
                        None
                    }
                })
                .max_by(order_float),
        }
    }

    #[must_use]
    pub fn contained_calculated_width(&self) -> f32 {
        log::trace!(
            "contained_calculated_width: direction={} element_count={} position={:?}",
            self.direction,
            self.children.len(),
            self.children
                .first()
                .map(|x| x.calculated_position.as_ref())
        );

        let width = match self.direction {
            LayoutDirection::Row => self
                .relative_positioned_elements()
                .chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { row, .. } => Some(row),
                        LayoutPosition::Default => None,
                    })
                })
                .into_iter()
                .inspect(|(row, _elements)| {
                    log::trace!("contained_calculated_width: row={row:?}");
                })
                .map(|(row, elements)| {
                    let mut len = 0;
                    let sum = elements
                        .inspect(|x| {
                            len += 1;
                            log::trace!("contained_calculated_width: element:\n{x}");
                        })
                        .filter_map(Self::bounding_calculated_width)
                        .sum();

                    log::trace!(
                        "contained_calculated_width: summed row {row:?} with {len} children: {sum}"
                    );

                    sum
                })
                .max_by(order_float)
                .unwrap_or(0.0),
            LayoutDirection::Column => self
                .relative_positioned_elements()
                .chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { col, .. } => Some(col),
                        LayoutPosition::Default => None,
                    })
                })
                .into_iter()
                .inspect(|(col, _elements)| {
                    log::trace!("contained_calculated_width: col={col:?}");
                })
                .map(|(col, elements)| {
                    let mut len = 0;
                    let max = elements
                        .inspect(|x| {
                            len += 1;
                            log::trace!("contained_calculated_width: element:\n{x}");
                        })
                        .filter_map(Self::bounding_calculated_width)
                        .max_by(order_float)
                        .unwrap_or(0.0);

                    log::trace!(
                        "contained_calculated_width: maxed col {col:?} with {len} children: {max}"
                    );

                    max
                })
                .max_by(order_float)
                .unwrap_or(0.0),
        };

        log::trace!("contained_calculated_width: width={width}");

        width
    }

    #[must_use]
    pub fn contained_calculated_height(&self) -> f32 {
        let height = match self.direction {
            LayoutDirection::Row => self
                .relative_positioned_elements()
                .chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { row, .. } => Some(row),
                        LayoutPosition::Default => None,
                    })
                })
                .into_iter()
                .inspect(|(row, _elements)| {
                    log::trace!("contained_calculated_height: row={row:?}");
                })
                .map(|(row, elements)| {
                    let mut len = 0;
                    let max = elements
                        .inspect(|x| {
                            len += 1;
                            log::trace!("contained_calculated_height: element:\n{x}");
                        })
                        .filter_map(Self::bounding_calculated_height)
                        .max_by(order_float)
                        .unwrap_or(0.0);

                    log::trace!(
                        "contained_calculated_height: maxed row {row:?} with {len} children: {max}"
                    );

                    max
                })
                .sum(),
            LayoutDirection::Column => self
                .relative_positioned_elements()
                .chunk_by(|x| {
                    x.calculated_position.as_ref().and_then(|x| match x {
                        LayoutPosition::Wrap { col, .. } => Some(col),
                        LayoutPosition::Default => None,
                    })
                })
                .into_iter()
                .inspect(|(col, _elements)| {
                    log::trace!("contained_calculated_height: col={col:?}");
                })
                .map(|(col, elements)| {
                    let mut len = 0;
                    let sum = elements
                        .inspect(|x| {
                            len += 1;
                            log::trace!("contained_calculated_height: element:\n{x}");
                        })
                        .filter_map(Self::bounding_calculated_height)
                        .sum();

                    log::trace!(
                        "contained_calculated_height: summed col {col:?} with {len} children: {sum}"
                    );

                    sum
                })
                .max_by(order_float)
                .unwrap_or(0.0),
        };

        log::trace!("contained_calculated_height: height={height}");

        height
    }

    pub fn iter_row(&self, row: u32) -> impl Iterator<Item = &Self> {
        Self::elements_iter_row(self.relative_positioned_elements(), row)
    }

    pub fn iter_column(&self, column: u32) -> impl Iterator<Item = &Self> {
        Self::elements_iter_column(self.relative_positioned_elements(), column)
    }

    pub fn elements_iter_row<'a>(
        elements: impl Iterator<Item = &'a Self>,
        row: u32,
    ) -> impl Iterator<Item = &'a Self> {
        elements.filter(move |x| {
            x.calculated_position
                .as_ref()
                .is_some_and(|x| x.row().is_some_and(|x| x == row))
        })
    }

    pub fn elements_iter_column<'a>(
        elements: impl Iterator<Item = &'a Self>,
        column: u32,
    ) -> impl Iterator<Item = &'a Self> {
        elements.filter(move |x| {
            x.calculated_position
                .as_ref()
                .is_some_and(|x| x.column().is_some_and(|x| x == column))
        })
    }

    /// # Panics
    ///
    /// * If there are more rows than can fit in a u32
    #[must_use]
    pub fn rows(&self) -> u32 {
        if self.overflow_x != LayoutOverflow::Wrap && self.overflow_y != LayoutOverflow::Wrap {
            match self.direction {
                LayoutDirection::Row => 1,
                LayoutDirection::Column => {
                    u32::try_from(self.relative_positioned_elements().count()).unwrap()
                }
            }
        } else {
            self.relative_positioned_elements()
                .filter_map(|x| x.calculated_position.as_ref())
                .filter_map(LayoutPosition::row)
                .max()
                .unwrap_or(0)
                + 1
        }
    }

    /// # Panics
    ///
    /// * If there are more columns than can fit in a u32
    #[must_use]
    pub fn columns(&self) -> u32 {
        if self.overflow_x != LayoutOverflow::Wrap && self.overflow_y != LayoutOverflow::Wrap {
            match self.direction {
                LayoutDirection::Row => {
                    u32::try_from(self.relative_positioned_elements().count()).unwrap()
                }
                LayoutDirection::Column => 1,
            }
        } else {
            self.relative_positioned_elements()
                .filter_map(|x| x.calculated_position.as_ref())
                .filter_map(LayoutPosition::column)
                .max()
                .unwrap_or(0)
                + 1
        }
    }

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
    pub(crate) fn internal_horizontal_margin(&self) -> Option<f32> {
        let mut margin = None;
        if let Some(margin_left) = self.internal_margin_left {
            margin = Some(margin_left);
        }
        if let Some(margin_right) = self.internal_margin_right {
            margin.replace(margin.map_or(margin_right, |x| x + margin_right));
        }
        margin
    }

    #[must_use]
    pub(crate) fn internal_vertical_margin(&self) -> Option<f32> {
        let mut margin = None;
        if let Some(margin_top) = self.internal_margin_top {
            margin = Some(margin_top);
        }
        if let Some(margin_bottom) = self.internal_margin_bottom {
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
    pub fn calculated_width_plus_margin(&self) -> Option<f32> {
        self.calculated_width.map(|x| {
            self.internal_horizontal_margin().map_or(x, |margin| {
                let x = x + margin;
                if x < 0.0 { 0.0 } else { x }
            })
        })
    }

    #[must_use]
    pub fn calculated_height_plus_margin(&self) -> Option<f32> {
        self.calculated_height.map(|x| {
            self.internal_vertical_margin().map_or(x, |margin| {
                let x = x + margin;
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

    #[allow(
        clippy::too_many_lines,
        clippy::cognitive_complexity,
        clippy::similar_names
    )]
    pub(crate) fn resize_children(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        root_size: (f32, f32),
    ) -> bool {
        if matches!(
            self.element,
            Element::Table
                | Element::TR
                | Element::TBody
                | Element::TD
                | Element::THead
                | Element::TH
        ) {
            log::trace!("resize_children: not resizing table");
            return false;
        }

        if self
            .relative_positioned_elements()
            .peekable()
            .peek()
            .is_none()
        {
            log::trace!("resize_children: no children");
            return false;
        }

        let (Some(mut width), Some(mut height)) = (
            self.calculated_width_minus_borders(),
            self.calculated_height_minus_borders(),
        ) else {
            moosicbox_assert::die_or_panic!(
                "Container missing calculated_width and/or calculated_height: {self:?}"
            );
        };

        let contained_calculated_width = self.contained_calculated_width();
        let contained_calculated_height = self.contained_calculated_height();

        log::trace!(
            "resize_children: calculated_width={width} contained_calculated_width={contained_calculated_width} calculated_height={height} contained_calculated_height={contained_calculated_height} {} overflow_x={} overflow_y={} width={:?} height={:?}",
            self.direction,
            self.overflow_x,
            self.overflow_y,
            self.width,
            self.height,
        );

        // TODO: Might not need to return out of these, might be able to just update the
        // contained_calculated_width and/or contained_calculated_height properties
        if self.check_scrollbar_x_changed(&mut width, height, contained_calculated_height) {
            self.evenly_distribute_children_width(arena, font_metrics, width, root_size);

            return true;
        }

        if self.check_scrollbar_y_changed(width, &mut height, contained_calculated_width) {
            self.evenly_distribute_children_height(arena, font_metrics, height, root_size);

            return true;
        }

        let mut resized = false;

        if width < contained_calculated_width - EPSILON {
            log::trace!(
                "resize_children: width < contained_calculated_width (width={width} contained_calculated_width={contained_calculated_width})"
            );
            match self.overflow_x {
                LayoutOverflow::Auto | LayoutOverflow::Scroll | LayoutOverflow::Hidden => {}
                LayoutOverflow::Expand => {
                    if self.width.is_none()
                        && (self.calculated_width.unwrap() - contained_calculated_width).abs()
                            > EPSILON
                    {
                        log::trace!(
                            "resize_children: resized because contained_calculated_width changed from {} to {contained_calculated_width}",
                            self.calculated_width.unwrap()
                        );
                        moosicbox_assert::assert!(contained_calculated_width >= 0.0);
                        self.calculated_width.replace(contained_calculated_width);
                        resized = true;
                    }
                }
                LayoutOverflow::Wrap | LayoutOverflow::Squash => {
                    resized = self.evenly_distribute_children_width(
                        arena,
                        font_metrics,
                        width,
                        root_size,
                    ) || resized;
                }
            }
        } else {
            log::trace!(
                "resize_children: width={width} currently contains all of the contained_calculated_width={contained_calculated_width}"
            );
        }

        if height < contained_calculated_height - EPSILON {
            log::trace!(
                "resize_children: height < contained_calculated_height (height={height} contained_calculated_height={contained_calculated_height})"
            );
            match self.overflow_y {
                LayoutOverflow::Auto | LayoutOverflow::Scroll | LayoutOverflow::Hidden => {}
                LayoutOverflow::Expand => {
                    if self.height.is_none()
                        && (self.calculated_height.unwrap() - contained_calculated_height).abs()
                            > EPSILON
                    {
                        log::trace!(
                            "resize_children: resized because contained_calculated_height changed from {} to {contained_calculated_height}",
                            self.calculated_height.unwrap()
                        );
                        moosicbox_assert::assert!(contained_calculated_height >= 0.0);
                        self.calculated_height.replace(contained_calculated_height);
                        resized = true;
                    }
                }
                LayoutOverflow::Wrap | LayoutOverflow::Squash => {
                    resized = self.evenly_distribute_children_height(
                        arena,
                        font_metrics,
                        height,
                        root_size,
                    ) || resized;
                }
            }
        } else {
            log::trace!(
                "resize_children: height={height} currently contains all of the contained_calculated_height={contained_calculated_height}"
            );
        }

        if resized {
            let (Some(new_width), Some(new_height)) = (
                self.calculated_width_minus_borders(),
                self.calculated_height_minus_borders(),
            ) else {
                moosicbox_assert::die_or_panic!(
                    "Container missing calculated_width and/or calculated_height: {self:?}"
                );
            };

            log::trace!(
                "resize_children: original_height={height} -> new_height={new_height} original_width={width} -> new_width={new_width}"
            );
        }

        resized
    }

    fn check_scrollbar_x_changed(
        &mut self,
        container_width: &mut f32,
        container_height: f32,
        contained_calculated_height: f32,
    ) -> bool {
        if self.overflow_y == LayoutOverflow::Scroll
            || contained_calculated_height > container_height
                && self.overflow_y == LayoutOverflow::Auto
        {
            if self.scrollbar_right.is_none() {
                let scrollbar_size = f32::from(get_scrollbar_size());
                self.scrollbar_right.replace(scrollbar_size);
                let new_width = self.calculated_width.unwrap() - scrollbar_size;
                moosicbox_assert::assert!(new_width >= 0.0);
                self.calculated_width = Some(new_width);
                *container_width = self.calculated_width_minus_borders().unwrap();
                log::trace!(
                    "resize_children: resized because vertical scrollbar is now visible and affected children elements, setting scrollbar_right to {scrollbar_size} new_width={new_width}"
                );
                return true;
            }
        } else if let Some(scrollbar_size) = self.scrollbar_right {
            self.scrollbar_right.take();
            let new_width = self.calculated_width.unwrap() + scrollbar_size;
            moosicbox_assert::assert!(new_width >= 0.0);
            self.calculated_width = Some(new_width);
            *container_width = self.calculated_width_minus_borders().unwrap();
            log::trace!(
                "resize_children: resized because vertical scrollbar is not visible anymore and affected children elements"
            );
            return true;
        }

        false
    }

    fn check_scrollbar_y_changed(
        &mut self,
        container_width: f32,
        container_height: &mut f32,
        contained_calculated_width: f32,
    ) -> bool {
        if self.overflow_x == LayoutOverflow::Scroll
            || contained_calculated_width > container_width
                && self.overflow_x == LayoutOverflow::Auto
        {
            if self.scrollbar_bottom.is_none() {
                let scrollbar_size = f32::from(get_scrollbar_size());
                self.scrollbar_bottom.replace(scrollbar_size);
                let new_height = self.calculated_height.unwrap() - scrollbar_size;
                moosicbox_assert::assert!(new_height >= 0.0);
                self.calculated_height = Some(new_height);
                *container_height = self.calculated_height_minus_borders().unwrap();
                log::trace!(
                    "resize_children: resized because horizontal scrollbar is now visible and affected children elements, setting scrollbar_bottom to {scrollbar_size} new_height={new_height}"
                );
                return true;
            }
        } else if let Some(scrollbar_size) = self.scrollbar_bottom {
            self.scrollbar_bottom.take();
            let new_height = self.calculated_height.unwrap() + scrollbar_size;
            moosicbox_assert::assert!(new_height >= 0.0);
            self.calculated_height = Some(new_height);
            *container_height = self.calculated_height_minus_borders().unwrap();
            log::trace!(
                "resize_children: resized because horizontal scrollbar is not visible anymore and affected children elements"
            );
            return true;
        }

        false
    }

    fn evenly_distribute_children_width(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        width: f32,
        root_size: (f32, f32),
    ) -> bool {
        let mut resized = false;
        let contained_sized_width = self.contained_sized_width(root_size, false).unwrap_or(0.0);
        #[allow(clippy::cast_precision_loss)]
        let evenly_split_remaining_size = if width > contained_sized_width {
            (width - contained_sized_width) / (self.columns() as f32)
        } else {
            0.0
        };
        log::trace!(
            "evenly_distribute_children_width: width={width} contained_sized_width={contained_sized_width} evenly_split_remaining_size={evenly_split_remaining_size}"
        );

        for element in self
            .relative_positioned_elements_mut()
            .filter(|x| x.width.is_none())
        {
            let element_width =
                evenly_split_remaining_size - element.horizontal_padding().unwrap_or(0.0);

            if let Some(existing) = element.calculated_width {
                if (existing - element_width).abs() > 0.01 {
                    moosicbox_assert::assert!(element_width >= 0.0);
                    element.calculated_width.replace(element_width);
                    resized = true;
                    log::trace!(
                        "evenly_distribute_children_width: resized because child calculated_width was different ({existing} != {element_width})"
                    );
                }
            } else {
                moosicbox_assert::assert!(element_width >= 0.0);
                element.calculated_width.replace(element_width);
                resized = true;
                log::trace!(
                    "evenly_distribute_children_width: resized because child calculated_width was None"
                );
            }

            if element.resize_children(arena, font_metrics, root_size) {
                resized = true;
                log::trace!("evenly_distribute_children_width: resized because child was resized");
            }
        }

        log::trace!(
            "evenly_distribute_children_width: {} updated unsized children width to {evenly_split_remaining_size}",
            self.direction,
        );

        resized
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn evenly_distribute_children_height(
        &mut self,
        arena: &Bump,
        font_metrics: &dyn FontMetrics,
        height: f32,
        root_size: (f32, f32),
    ) -> bool {
        let mut resized = false;

        let overflow_x = self.overflow_x;
        let overflow_y = self.overflow_y;
        let direction = self.direction;

        let mut contained_sized_height = 0.0;
        let mut unsized_row_count = 0;

        let rows = &mut bumpalo::collections::Vec::with_capacity_in(self.rows() as usize, arena);

        // calculate row heights
        for (row, elements) in
            &self
                .relative_positioned_elements()
                .enumerate()
                .chunk_by(|(index, x)| {
                    if overflow_x != LayoutOverflow::Wrap && overflow_y != LayoutOverflow::Wrap {
                        match direction {
                            LayoutDirection::Row => None,
                            LayoutDirection::Column => Some(u32::try_from(*index).unwrap()),
                        }
                    } else {
                        x.calculated_position.as_ref().and_then(LayoutPosition::row)
                    }
                })
        {
            if let Some(height) = elements
                .filter_map(|(_, x)| x.contained_sized_height(root_size, true))
                .max_by(order_float)
            {
                log::trace!(
                    "evenly_distribute_children_height: row={row:?} contained_sized_height={height}"
                );
                rows.push(Some(height));
                contained_sized_height += height;
            } else {
                log::trace!("evenly_distribute_children_height: row={row:?} unsized");
                rows.push(None);
                unsized_row_count += 1;
            }
        }

        #[allow(clippy::cast_precision_loss)]
        let evenly_split_remaining_size =
            if unsized_row_count > 0 && height > contained_sized_height {
                (height - contained_sized_height) / (unsized_row_count as f32)
            } else {
                0.0
            };

        let overflow_y = self.overflow_y;
        let direction = self.direction;

        log::trace!(
            "evenly_distribute_children_height: height={height} contained_sized_height={contained_sized_height} evenly_split_remaining_size={evenly_split_remaining_size}"
        );

        let mut contained_sized_height = 0.0;
        let mut unsized_row_count = 0;

        let rows = &mut bumpalo::collections::Vec::with_capacity_in(self.rows() as usize, arena);

        for (row, elements) in
            &self
                .relative_positioned_elements()
                .enumerate()
                .chunk_by(|(index, x)| {
                    if overflow_x != LayoutOverflow::Wrap && overflow_y != LayoutOverflow::Wrap {
                        match direction {
                            LayoutDirection::Row => None,
                            LayoutDirection::Column => Some(u32::try_from(*index).unwrap()),
                        }
                    } else {
                        x.calculated_position.as_ref().and_then(LayoutPosition::row)
                    }
                })
        {
            if let Some(height) = elements
                .filter_map(|(_, x)| x.contained_sized_height(root_size, true))
                .max_by(order_float)
            {
                log::trace!("evenly_distribute_children_height: row={row:?} height={height}");
                rows.push(Some(height));
                contained_sized_height += height;
            } else {
                log::trace!("evenly_distribute_children_height: row={row:?} unsized");
                rows.push(None);
                unsized_row_count += 1;
            }
        }

        #[allow(clippy::cast_precision_loss)]
        let evenly_split_remaining_size =
            if unsized_row_count > 0 && height > contained_sized_height {
                (height - contained_sized_height) / (unsized_row_count as f32)
            } else {
                0.0
            };
        log::trace!(
            "evenly_distribute_children_height: height={height} contained_sized_height={contained_sized_height} evenly_split_remaining_size={evenly_split_remaining_size}"
        );

        for (row, elements) in &self
            .relative_positioned_elements_mut()
            .enumerate()
            .chunk_by(|(index, x)| {
                if overflow_x != LayoutOverflow::Wrap && overflow_y != LayoutOverflow::Wrap {
                    match direction {
                        LayoutDirection::Row => None,
                        LayoutDirection::Column => Some(u32::try_from(*index).unwrap()),
                    }
                } else {
                    x.calculated_position.as_ref().and_then(LayoutPosition::row)
                }
            })
        {
            if let Some(height) = row.and_then(|i| rows.get(i as usize)).copied() {
                log::trace!(
                    "evenly_distribute_children_height: row={row:?} updating elements heights"
                );
                for (i, element) in elements {
                    let height = height.unwrap_or(evenly_split_remaining_size);
                    let element_height = height - element.vertical_padding().unwrap_or(0.0);

                    log::trace!(
                        "evenly_distribute_children_height: i={i} updating element height from={:?} element_height={element_height}",
                        element.calculated_height
                    );

                    if let Some(existing) = element.calculated_height {
                        if (existing - element_height).abs() > 0.01 {
                            moosicbox_assert::assert!(element_height >= 0.0);
                            element.calculated_height.replace(element_height);
                            resized = true;
                            log::trace!(
                                "evenly_distribute_children_height: resized because child calculated_height was different ({existing} != {element_height})"
                            );
                            element.evenly_distribute_children_height(
                                arena,
                                font_metrics,
                                height,
                                root_size,
                            );
                        } else {
                            log::trace!(
                                "evenly_distribute_children_height: existing height already set to {element_height}"
                            );
                        }
                    } else {
                        moosicbox_assert::assert!(element_height >= 0.0);
                        element.calculated_height.replace(element_height);
                        resized = true;
                        log::trace!(
                            "evenly_distribute_children_height: resized because child calculated_height was None"
                        );
                        element.evenly_distribute_children_height(
                            arena,
                            font_metrics,
                            height,
                            root_size,
                        );
                    }

                    if element.resize_children(arena, font_metrics, root_size) {
                        resized = true;
                        log::trace!(
                            "evenly_distribute_children_height: resized because child was resized"
                        );
                    }
                }
            }
        }

        log::trace!(
            "evenly_distribute_children_height: {} updated unsized children height to {evenly_split_remaining_size}",
            self.direction,
        );

        resized
    }
}
