use std::sync::atomic::AtomicU16;

use bumpalo::Bump;
use gigachad_transformer_models::{
    JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition,
};
use itertools::Itertools;

use crate::{
    absolute_positioned_elements_mut, calc_number, relative_positioned_elements,
    relative_positioned_elements_mut, Container, Element, Number, Position, TableIter,
    TableIterMut,
};

static SCROLLBAR_SIZE: AtomicU16 = AtomicU16::new(16);

static EPSILON: f32 = 0.001;

pub fn get_scrollbar_size() -> u16 {
    SCROLLBAR_SIZE.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_scrollbar_size(size: u16) {
    SCROLLBAR_SIZE.store(size, std::sync::atomic::Ordering::SeqCst);
}

pub trait Calc {
    fn calc(&mut self) -> bool;
}

impl Container {
    fn calc_inner(&mut self, arena: &Bump, relative_size: Option<(f32, f32)>) -> bool {
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
            element.calc_styling(container_width, container_height);
        }

        if self.element == Element::Table {
            self.calc_table(arena, relative_size)
        } else {
            self.calc_inner_container(arena, relative_size)
        }
    }

    fn calc_styling(&mut self, container_width: f32, container_height: f32) {
        self.calc_margin(container_width, container_height);
        self.calc_padding(container_width, container_height);
        self.calc_borders(container_width, container_height);
        self.calc_opacity();
        self.calc_hardsized_elements();
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn calc_table(&mut self, arena: &Bump, relative_size: Option<(f32, f32)>) -> bool {
        fn size_cells<'a>(
            iter: impl Iterator<Item = &'a mut Container>,
            col_sizes: &mut Vec<(Option<f32>, Option<f32>)>,
            cols: &mut Vec<&'a mut Container>,
        ) -> f32 {
            let mut col_count = 0;

            let sized_cols = iter.enumerate().map(|(i, x)| {
                col_count += 1;

                let width = x.width.as_ref().and(x.calculated_width);
                let height = x.height.as_ref().and(x.calculated_height);
                let contained_width = x.contained_sized_width(true);
                let contained_height = x.contained_sized_height(true);

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
                                x.calc_sized_element_height(container_height);
                            } else if x.calculated_height.is_none() {
                                x.calculated_height = Some(x.contained_calculated_height());
                            }
                            if x.width.is_some() {
                                x.calc_sized_element_width(container_width);
                            } else if x.calculated_width.is_none() {
                                x.calculated_width = Some(evenly_split_size);
                                x.calc_unsized_element_size(
                                    arena,
                                    relative_size,
                                    evenly_split_size,
                                );
                            }
                            x
                        });
                        let height = size_cells(heading, &mut col_sizes, &mut cols);
                        heading_height.replace(heading_height.map_or(height, |x| x + height));
                        log::trace!("calc_table: increased heading_height={heading_height:?}");
                    }
                }

                for row in rows {
                    #[allow(clippy::manual_inspect)]
                    let row = row.map(|x| {
                        if x.height.is_some() {
                            x.calc_sized_element_height(container_height);
                        } else if x.calculated_height.is_none() {
                            x.calculated_height = Some(x.contained_calculated_height());
                        }
                        if x.width.is_some() {
                            x.calc_sized_element_width(container_width);
                        } else if x.calculated_width.is_none() {
                            x.calculated_width = Some(evenly_split_size);
                            x.calc_unsized_element_size(arena, relative_size, evenly_split_size);
                        }
                        x
                    });
                    body_height += size_cells(row, &mut col_sizes, &mut cols);
                    log::trace!("calc_table: increased body_height={body_height}");
                }
            }

            {
                let TableIterMut { rows, headings } = self.table_iter_mut();

                for row in rows {
                    for element in row {
                        element.calc_styling(container_width, container_height);
                    }
                }

                if let Some(headings) = headings {
                    for row in headings {
                        for element in row {
                            element.calc_styling(container_width, container_height);
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

            log::debug!("calc_table: col_sizes={col_sizes:?} evenly_split_remaining_size={evenly_split_remaining_size} evenly_split_increase_size={evenly_split_increase_size}");

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
            element.calc_borders(container_width, container_height);
            element.calc_opacity();
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
                        element.calc_borders(container_width, container_height);
                        element.calc_opacity();
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
                        element.calc_borders(container_width, container_height);
                        element.calc_opacity();
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
                    th.calc_inner(arena, relative_size);
                }
            }
        }

        for row in rows {
            for td in row {
                td.calc_inner(arena, relative_size);
            }
        }

        true
    }
}

impl Calc for Container {
    fn calc(&mut self) -> bool {
        log::trace!("calc");
        let arena = Bump::new();
        self.calc_inner(&arena, None)
    }
}

#[cfg_attr(feature = "profiling", profiling::all_functions)]
impl Container {
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    fn calc_inner_container(&mut self, arena: &Bump, relative_size: Option<(f32, f32)>) -> bool {
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
            self.relative_positioned_elements_mut(),
            direction,
            overflow_x,
            overflow_y,
            container_width,
            container_height,
        );

        let relative_size = self.get_relative_size().or(relative_size);

        for element in self.relative_positioned_elements_mut() {
            element.calc_inner(arena, relative_size);
        }

        if let Some((width, height)) = relative_size {
            Self::calc_child_margins_and_padding(
                self.absolute_positioned_elements_mut(),
                width,
                height,
            );

            Self::calc_element_sizes(
                arena,
                self.absolute_positioned_elements_mut(),
                direction,
                overflow_x,
                overflow_y,
                container_width,
                container_height,
            );

            for container in self.absolute_positioned_elements_mut() {
                container.calc_inner(arena, relative_size);
            }
        }

        let mut attempt = 0;
        while self.handle_overflow(&Bump::new(), relative_size) {
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

    fn calc_element_sizes<'a>(
        arena: &Bump,
        elements: impl Iterator<Item = &'a mut Self>,
        direction: LayoutDirection,
        overflow_x: LayoutOverflow,
        overflow_y: LayoutOverflow,
        container_width: f32,
        container_height: f32,
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
                    Self::size_elements(elements, direction, container_width, container_height);
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
                    &mut elements,
                    direction,
                    container_width - padding_x,
                    container_height - padding_y,
                );
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn size_elements(
        elements: &mut [&mut Self],
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
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
                remainder -=
                    container.calc_sized_element_size(direction, container_width, container_height);
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
                if direction == LayoutDirection::Row { "width"} else { "height" },
            );

            for container in unsized_elements.map(|x| &mut **x) {
                container.size_unsized_element(
                    container_width,
                    container_height,
                    direction,
                    evenly_split_remaining_size,
                );
            }
        }
    }

    fn size_unsized_element(
        &mut self,
        container_width: f32,
        container_height: f32,
        direction: LayoutDirection,
        size: f32,
    ) {
        match direction {
            LayoutDirection::Row => {
                let container_height = container_height
                    - self
                        .padding_and_margins(LayoutDirection::Column)
                        .unwrap_or(0.0);
                let height = self
                    .height
                    .as_ref()
                    .map_or(container_height, |x| calc_number(x, container_height));
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
                let width = self
                    .width
                    .as_ref()
                    .map_or(container_width, |x| calc_number(x, container_width));
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

    fn calc_margin(&mut self, container_width: f32, container_height: f32) {
        if let Some(size) = &self.margin_top {
            self.calculated_margin_top = Some(calc_number(size, container_height));
        }
        if let Some(size) = &self.margin_bottom {
            self.calculated_margin_bottom = Some(calc_number(size, container_height));
        }
        if let Some(size) = &self.margin_left {
            self.calculated_margin_left = Some(calc_number(size, container_width));
        }
        if let Some(size) = &self.margin_right {
            self.calculated_margin_right = Some(calc_number(size, container_width));
        }
    }

    fn calc_padding(&mut self, container_width: f32, container_height: f32) {
        if let Some(size) = &self.padding_top {
            self.calculated_padding_top = Some(calc_number(size, container_height));
        }
        if let Some(size) = &self.padding_bottom {
            self.calculated_padding_bottom = Some(calc_number(size, container_height));
        }
        if let Some(size) = &self.padding_left {
            self.calculated_padding_left = Some(calc_number(size, container_width));
        }
        if let Some(size) = &self.padding_right {
            self.calculated_padding_right = Some(calc_number(size, container_width));
        }
    }

    fn calc_borders(&mut self, container_width: f32, container_height: f32) {
        if let Some((color, size)) = &self.border_top {
            self.calculated_border_top = Some((*color, calc_number(size, container_height)));
        }
        if let Some((color, size)) = &self.border_bottom {
            self.calculated_border_bottom = Some((*color, calc_number(size, container_height)));
        }
        if let Some((color, size)) = &self.border_left {
            self.calculated_border_left = Some((*color, calc_number(size, container_width)));
        }
        if let Some((color, size)) = &self.border_right {
            self.calculated_border_right = Some((*color, calc_number(size, container_width)));
        }
        if let Some(radius) = &self.border_top_left_radius {
            self.calculated_border_top_left_radius = Some(calc_number(radius, container_width));
        }
        if let Some(radius) = &self.border_top_right_radius {
            self.calculated_border_top_right_radius = Some(calc_number(radius, container_width));
        }
        if let Some(radius) = &self.border_bottom_left_radius {
            self.calculated_border_bottom_left_radius = Some(calc_number(radius, container_width));
        }
        if let Some(radius) = &self.border_bottom_right_radius {
            self.calculated_border_bottom_right_radius = Some(calc_number(radius, container_width));
        }
    }

    fn calc_opacity(&mut self) {
        if let Some(opacity) = &self.opacity {
            self.calculated_opacity = Some(calc_number(opacity, 1.0));
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
                    Number::RealPercent(_) | Number::IntegerPercent(_) | Number::Calc(_) => {}
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
                    Number::RealPercent(_) | Number::IntegerPercent(_) | Number::Calc(_) => {}
                }
            }
        }
    }

    fn calc_sized_element_width(&mut self, container_width: f32) -> f32 {
        let width = calc_number(self.width.as_ref().unwrap(), container_width);
        moosicbox_assert::assert!(width >= 0.0);
        self.calculated_width.replace(width);
        width
    }

    fn calc_sized_element_height(&mut self, container_height: f32) -> f32 {
        let height = calc_number(self.height.as_ref().unwrap(), container_height);
        self.calculated_height.replace(height);
        height
    }

    fn calc_sized_element_size(
        &mut self,
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
    ) -> f32 {
        match direction {
            LayoutDirection::Row => {
                let container_height = container_height
                    - self
                        .padding_and_margins(LayoutDirection::Column)
                        .unwrap_or(0.0);
                let width = calc_number(self.width.as_ref().unwrap(), container_width);
                let height = self
                    .height
                    .as_ref()
                    .map_or(container_height, |x| calc_number(x, container_height));
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
                let width = self
                    .width
                    .as_ref()
                    .map_or(container_width, |x| calc_number(x, container_width));
                let height = calc_number(self.height.as_ref().unwrap(), container_height);
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
    ) {
        for element in elements {
            element.calc_margin(container_width, container_height);
            element.calc_padding(container_width, container_height);
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

            log::trace!("calc_element_sizes_by_rowcol: current_rowcol_index={current_rowcol_index} rowcol_index={rowcol_index}");
            if current_rowcol_index == rowcol_index {
                if let Some(fluff) = element.padding_and_margins(LayoutDirection::Row) {
                    if direction == LayoutDirection::Row {
                        padding_and_margins_x += fluff;
                    } else if fluff > padding_and_margins_x {
                        padding_and_margins_x = fluff;
                    }
                    log::trace!("calc_element_sizes_by_rowcol: increased padding_and_margins_x={padding_and_margins_x}");
                }
                if let Some(fluff) = element.padding_and_margins(LayoutDirection::Column) {
                    if direction == LayoutDirection::Column {
                        padding_and_margins_y += fluff;
                    } else if fluff > padding_and_margins_y {
                        padding_and_margins_y = fluff;
                    }
                    log::trace!("calc_element_sizes_by_rowcol: increased padding_and_margins_y={padding_and_margins_y}");
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
                log::trace!("calc_element_sizes_by_rowcol: increased padding_and_margins_x={padding_and_margins_x}");
            }
            if let Some(fluff) = element.padding_and_margins(LayoutDirection::Column) {
                padding_and_margins_y += fluff;
                log::trace!("calc_element_sizes_by_rowcol: increased padding_and_margins_y={padding_and_margins_y}");
            }

            log::trace!("calc_element_sizes_by_rowcol: next rowcol_index={rowcol_index} padding_and_margins_x={padding_and_margins_x} padding_and_margins_y={padding_and_margins_y}");

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
        relative_size: Option<(f32, f32)>,
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
            relative_size,
            relative_positioned_elements_mut(&mut self.children),
            self.direction,
            container_width,
            container_height,
            remainder,
        );
    }

    #[allow(clippy::cognitive_complexity)]
    fn calc_unsized_element_sizes<'a>(
        arena: &Bump,
        relative_size: Option<(f32, f32)>,
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
                if direction == LayoutDirection::Row { "width"}  else { "height" },
            ),
            (
                "calc_unsized_element_sizes: setting {} to evenly_split_remaining_size={evenly_split_remaining_size}{}",
                if direction == LayoutDirection::Row { "width"}  else { "height" },
                if elements.is_empty(){
                    String::new()
                } else {
                    format!("\n{}", elements.iter().map(|x| format!("{x}")).collect_vec().join("\n"))
                }
            )
        );

        for element in &mut *elements {
            match direction {
                LayoutDirection::Row => {
                    let height = element
                        .height
                        .as_ref()
                        .map_or(container_height, |x| calc_number(x, container_height));
                    moosicbox_assert::assert!(height >= 0.0);
                    element.calculated_height.replace(height);

                    let width = evenly_split_remaining_size;
                    moosicbox_assert::assert!(width >= 0.0);
                    element.calculated_width.replace(width);
                }
                LayoutDirection::Column => {
                    let width = element
                        .width
                        .as_ref()
                        .map_or(container_width, |x| calc_number(x, container_width));
                    moosicbox_assert::assert!(width >= 0.0);
                    element.calculated_width.replace(width);

                    let height = evenly_split_remaining_size;
                    moosicbox_assert::assert!(height >= 0.0);
                    element.calculated_height.replace(height);
                }
            }
        }

        for element in elements {
            element.calc_inner(arena, relative_size);
        }
    }

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)]
    pub fn handle_overflow(&mut self, arena: &Bump, relative_size: Option<(f32, f32)>) -> bool {
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

        let gap_x = self.gap.as_ref().map(|x| calc_number(x, container_width));
        let gap_y = self.gap.as_ref().map(|x| calc_number(x, container_height));

        let relative_size = self.get_relative_size().or(relative_size);

        for container in self.relative_positioned_elements_mut().inspect(|element| {
            log::trace!("handle_overflow: processing child element\n{element}");
        }) {
            // TODO:
            // need to handle non container elements that have a width/height that is the split
            // remainder of the container width/height
            container.handle_overflow(arena, relative_size);
            let width = container.calculated_width_minus_borders().unwrap_or(0.0);
            let height = container.calculated_height_minus_borders().unwrap_or(0.0);

            let mut current_row = row;
            let mut current_col = col;

            match overflow {
                LayoutOverflow::Auto
                | LayoutOverflow::Scroll
                | LayoutOverflow::Expand
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
                            log::trace!("handle_overflow: layout_shifted because current_row != old_row || current_col != old_col ({current_row} != {old_row} || {current_col} != {old_col})");
                            layout_shifted = true;
                            true
                        } else {
                            false
                        }
                    } else {
                        true
                    };

                    if updated {
                        log::trace!("handle_overflow: setting element row/col ({current_row}, {current_col})");
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

        if self.resize_children(arena) {
            log::trace!("handle_overflow: layout_shifted because children were resized");
            layout_shifted = true;
        }

        self.position_children(relative_size);

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
    fn position_children(&mut self, relative_size: Option<(f32, f32)>) {
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
        match self.justify_content {
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

        if let Some(gap) = &self.gap {
            let gap_x = calc_number(gap, container_width);
            let gap_y = calc_number(gap, container_height);

            if let Some(margin) = horizontal_margin {
                if gap_x > margin {
                    horizontal_margin.replace(gap_x);

                    if self.justify_content == JustifyContent::SpaceEvenly {
                        #[allow(clippy::cast_precision_loss)]
                        first_horizontal_margin
                            .replace(gap_x.mul_add(-((columns - 1) as f32), remainder_width) / 2.0);
                    }
                }
            } else {
                horizontal_margin = Some(gap_x);
            }
            if let Some(margin) = vertical_margin {
                if gap_y > margin {
                    vertical_margin.replace(gap_y);

                    if self.justify_content == JustifyContent::SpaceEvenly {
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
                moosicbox_assert::die_or_warn!("position_children: missing width, height, and/or position. continuing on to next element");
                continue;
            };

            log::trace!(
                "position_children: x={x} y={y} width={width} height={height} position={position:?} child=\n{element}"
            );

            if let LayoutPosition::Wrap { row, col } = position {
                if self.justify_content == JustifyContent::SpaceEvenly || *col > 0 {
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
                if self.justify_content == JustifyContent::SpaceEvenly || *row > 0 {
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
                                x = if self.justify_content == JustifyContent::SpaceEvenly {
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
                                y = if self.justify_content == JustifyContent::SpaceEvenly {
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
                    element.calculated_x = Some(calc_number(left, width));
                }
                if let Some(right) = &element.right {
                    let offset = calc_number(right, width);
                    let bounding_width = element.bounding_calculated_width().unwrap();
                    element.calculated_x = Some(width - offset - bounding_width);
                    log::trace!("position_children: absolute position right={right} calculated_x={} width={width} offset={offset} bounding_width={bounding_width}", element.calculated_x.unwrap());
                }
                if let Some(top) = &element.top {
                    element.calculated_y = Some(calc_number(top, height));
                }
                if let Some(bottom) = &element.bottom {
                    let offset = calc_number(bottom, height);
                    let bounding_height = element.bounding_calculated_height().unwrap();
                    element.calculated_y = Some(height - offset - bounding_height);
                    log::trace!("position_children: absolute position bottom={bottom} calculated_y={} height={height} offset={offset} bounding_height={bounding_height}", element.calculated_y.unwrap());
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
    }

    pub fn contained_sized_width(&self, recurse: bool) -> Option<f32> {
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
                                .map(|x| calc_number(x, calculated_width))
                                .or_else(|| {
                                    if recurse {
                                        x.contained_sized_width(recurse)
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
                                    .map(|x| calc_number(x, calculated_width))
                                    .or_else(|| {
                                        if recurse {
                                            x.contained_sized_width(recurse)
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

    pub fn contained_sized_height(&self, recurse: bool) -> Option<f32> {
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
                                    .map(|x| calc_number(x, calculated_height))
                                    .or_else(|| {
                                        if recurse {
                                            x.contained_sized_height(recurse)
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
                                .map(|x| calc_number(x, calculated_height))
                                .or_else(|| {
                                    if recurse {
                                        x.contained_sized_height(recurse)
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
                if x < 0.0 {
                    0.0
                } else {
                    x
                }
            })
        })
    }

    #[must_use]
    pub fn calculated_height_minus_borders(&self) -> Option<f32> {
        self.calculated_height.map(|x| {
            self.vertical_borders().map_or(x, |borders| {
                let x = x - borders;
                if x < 0.0 {
                    0.0
                } else {
                    x
                }
            })
        })
    }

    #[must_use]
    pub fn calculated_width_plus_margin(&self) -> Option<f32> {
        self.calculated_width.map(|x| {
            self.internal_horizontal_margin().map_or(x, |margin| {
                let x = x + margin;
                if x < 0.0 {
                    0.0
                } else {
                    x
                }
            })
        })
    }

    #[must_use]
    pub fn calculated_height_plus_margin(&self) -> Option<f32> {
        self.calculated_height.map(|x| {
            self.internal_vertical_margin().map_or(x, |margin| {
                let x = x + margin;
                if x < 0.0 {
                    0.0
                } else {
                    x
                }
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
    fn resize_children(&mut self, arena: &Bump) -> bool {
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
            self.evenly_distribute_children_width(arena, width);

            return true;
        }

        if self.check_scrollbar_y_changed(width, &mut height, contained_calculated_width) {
            self.evenly_distribute_children_height(arena, height);

            return true;
        }

        let mut resized = false;

        if width < contained_calculated_width - EPSILON {
            log::trace!("resize_children: width < contained_calculated_width (width={width} contained_calculated_width={contained_calculated_width})");
            match self.overflow_x {
                LayoutOverflow::Auto | LayoutOverflow::Scroll => {}
                LayoutOverflow::Expand => {
                    if self.width.is_none()
                        && (self.calculated_width.unwrap() - contained_calculated_width).abs()
                            > EPSILON
                    {
                        log::trace!("resize_children: resized because contained_calculated_width changed from {} to {contained_calculated_width}", self.calculated_width.unwrap());
                        moosicbox_assert::assert!(contained_calculated_width >= 0.0);
                        self.calculated_width.replace(contained_calculated_width);
                        resized = true;
                    }
                }
                LayoutOverflow::Wrap | LayoutOverflow::Squash => {
                    resized = self.evenly_distribute_children_width(arena, width) || resized;
                }
            }
        } else {
            log::trace!("resize_children: width={width} currently contains all of the contained_calculated_width={contained_calculated_width}");
        }

        if height < contained_calculated_height - EPSILON {
            log::trace!("resize_children: height < contained_calculated_height (height={height} contained_calculated_height={contained_calculated_height})");
            match self.overflow_y {
                LayoutOverflow::Auto | LayoutOverflow::Scroll => {}
                LayoutOverflow::Expand => {
                    if self.height.is_none()
                        && (self.calculated_height.unwrap() - contained_calculated_height).abs()
                            > EPSILON
                    {
                        log::trace!("resize_children: resized because contained_calculated_height changed from {} to {contained_calculated_height}", self.calculated_height.unwrap());
                        moosicbox_assert::assert!(contained_calculated_height >= 0.0);
                        self.calculated_height.replace(contained_calculated_height);
                        resized = true;
                    }
                }
                LayoutOverflow::Wrap | LayoutOverflow::Squash => {
                    resized = self.evenly_distribute_children_height(arena, height) || resized;
                }
            }
        } else {
            log::trace!("resize_children: height={height} currently contains all of the contained_calculated_height={contained_calculated_height}");
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

            log::trace!("resize_children: original_height={height} -> new_height={new_height} original_width={width} -> new_width={new_width}");
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
                log::trace!("resize_children: resized because vertical scrollbar is now visible and affected children elements, setting scrollbar_right to {scrollbar_size} new_width={new_width}");
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
                log::trace!("resize_children: resized because horizontal scrollbar is now visible and affected children elements, setting scrollbar_bottom to {scrollbar_size} new_height={new_height}");
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

    fn evenly_distribute_children_width(&mut self, arena: &Bump, width: f32) -> bool {
        let mut resized = false;
        let contained_sized_width = self.contained_sized_width(false).unwrap_or(0.0);
        #[allow(clippy::cast_precision_loss)]
        let evenly_split_remaining_size = if width > contained_sized_width {
            (width - contained_sized_width) / (self.columns() as f32)
        } else {
            0.0
        };
        log::trace!("evenly_distribute_children_width: width={width} contained_sized_width={contained_sized_width} evenly_split_remaining_size={evenly_split_remaining_size}");

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
                    log::trace!("evenly_distribute_children_width: resized because child calculated_width was different ({existing} != {element_width})");
                }
            } else {
                moosicbox_assert::assert!(element_width >= 0.0);
                element.calculated_width.replace(element_width);
                resized = true;
                log::trace!("evenly_distribute_children_width: resized because child calculated_width was None");
            }

            if element.resize_children(arena) {
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
    fn evenly_distribute_children_height(&mut self, arena: &Bump, height: f32) -> bool {
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
                .filter_map(|(_, x)| x.contained_sized_height(true))
                .max_by(order_float)
            {
                log::trace!("evenly_distribute_children_height: row={row:?} contained_sized_height={height}");
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

        log::trace!("evenly_distribute_children_height: height={height} contained_sized_height={contained_sized_height} evenly_split_remaining_size={evenly_split_remaining_size}");

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
                .filter_map(|(_, x)| x.contained_sized_height(true))
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
        log::trace!("evenly_distribute_children_height: height={height} contained_sized_height={contained_sized_height} evenly_split_remaining_size={evenly_split_remaining_size}");

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

                    log::trace!("evenly_distribute_children_height: i={i} updating element height from={:?} element_height={element_height}", element.calculated_height);

                    if let Some(existing) = element.calculated_height {
                        if (existing - element_height).abs() > 0.01 {
                            moosicbox_assert::assert!(element_height >= 0.0);
                            element.calculated_height.replace(element_height);
                            resized = true;
                            log::trace!("evenly_distribute_children_height: resized because child calculated_height was different ({existing} != {element_height})");
                            element.evenly_distribute_children_height(arena, height);
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
                        element.evenly_distribute_children_height(arena, height);
                    }

                    if element.resize_children(arena) {
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

#[allow(clippy::trivially_copy_pass_by_ref)]
#[inline]
fn order_float(a: &f32, b: &f32) -> std::cmp::Ordering {
    if a > b {
        std::cmp::Ordering::Greater
    } else if a < b {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Equal
    }
}

fn increase_opt(opt: &mut Option<f32>, value: f32) -> f32 {
    if let Some(existing) = *opt {
        opt.replace(existing + value);
        existing + value
    } else {
        opt.replace(value);
        value
    }
}

#[cfg(test)]
mod test {
    use bumpalo::Bump;
    use maud::html;
    use pretty_assertions::{assert_eq, assert_ne};

    use crate::{
        calc::{get_scrollbar_size, Calc as _, EPSILON},
        models::{JustifyContent, LayoutDirection, LayoutOverflow, LayoutPosition},
        Calculation, Container, Element, HeaderSize, Number, Position,
    };

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

    #[macro_export]
    macro_rules! pre_escaped {
        ($($message:tt)+) => {
            maud::PreEscaped(format!($($message)*))
        };
    }

    #[test_log::test]
    fn calc_can_calc_single_element_size() {
        let mut container = Container {
            children: vec![Container::default()],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    calculated_width: Some(100.0),
                    calculated_height: Some(50.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Default),
                    ..Default::default()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_two_elements_with_size_split_evenly_row() {
        let mut container = Container {
            children: vec![Container {
                children: vec![Container::default(), Container::default()],
                direction: LayoutDirection::Row,
                ..Default::default()
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![Container {
                    children: vec![
                        Container {
                            calculated_width: Some(50.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..Default::default()
                        },
                        Container {
                            calculated_width: Some(50.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(50.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..Default::default()
                        },
                    ],
                    calculated_width: Some(100.0),
                    calculated_height: Some(40.0),
                    calculated_x: Some(0.0),
                    calculated_y: Some(0.0),
                    calculated_position: Some(LayoutPosition::Default),
                    direction: LayoutDirection::Row,
                    ..Default::default()
                }],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_above_a_vertial_split() {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![Container::default(), Container::default()],
                    direction: LayoutDirection::Row,
                    ..Default::default()
                },
                Container {
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
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
                                ..Default::default()
                            },
                            Container {
                                calculated_width: Some(50.0),
                                calculated_height: Some(20.0),
                                calculated_x: Some(50.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..Default::default()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..Default::default()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_calcs_contained_height_correctly() {
        let mut container = Container {
            children: vec![
                Container::default(),
                Container {
                    children: vec![Container::default(), Container::default()],
                    direction: LayoutDirection::Row,
                    ..Default::default()
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Squash,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..Default::default()
                    },
                    Container {
                        children: vec![
                            Container {
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(0.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..Default::default()
                            },
                            Container {
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_x: Some(25.0),
                                calculated_y: Some(0.0),
                                calculated_position: Some(LayoutPosition::Default),
                                ..Default::default()
                            },
                        ],
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(50.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let width = container.contained_sized_width(true);
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let width = container.contained_sized_width(true);

        assert_eq!(width, None);
    }

    #[test_log::test]
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let height = container.contained_sized_height(true);
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let height = container.contained_sized_height(true);

        assert_eq!(height, None);
    }

    #[test_log::test]
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
    fn contained_calculated_width_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}
        let width = container.contained_calculated_width();
        let expected = 50.0 - f32::from(get_scrollbar_size());

        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}
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
    fn contained_calculated_width_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow_and_hardsized_elements(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}
        let width = container.contained_calculated_width();
        let expected = 25.0;

        assert_eq!(
            (width - expected).abs() < EPSILON,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn handle_overflow_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow_and_hardsized_elements(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            justify_content: JustifyContent::SpaceBetween,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            justify_content: JustifyContent::SpaceBetween,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
    fn handle_overflow_y_squash_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            justify_content: JustifyContent::SpaceBetween,
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

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
    fn handle_overflow_y_expand_handles_justify_content_space_between_and_wraps_elements_properly_and_can_recalc_with_new_rows(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceBetween,
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceBetween,
            gap: Some(Number::Integer(10)),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceBetween,
            gap: Some(Number::Integer(10)),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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

        while actual.handle_overflow(&Bump::new(), None) {}

        compare_containers(&actual, &expected);
    }

    #[test_log::test]
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
            overflow_x: LayoutOverflow::Wrap,
            justify_content: JustifyContent::SpaceEvenly,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_with_padding_and_wraps_elements_properly(
    ) {
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
            calculated_padding_left: Some(20.0),
            calculated_padding_right: Some(20.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            justify_content: JustifyContent::SpaceEvenly,
            ..Default::default()
        };
        container.calc();
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
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_with_padding_and_wraps_elements_properly(
    ) {
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
            calculated_padding_left: Some(20.0),
            calculated_padding_right: Some(20.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceEvenly,
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(40.0),
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
            overflow_x: LayoutOverflow::Wrap,
            justify_content: JustifyContent::SpaceEvenly,
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(0.0),
                        ..container.children[0].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        ..container.children[1].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(40.0 + 3.75 + 3.75 + 3.75),
                        calculated_y: Some(0.0),
                        ..container.children[2].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(3.75),
                        calculated_y: Some(20.0),
                        ..container.children[3].clone()
                    },
                    Container {
                        calculated_width: Some(20.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(20.0 + 3.75 + 3.75),
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
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            justify_content: JustifyContent::SpaceEvenly,
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

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
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_and_wraps_elements_properly_and_can_recalc_with_new_rows(
    ) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceEvenly,
            ..Default::default()
        };

        log::debug!("First handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

        container.children.extend(vec![
            div.clone(),
            div.clone(),
            div.clone(),
            div.clone(),
            div,
        ]);

        log::debug!("Second handle_overflow");
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceEvenly,
            gap: Some(Number::Integer(10)),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            justify_content: JustifyContent::SpaceEvenly,
            gap: Some(Number::Integer(10)),
            ..Default::default()
        };
        while container.handle_overflow(&Bump::new(), None) {}

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

        while actual.handle_overflow(&Bump::new(), None) {}

        compare_containers(&actual, &expected);
    }

    #[test_log::test]
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
                overflow_x: LayoutOverflow::Wrap,
                overflow_y: LayoutOverflow::Expand,
                ..Default::default()
            }],

            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Expand,
            ..Default::default()
        };
        let resized = container.resize_children(&Bump::new());

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let resized = container.resize_children(&Bump::new());

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let resized = container.resize_children(&Bump::new());

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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let mut shifted = false;
        while container.handle_overflow(&Bump::new(), None) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        let mut shifted = false;
        while container.handle_overflow(&Bump::new(), None) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let mut shifted = false;
        while container.handle_overflow(&Bump::new(), None) {
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        let remainder = 50.0f32 / 3_f32; // 16.66666

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(remainder),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(remainder),
                        calculated_height: Some(40.0),
                        calculated_x: Some(remainder),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[2].children[0].clone()
                        }],
                        calculated_width: Some(remainder),
                        calculated_height: Some(40.0),
                        calculated_x: Some(remainder * 2.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                        ..container.children[2].clone()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    #[ignore]
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        let remainder = 50.0f32 / 3_f32; // 16.66666

        compare_containers(
            &container.clone(),
            &Container {
                children: vec![
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[0].children[0].clone()
                        }],
                        calculated_width: Some(remainder),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..container.children[0].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[1].children[0].clone()
                        }],
                        calculated_width: Some(remainder),
                        calculated_height: Some(40.0),
                        calculated_x: Some(remainder),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..container.children[1].clone()
                    },
                    Container {
                        children: vec![Container {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::default()),
                            ..container.children[2].children[0].clone()
                        }],
                        calculated_width: Some(remainder),
                        calculated_height: Some(40.0),
                        calculated_x: Some(remainder * 2.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        container.calc();
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
    #[ignore]
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
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Squash,
            ..Default::default()
        };
        container.calc();
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
                Default::default(),
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            ..Default::default()
        };
        container.calc();
        log::trace!("container:\n{}", container);

        compare_containers(
            &container.clone(),
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
                                ..Default::default()
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
                                        ..Default::default()
                                    },
                                    Container {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(25.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
                                ],
                                ..Default::default()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(20.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..Default::default()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_with_row_content_in_right_pane_above_a_vertial_split_with_a_specified_height(
    ) {
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
        container.calc();
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
                                ..Default::default()
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
                                        ..Default::default()
                                    },
                                    Container {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(70.0),
                                        calculated_x: Some(25.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
                                ],
                                ..Default::default()
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(70.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                    Container {
                        height: Some(Number::Integer(10)),
                        calculated_width: Some(100.0),
                        calculated_height: Some(10.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(70.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..Default::default()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
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
        container.calc();
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
        container.calc();
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
                                children: vec![Container {
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
        container.calc();
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
        container.calc();
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

        container.calc();
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
        container.calc();
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
        container.calc();
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
                        ..Default::default()
                    },
                    Container {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        position: Some(Position::Absolute),
                        ..Default::default()
                    },
                ],
                ..container
            },
        );
    }

    #[test_log::test]
    fn calc_can_calc_absolute_positioned_element_nested_on_top_of_a_relative_element_with_left_offset(
    ) {
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
        container.calc();
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
                            ..Default::default()
                        },
                        Container {
                            left: Some(Number::Integer(30)),
                            calculated_width: Some(100.0),
                            calculated_height: Some(50.0),
                            calculated_x: Some(30.0),
                            calculated_y: Some(0.0),
                            position: Some(Position::Absolute),
                            ..Default::default()
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
        container.calc();
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
                        ..Default::default()
                    },
                    Container {
                        left: Some(Number::Integer(30)),
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(30.0),
                        calculated_y: Some(0.0),
                        position: Some(Position::Absolute),
                        ..Default::default()
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
        container.calc();
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
                        ..Default::default()
                    },
                    Container {
                        left: Some(Number::Integer(30)),
                        width: Some(Number::Integer(30)),
                        height: Some(Number::Integer(20)),
                        calculated_width: Some(30.0),
                        calculated_height: Some(20.0),
                        calculated_x: Some(30.0),
                        calculated_y: Some(0.0),
                        position: Some(Position::Absolute),
                        ..Default::default()
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
            justify_content: JustifyContent::Center,
            ..Default::default()
        };
        container.calc();
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
            justify_content: JustifyContent::Start,
            ..Default::default()
        };
        container.calc();
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
        container.calc();
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
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            ..Default::default()
        };
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            ..Default::default()
        };
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            ..Default::default()
        };
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            ..Default::default()
        };
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            ..Default::default()
        };
        container.calc();
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
        container.calc();
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
            overflow_x: LayoutOverflow::Wrap,
            ..Default::default()
        };
        container.calc();
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
        container.calc();
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
        container.calc();
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
                ..container
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
        container.calc();
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
            ..Default::default()
        };
        container.calc();
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
            ..Default::default()
        };
        container.calc();
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
                ..Default::default()
            }],

            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
            ..Default::default()
        };
        container.calc();
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
                ..Default::default()
            }],

            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            height: Some(Number::Integer(50)),
            ..Default::default()
        };
        container.calc();
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
            ..Default::default()
        };
        container.calc();
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
        container.calc();
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
    fn calc_calculates_width_minus_the_horizontal_padding_for_nested_children_with_calc_parent_sizes(
    ) {
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

        container.calc();
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

        container.calc();
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

        container.calc();
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
    fn calc_calculates_horizontal_and_vertical_position_from_right_for_absolute_position_with_padding(
    ) {
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

        container.calc();
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
    fn calc_calculates_horizontal_and_vertical_position_from_right_for_nested_absolute_position_with_padding(
    ) {
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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

        container.calc();
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
    fn calc_overflow_y_expand_expands_height_when_contained_height_is_greater_than_single_unsized_div(
    ) {
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

        container.calc();
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
    fn calc_overflow_y_expand_expands_height_when_contained_height_is_greater_than_two_unsized_divs(
    ) {
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

        container.calc();
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

        container.calc();
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
