use std::sync::atomic::AtomicU16;

use itertools::Itertools;

use crate::{
    calc_number, ContainerElement, Element, LayoutDirection, LayoutOverflow, LayoutPosition,
    Number, TableIter, TableIterMut,
};

static SCROLLBAR_SIZE: AtomicU16 = AtomicU16::new(16);

pub fn get_scrollbar_size() -> u16 {
    SCROLLBAR_SIZE.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn set_scrollbar_size(size: u16) {
    SCROLLBAR_SIZE.store(size, std::sync::atomic::Ordering::SeqCst);
}

pub trait Calc {
    fn calc(&mut self);
}

impl Calc for Element {
    fn calc(&mut self) {
        if let Self::Table { .. } = self {
            self.calc_table();
        } else if let Some(container) = self.container_element_mut() {
            container.calc();
        }
    }
}

impl Element {
    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    fn calc_table(&mut self) {
        fn size_cells<'a>(
            iter: impl Iterator<Item = &'a mut ContainerElement>,
            col_sizes: &mut Vec<Option<f32>>,
            cols: &mut Vec<&'a mut ContainerElement>,
        ) -> f32 {
            let mut col_count = 0;

            let sized_cols = iter.enumerate().map(|(i, x)| {
                col_count += 1;

                let width = x.contained_sized_width(true);
                let height = x.contained_sized_height(true);

                if i >= cols.len() {
                    cols.push(x);
                } else {
                    cols[i] = x;
                }

                (width, height)
            });

            let mut max_height = None;

            for (i, (width, height)) in sized_cols.enumerate() {
                if let Some(width) = width {
                    while i >= col_sizes.len() {
                        col_sizes.push(None);
                    }

                    if let Some(col) = col_sizes[i] {
                        if width > col {
                            col_sizes[i].replace(width);
                        }
                    } else {
                        col_sizes[i] = Some(width);
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
                }
            }

            let row_height = max_height.unwrap_or(25.0);

            for container in cols {
                container.calculated_height.replace(row_height);
            }

            row_height
        }

        moosicbox_logging::debug_or_trace!(("calc_table"), ("calc_table: {self:?}"));

        let (container_width, container_height) = {
            let Self::Table { element: container } = self else {
                panic!("Not a table");
            };

            let (Some(container_width), Some(container_height)) = (
                container.calculated_width_minus_padding(),
                container.calculated_height_minus_padding(),
            ) else {
                moosicbox_assert::die_or_panic!(
                    "calc_table requires calculated_width and calculated_height to be set"
                );
            };

            container.calc_hardsized_elements();

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
            let mut col_sizes = vec![None; col_count];
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
                                x.calculated_height = Some(25.0);
                            }
                            if x.width.is_some() {
                                x.calc_sized_element_width(container_width);
                            } else if x.calculated_width.is_none() {
                                x.calculated_width = Some(evenly_split_size);
                                x.calc_unsized_element_size(evenly_split_size);
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
                            x.calculated_height = Some(25.0);
                        }
                        if x.width.is_some() {
                            x.calc_sized_element_width(container_width);
                        } else if x.calculated_width.is_none() {
                            x.calculated_width = Some(evenly_split_size);
                            x.calc_unsized_element_size(evenly_split_size);
                        }
                        x
                    });
                    body_height += size_cells(row, &mut col_sizes, &mut cols);
                    log::trace!("calc_table: increased body_height={body_height}");
                }
            }

            // Set unsized cells to remainder size
            let TableIterMut { rows, headings } = self.table_iter_mut();

            let unsized_col_count = col_sizes.iter().filter(|x| x.is_none()).count();
            let sized_width: f32 = col_sizes.iter().flatten().sum();

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

            log::debug!("calc_table: col_sizes={:?}", col_sizes);

            if let Some(headings) = headings {
                for heading in headings {
                    for (th, size) in heading.zip(&col_sizes) {
                        if let Some(size) = size {
                            th.calculated_width = Some(*size + evenly_split_increase_size);
                        } else {
                            th.calculated_width = Some(evenly_split_remaining_size);
                        }
                    }
                }
            }

            for row in rows {
                for (td, size) in row.zip(&col_sizes) {
                    if let Some(size) = size {
                        td.calculated_width = Some(*size + evenly_split_increase_size);
                    } else {
                        td.calculated_width = Some(evenly_split_remaining_size);
                    }
                }
            }

            (body_height, heading_height)
        };

        let Self::Table { element: container } = self else {
            panic!("Not a table");
        };

        container
            .calculated_height
            .replace(heading_height.unwrap_or(0.0) + body_height);

        {
            for element in &mut container.elements {
                match element {
                    Self::THead { element } => {
                        if element.width.is_none() {
                            element.calculated_width.replace(container_width);
                        }
                        if element.height.is_none() {
                            element
                                .calculated_height
                                .replace(heading_height.unwrap_or(0.0));
                        }

                        for element in element
                            .elements
                            .iter_mut()
                            .filter_map(|x| x.container_element_mut())
                        {
                            if element.width.is_none() {
                                element.calculated_width.replace(container_width);
                            }
                            if element.height.is_none() {
                                element.calculated_height.replace(
                                    element
                                        .elements
                                        .iter()
                                        .filter_map(|x| x.container_element())
                                        .find_map(|x| x.calculated_height)
                                        .unwrap_or(0.0),
                                );
                            }
                        }
                    }
                    Self::TBody { element } => {
                        if element.width.is_none() {
                            element.calculated_width.replace(container_width);
                        }
                        if element.height.is_none() {
                            element.calculated_height.replace(body_height);
                        }

                        for element in element
                            .elements
                            .iter_mut()
                            .filter_map(|x| x.container_element_mut())
                        {
                            if element.width.is_none() {
                                element.calculated_width.replace(container_width);
                            }
                            if element.height.is_none() {
                                element.calculated_height.replace(
                                    element
                                        .elements
                                        .iter()
                                        .filter_map(|x| x.container_element())
                                        .find_map(|x| x.calculated_height)
                                        .unwrap_or(0.0),
                                );
                            }
                        }
                    }
                    Self::TR { element } => {
                        if element.width.is_none() {
                            element.calculated_width.replace(container_width);
                        }
                        if element.height.is_none() {
                            element.calculated_height.replace(
                                element
                                    .elements
                                    .iter()
                                    .filter_map(|x| x.container_element())
                                    .find_map(|x| x.calculated_height)
                                    .unwrap_or(0.0),
                            );
                        }
                    }
                    _ => {
                        panic!("Invalid table element: {element}");
                    }
                }
            }
        }

        let TableIterMut { rows, headings } = self.table_iter_mut();

        if let Some(headings) = headings {
            for heading in headings {
                for th in heading {
                    th.calc();
                }
            }
        }

        for row in rows {
            for td in row {
                td.calc();
            }
        }
    }
}

impl Calc for ContainerElement {
    fn calc(&mut self) {
        self.calc_inner();
    }
}

impl ContainerElement {
    fn calc_inner(&mut self) {
        let (Some(container_width), Some(container_height)) = (
            self.calculated_width_minus_padding(),
            self.calculated_height_minus_padding(),
        ) else {
            moosicbox_assert::die_or_panic!(
                "calc_inner requires calculated_width and calculated_height to be set"
            );
        };

        self.calc_hardsized_elements();

        let direction = self.direction;

        let (sized_elements, unsized_elements): (Vec<_>, Vec<_>) =
            self.elements.iter_mut().partition(|x| {
                x.container_element().is_some_and(|x| match direction {
                    LayoutDirection::Row => x.width.is_some(),
                    LayoutDirection::Column => x.height.is_some(),
                })
            });

        let remainder = Self::calc_sized_element_sizes(
            sized_elements.into_iter(),
            direction,
            container_width,
            container_height,
        );

        Self::calc_unsized_element_sizes(
            unsized_elements.into_iter(),
            direction,
            container_width,
            container_height,
            remainder,
        );

        while self.handle_overflow() {}
    }

    fn calc_hardsized_elements(&mut self) {
        for element in self
            .elements
            .iter_mut()
            .filter_map(|x| x.container_element_mut())
        {
            element.calc_hardsized_elements();

            if let Some(width) = &element.width {
                match width {
                    Number::Real(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_width={x} {element:?}"
                        );
                        element.calculated_width.replace(*x);
                    }
                    Number::Integer(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_width={x} {element:?}"
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
                            "calc_hardsized_elements: setting calculated_height={x} {element:?}"
                        );
                        element.calculated_height.replace(*x);
                    }
                    Number::Integer(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_height={x} {element:?}"
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
                let width = calc_number(self.width.as_ref().unwrap(), container_width);
                let height = self
                    .height
                    .as_ref()
                    .map_or(container_height, |x| calc_number(x, container_height));
                self.calculated_width.replace(width);
                self.calculated_height.replace(height);
            }
            LayoutDirection::Column => {
                let width = self
                    .width
                    .as_ref()
                    .map_or(container_width, |x| calc_number(x, container_width));
                let height = calc_number(self.height.as_ref().unwrap(), container_height);
                self.calculated_width.replace(width);
                self.calculated_height.replace(height);
            }
        }
        match direction {
            LayoutDirection::Row => self.calculated_width.unwrap_or(0.0),
            LayoutDirection::Column => self.calculated_height.unwrap_or(0.0),
        }
    }

    fn calc_sized_element_sizes<'a>(
        elements: impl Iterator<Item = &'a mut Element>,
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
    ) -> f32 {
        log::debug!("calc_unsized_element_sizes: container_width={container_width} container_height={container_height}");

        let mut remainder = match direction {
            LayoutDirection::Row => container_width,
            LayoutDirection::Column => container_height,
        };

        for element in elements {
            if let Some(container) = element.container_element_mut() {
                remainder -=
                    container.calc_sized_element_size(direction, container_width, container_height);
            }

            element.calc();
        }

        remainder
    }

    fn calc_unsized_element_size(&mut self, remainder: f32) {
        let (Some(container_width), Some(container_height)) = (
            self.calculated_width_minus_padding(),
            self.calculated_height_minus_padding(),
        ) else {
            moosicbox_assert::die_or_panic!(
                    "calc_unsized_element_size requires calculated_width and calculated_height to be set"
                );
        };
        Self::calc_unsized_element_sizes(
            self.elements.iter_mut(),
            self.direction,
            container_width,
            container_height,
            remainder,
        );
    }

    fn calc_unsized_element_sizes<'a>(
        elements: impl Iterator<Item = &'a mut Element>,
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
        remainder: f32,
    ) {
        let elements = elements.collect_vec();
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

        for element in elements {
            if let Some(container) = element.container_element_mut() {
                match direction {
                    LayoutDirection::Row => {
                        let height = container
                            .height
                            .as_ref()
                            .map_or(container_height, |x| calc_number(x, container_height));
                        container.calculated_height.replace(height);
                        container
                            .calculated_width
                            .replace(evenly_split_remaining_size);
                    }
                    LayoutDirection::Column => {
                        let width = container
                            .width
                            .as_ref()
                            .map_or(container_width, |x| calc_number(x, container_width));
                        container.calculated_width.replace(width);
                        container
                            .calculated_height
                            .replace(evenly_split_remaining_size);
                    }
                }
            }

            element.calc();
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_overflow(&mut self) -> bool {
        log::trace!("handle_overflow: processing self\n{self:?}");
        let mut layout_shifted = false;

        let direction = self.direction;
        let overflow = self.overflow_x;
        let container_width = self.calculated_width_minus_padding().unwrap_or(0.0);
        let container_height = self.calculated_height_minus_padding().unwrap_or(0.0);

        let mut x = 0.0;
        let mut y = 0.0;
        let mut max_width = 0.0;
        let mut max_height = 0.0;
        let mut row = 0;
        let mut col = 0;

        for element in &mut self.elements {
            log::trace!("handle_overflow: processing child element\n{element}");
            // TODO:
            // need to handle non container elements that have a width/height that is the split
            // remainder of the container width/height
            if let Some(element) = element.container_element_mut() {
                element.handle_overflow();
                let width = element.calculated_width_minus_padding().unwrap_or(0.0);
                let height = element.calculated_height_minus_padding().unwrap_or(0.0);

                let mut current_row = row;
                let mut current_col = col;

                match overflow {
                    LayoutOverflow::Auto
                    | LayoutOverflow::Scroll
                    | LayoutOverflow::Show
                    | LayoutOverflow::Squash => {
                        match direction {
                            LayoutDirection::Row => {
                                x += width;
                            }
                            LayoutDirection::Column => {
                                y += height;
                            }
                        }

                        element
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
                                row += 1;
                            }
                        }

                        let updated = if let Some(LayoutPosition::Wrap {
                            row: old_row,
                            col: old_col,
                        }) = element.calculated_position
                        {
                            if current_row != old_row || current_col != old_col {
                                layout_shifted = true;
                                true
                            } else {
                                false
                            }
                        } else {
                            true
                        };

                        if updated {
                            log::debug!("handle_overflow: setting element row/col ({current_row}, {current_col})");
                            element.calculated_position.replace(LayoutPosition::Wrap {
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
        }

        layout_shifted = layout_shifted || self.resize_children();
        self.position_children();

        layout_shifted
    }

    fn position_children(&mut self) {
        log::trace!("position_children");

        let mut x = 0.0;
        let mut y = 0.0;
        let mut max_width = 0.0;
        let mut max_height = 0.0;

        for element in self
            .elements
            .iter_mut()
            .filter_map(|x| x.container_element_mut())
        {
            log::trace!("position_children: x={x} y={y} child={element:?}");

            let (Some(width), Some(height), Some(position)) = (
                element.calculated_width,
                element.calculated_height,
                element.calculated_position.as_ref(),
            ) else {
                moosicbox_assert::die_or_warn!("position_children: missing width, height, and/or position. continuing on to next element");
                continue;
            };

            element.calculated_x.replace(x);
            element.calculated_y.replace(y);

            match self.direction {
                LayoutDirection::Row => {
                    match position {
                        LayoutPosition::Wrap { col, .. } => {
                            if *col == 0 {
                                x = 0.0;
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
                                y = 0.0;
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
    }

    pub fn contained_sized_width(&self, recurse: bool) -> Option<f32> {
        let Some(calculated_width) = self.calculated_width else {
            moosicbox_assert::die_or_panic!(
                "calculated_width is required to get the contained_sized_width"
            );
        };

        match self.direction {
            LayoutDirection::Row => self
                .elements
                .iter()
                .chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { row, .. } => Some(row),
                            LayoutPosition::Default => None,
                        })
                    })
                })
                .into_iter()
                .filter_map(|(_, elements)| {
                    let mut widths = elements
                        .filter_map(|x| x.container_element())
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
                let columns = self.elements.iter().chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { col, .. } => Some(col),
                            LayoutPosition::Default => None,
                        })
                    })
                });

                let mut widths = columns
                    .into_iter()
                    .filter_map(|(_, elements)| {
                        elements
                            .filter_map(|x| x.container_element())
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
                let rows = self.elements.iter().chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { row, .. } => Some(row),
                            LayoutPosition::Default => None,
                        })
                    })
                });

                let mut heights = rows
                    .into_iter()
                    .filter_map(|(_, elements)| {
                        elements
                            .filter_map(|x| x.container_element())
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
                .elements
                .iter()
                .chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { col, .. } => Some(col),
                            LayoutPosition::Default => None,
                        })
                    })
                })
                .into_iter()
                .filter_map(|(_, elements)| {
                    let mut heights = elements
                        .filter_map(|x| x.container_element())
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

    pub fn contained_calculated_width(&self) -> f32 {
        log::trace!(
            "contained_calculated_width: direction={} element_count={} position={:?}",
            self.direction,
            self.elements.len(),
            self.elements
                .first()
                .and_then(|x| x.container_element().map(|x| x.calculated_position.clone()))
        );

        match self.direction {
            LayoutDirection::Row => self
                .elements
                .iter()
                .chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { row, .. } => Some(row),
                            LayoutPosition::Default => None,
                        })
                    })
                })
                .into_iter()
                .map(|(row, elements)| {
                    let mut len = 0;
                    let sum = elements
                        .map(|x| {
                            len += 1;
                            log::trace!("contained_calculated_width: element:\n{x}");
                            x.container_element()
                                .and_then(|x| x.calculated_width)
                                .unwrap_or(0.0)
                        })
                        .sum();

                    log::trace!(
                        "contained_calculated_width: summed row {row:?} with {len} elements: {sum}"
                    );

                    sum
                })
                .max_by(order_float)
                .unwrap_or(0.0),
            LayoutDirection::Column => self
                .elements
                .iter()
                .chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { col, .. } => Some(col),
                            LayoutPosition::Default => None,
                        })
                    })
                })
                .into_iter()
                .map(|(col, elements)| {
                    let mut len = 0;
                    let max = elements
                        .map(|x| {
                            len += 1;
                            log::trace!("contained_calculated_width: element:\n{x}");
                            x.container_element()
                                .and_then(|x| x.calculated_width)
                                .unwrap_or(0.0)
                        })
                        .max_by(order_float)
                        .unwrap_or(0.0);

                    log::trace!(
                        "contained_calculated_width: maxed col {col:?} with {len} elements: {max}"
                    );

                    max
                })
                .max_by(order_float)
                .unwrap_or(0.0),
        }
    }

    pub fn contained_calculated_height(&self) -> f32 {
        match self.direction {
            LayoutDirection::Row => self
                .elements
                .iter()
                .chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { row, .. } => Some(row),
                            LayoutPosition::Default => None,
                        })
                    })
                })
                .into_iter()
                .map(|(_, elements)| {
                    elements
                        .map(|x| {
                            x.container_element()
                                .and_then(|x| x.calculated_height)
                                .unwrap_or(0.0)
                        })
                        .max_by(order_float)
                        .unwrap_or(0.0)
                })
                .sum(),
            LayoutDirection::Column => self
                .elements
                .iter()
                .chunk_by(|x| {
                    x.container_element().and_then(|x| {
                        x.calculated_position.as_ref().and_then(|x| match x {
                            LayoutPosition::Wrap { col, .. } => Some(col),
                            LayoutPosition::Default => None,
                        })
                    })
                })
                .into_iter()
                .map(|(_, elements)| {
                    elements
                        .map(|x| {
                            x.container_element()
                                .and_then(|x| x.calculated_height)
                                .unwrap_or(0.0)
                        })
                        .max_by(order_float)
                        .unwrap_or(0.0)
                })
                .max_by(order_float)
                .unwrap_or(0.0),
        }
    }

    pub fn rows(&self) -> u32 {
        self.elements
            .iter()
            .filter_map(|x| x.container_element())
            .filter_map(|x| x.calculated_position.as_ref())
            .filter_map(LayoutPosition::row)
            .max()
            .unwrap_or(0)
            + 1
    }

    pub fn columns(&self) -> u32 {
        self.elements
            .iter()
            .filter_map(|x| x.container_element())
            .filter_map(|x| x.calculated_position.as_ref())
            .filter_map(LayoutPosition::column)
            .max()
            .unwrap_or(0)
            + 1
    }

    #[must_use]
    pub fn horizontal_padding(&self) -> Option<f32> {
        let mut padding = None;
        if let Some(padding_left) = self.padding_left {
            padding = Some(padding_left);
        }
        if let Some(padding_right) = self.padding_right {
            padding.replace(padding.map_or(padding_right, |x| x + padding_right));
        }
        padding
    }

    #[must_use]
    pub fn vertical_padding(&self) -> Option<f32> {
        let mut padding = None;
        if let Some(padding_top) = self.padding_top {
            padding = Some(padding_top);
        }
        if let Some(padding_bottom) = self.padding_bottom {
            padding.replace(padding.map_or(padding_bottom, |x| x + padding_bottom));
        }
        padding
    }

    #[must_use]
    pub fn calculated_width_minus_padding(&self) -> Option<f32> {
        self.calculated_width.map(|x| {
            self.horizontal_padding().map_or(x, |padding| {
                let x = x - padding;
                if x < 0.0 {
                    0.0
                } else {
                    x
                }
            })
        })
    }

    #[must_use]
    pub fn calculated_height_minus_padding(&self) -> Option<f32> {
        self.calculated_height.map(|x| {
            self.vertical_padding().map_or(x, |padding| {
                let x = x - padding;
                if x < 0.0 {
                    0.0
                } else {
                    x
                }
            })
        })
    }

    #[allow(clippy::too_many_lines)]
    #[allow(clippy::cognitive_complexity)]
    fn resize_children(&mut self) -> bool {
        if self.elements.is_empty() {
            log::trace!("resize_children: no children");
            return false;
        }
        let (Some(width), Some(height)) = (
            self.calculated_width_minus_padding(),
            self.calculated_height_minus_padding(),
        ) else {
            moosicbox_assert::die_or_panic!(
                "ContainerElement missing calculated_width and/or calculated_height: {self:?}"
            );
        };

        let mut resized = false;

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

        let scrollbar_size = f32::from(get_scrollbar_size());

        if self.overflow_y == LayoutOverflow::Scroll
            || contained_calculated_height > height && self.overflow_y == LayoutOverflow::Auto
        {
            log::debug!(
                "resize_children: vertical scrollbar is visible, setting padding_right to {scrollbar_size}"
            );
            if !self
                .padding_right
                .is_some_and(|x| (x - scrollbar_size).abs() < 0.001)
            {
                self.padding_right.replace(scrollbar_size);
                resized = true;
            }
        }

        if self.overflow_x == LayoutOverflow::Scroll
            || contained_calculated_width > width && self.overflow_x == LayoutOverflow::Auto
        {
            log::debug!(
                "resize_children: horizontal scrollbar is visible, setting padding_bottom to {scrollbar_size}"
            );
            if !self
                .padding_bottom
                .is_some_and(|x| (x - scrollbar_size).abs() < 0.001)
            {
                self.padding_bottom.replace(scrollbar_size);
                resized = true;
            }
        }

        if width < contained_calculated_width {
            log::debug!("resize_children: width < contained_calculated_width (width={width} contained_calculated_width={contained_calculated_width})");
            match self.overflow_x {
                LayoutOverflow::Auto | LayoutOverflow::Scroll => {}
                LayoutOverflow::Show => {
                    if self.width.is_none() {
                        self.calculated_width.replace(contained_calculated_width);
                        resized = true;
                    }
                }
                LayoutOverflow::Squash | LayoutOverflow::Wrap => {
                    let contained_sized_width = self.contained_sized_width(false).unwrap_or(0.0);
                    log::debug!("resize_children: contained_sized_width={contained_sized_width}");
                    #[allow(clippy::cast_precision_loss)]
                    let evenly_split_remaining_size = if width > contained_sized_width {
                        (width - contained_sized_width) / (self.columns() as f32)
                    } else {
                        0.0
                    };

                    for element in self
                        .elements
                        .iter_mut()
                        .filter_map(|x| x.container_element_mut())
                        .filter(|x| x.width.is_none())
                    {
                        element
                            .calculated_width
                            .replace(evenly_split_remaining_size);

                        element.resize_children();
                        resized = true;
                    }

                    log::trace!(
                        "resize_children: {} updated unsized children width to {evenly_split_remaining_size}",
                        self.direction,
                    );
                }
            }
        }
        if height < contained_calculated_height {
            log::debug!("resize_children: height < contained_calculated_height (height={height} contained_calculated_height={contained_calculated_height})");
            match self.overflow_y {
                LayoutOverflow::Auto | LayoutOverflow::Scroll => {}
                LayoutOverflow::Show => {
                    if self.height.is_none() {
                        self.calculated_height.replace(contained_calculated_height);
                        resized = true;
                    }
                }
                LayoutOverflow::Squash | LayoutOverflow::Wrap => {
                    let contained_sized_height = self.contained_sized_height(false).unwrap_or(0.0);
                    log::debug!("resize_children: contained_sized_height={contained_sized_height}");
                    #[allow(clippy::cast_precision_loss)]
                    let evenly_split_remaining_size = if height > contained_sized_height {
                        (height - contained_sized_height) / (self.rows() as f32)
                    } else {
                        0.0
                    };

                    for element in self
                        .elements
                        .iter_mut()
                        .filter_map(|x| x.container_element_mut())
                        .filter(|x| x.height.is_none())
                    {
                        element
                            .calculated_height
                            .replace(evenly_split_remaining_size);

                        element.resize_children();
                        resized = true;
                    }

                    log::trace!(
                        "resize_children: {} updated unsized children height to {evenly_split_remaining_size}",
                        self.direction,
                    );
                }
            }
        }

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

#[cfg(test)]
mod test {
    use pretty_assertions::{assert_eq, assert_ne};

    use crate::{
        calc::{get_scrollbar_size, Calc as _},
        ContainerElement, Element, LayoutDirection, LayoutOverflow, LayoutPosition, Number,
    };

    #[test_log::test]
    fn calc_can_calc_single_element_size() {
        let mut container = ContainerElement {
            elements: vec![Element::Div {
                element: ContainerElement::default(),
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(50.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(100.0),
                        calculated_height: Some(50.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        ..Default::default()
                    },
                }],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_two_elements_with_size_split_evenly_row() {
        let mut container = ContainerElement {
            elements: vec![Element::Div {
                element: ContainerElement {
                    elements: vec![
                        Element::Div {
                            element: ContainerElement::default(),
                        },
                        Element::Div {
                            element: ContainerElement::default(),
                        },
                    ],
                    direction: LayoutDirection::Row,
                    ..Default::default()
                },
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement {
                                    calculated_width: Some(50.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::Default),
                                    ..Default::default()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_width: Some(50.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(50.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::Default),
                                    ..Default::default()
                                },
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(40.0),
                        calculated_x: Some(0.0),
                        calculated_y: Some(0.0),
                        calculated_position: Some(LayoutPosition::Default),
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                }],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_above_a_vertial_split() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement::default(),
                            },
                            Element::Div {
                                element: ContainerElement::default(),
                            },
                        ],
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        elements: vec![],
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
                                },
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(50.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
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
                    },
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![],
                            calculated_width: Some(100.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(20.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..Default::default()
                        },
                    }
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_inner_calcs_contained_height_correctly() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement::default(),
                },
                Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement::default(),
                            },
                            Element::Div {
                                element: ContainerElement::default(),
                            },
                        ],
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::default(),
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        container.calc_inner();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            calculated_width: Some(50.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(40.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
                                },
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(25.0),
                                        calculated_height: Some(40.0),
                                        calculated_x: Some(25.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
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
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn contained_sized_width_calculates_wrapped_width_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let width = container.contained_sized_width(true);
        let expected = 50.0;

        assert_ne!(width, None);
        let width = width.unwrap();
        assert_eq!(
            (width - expected).abs() < 0.0001,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn contained_sized_width_calculates_wrapped_empty_width_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        height: Some(Number::Integer(25)),
                        calculated_width: Some(40.0),
                        calculated_height: Some(25.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        height: Some(Number::Integer(25)),
                        calculated_width: Some(40.0),
                        calculated_height: Some(25.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        height: Some(Number::Integer(25)),
                        calculated_width: Some(40.0),
                        calculated_height: Some(25.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(40.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let width = container.contained_sized_width(true);

        assert_eq!(width, None);
    }

    #[test_log::test]
    fn contained_sized_height_calculates_wrapped_height_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        height: Some(Number::Integer(25)),
                        calculated_width: Some(40.0),
                        calculated_height: Some(25.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        height: Some(Number::Integer(25)),
                        calculated_width: Some(40.0),
                        calculated_height: Some(25.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        height: Some(Number::Integer(25)),
                        calculated_width: Some(40.0),
                        calculated_height: Some(25.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(40.0),
            calculated_height: Some(50.0),
            direction: LayoutDirection::Column,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let height = container.contained_sized_height(true);
        let expected = 50.0;

        assert_ne!(height, None);
        let height = height.unwrap();
        assert_eq!(
            (height - expected).abs() < 0.0001,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    fn contained_sized_height_calculates_empty_height_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let height = container.contained_sized_height(true);

        assert_eq!(height, None);
    }

    #[test_log::test]
    fn contained_calculated_width_calculates_wrapped_width_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let width = container.contained_calculated_width();
        let expected = 50.0;

        assert_eq!(
            (width - expected).abs() < 0.0001,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn contained_calculated_height_calculates_wrapped_height_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < 0.0001,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    fn contained_calculated_scroll_y_width_calculates_wrapped_height_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
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
            (width - expected).abs() < 0.0001,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn contained_calculated_scroll_y_calculates_height_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
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
            (height - expected).abs() < 0.0001,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    fn contained_calculated_width_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow(
    ) {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow() {}
        let width = container.contained_calculated_width();
        let expected = 50.0 - f32::from(get_scrollbar_size());

        assert_eq!(
            (width - expected).abs() < 0.0001,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn handle_overflow_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        calculated_width: Some(50.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow() {}
        let width = 50.0 - f32::from(get_scrollbar_size());

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            calculated_width: Some(width),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            ..container.elements[0].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            calculated_width: Some(width),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(40.0),
                            ..container.elements[1].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                            calculated_width: Some(width),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(80.0),
                            ..container.elements[2].container_element().unwrap().clone()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn contained_calculated_width_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow_and_hardsized_elements(
    ) {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow() {}
        let width = container.contained_calculated_width();
        let expected = 25.0;

        assert_eq!(
            (width - expected).abs() < 0.0001,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn handle_overflow_auto_y_takes_into_account_scrollbar_size_when_there_is_scroll_overflow_and_hardsized_elements(
    ) {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow() {}

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            ..container.elements[0].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(40.0),
                            ..container.elements[1].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(80.0),
                            ..container.elements[2].container_element().unwrap().clone()
                        },
                    },
                ],
                calculated_width: Some(50.0),
                calculated_height: Some(40.0),
                ..container
            }
        );
    }

    #[test_log::test]
    fn handle_overflow_auto_y_wraps_elements_properly_by_taking_into_account_scrollbar_size() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        while container.handle_overflow() {}

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            ..container.elements[0].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(25.0),
                            calculated_y: Some(0.0),
                            ..container.elements[1].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(40.0),
                            ..container.elements[2].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(25.0),
                            calculated_y: Some(40.0),
                            ..container.elements[3].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(80.0),
                            ..container.elements[4].container_element().unwrap().clone()
                        },
                    },
                ],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_auto_y_wraps_nested_elements_properly_by_taking_into_account_scrollbar_size() {
        let mut container = ContainerElement {
            elements: vec![Element::Div {
                element: ContainerElement {
                    elements: vec![
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                                ..Default::default()
                            },
                        },
                    ],
                    calculated_width: Some(75.0),
                    calculated_height: Some(40.0),
                    direction: LayoutDirection::Row,
                    overflow_x: LayoutOverflow::Wrap,
                    overflow_y: LayoutOverflow::Show,
                    ..Default::default()
                },
            }],
            calculated_width: Some(75.0),
            calculated_height: Some(40.0),
            overflow_y: LayoutOverflow::Auto,
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement {
                                    calculated_position: Some(LayoutPosition::Wrap {
                                        row: 0,
                                        col: 0,
                                    }),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_position: Some(LayoutPosition::Wrap {
                                        row: 0,
                                        col: 1,
                                    }),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(25.0),
                                    calculated_y: Some(0.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_position: Some(LayoutPosition::Wrap {
                                        row: 1,
                                        col: 0,
                                    }),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[2]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_position: Some(LayoutPosition::Wrap {
                                        row: 1,
                                        col: 1,
                                    }),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(25.0),
                                    calculated_y: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[3]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_position: Some(LayoutPosition::Wrap {
                                        row: 2,
                                        col: 0,
                                    }),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(80.0),
                                    ..container.elements[0].container_element().unwrap().elements[4]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        ..container.elements[0].container_element().unwrap().clone()
                    }
                }],
                calculated_width: Some(75.0),
                calculated_height: Some(40.0),
                ..container
            }
        );
    }

    #[test_log::test]
    fn contained_calculated_show_y_calculates_height_correctly() {
        let container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Show,
            ..Default::default()
        };
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < 0.0001,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    fn contained_calculated_show_y_nested_calculates_height_correctly() {
        let container = ContainerElement {
            elements: vec![Element::Div {
                element: ContainerElement {
                    elements: vec![
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                                ..Default::default()
                            },
                        },
                    ],
                    calculated_width: Some(50.0),
                    calculated_height: Some(80.0),
                    ..Default::default()
                },
            }],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Show,
            ..Default::default()
        };
        let height = container.contained_calculated_height();
        let expected = 80.0;

        assert_eq!(
            (height - expected).abs() < 0.0001,
            true,
            "height expected to be {expected} (actual={height})"
        );
    }

    #[test_log::test]
    fn resize_children_show_y_nested_expands_parent_height_correctly() {
        let mut container = ContainerElement {
            elements: vec![Element::Div {
                element: ContainerElement {
                    elements: vec![
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                                ..Default::default()
                            },
                        },
                        Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                calculated_width: Some(25.0),
                                calculated_height: Some(40.0),
                                calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                                ..Default::default()
                            },
                        },
                    ],
                    calculated_width: Some(50.0),
                    calculated_height: Some(80.0),
                    ..Default::default()
                },
            }],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Show,
            ..Default::default()
        };
        let resized = container.resize_children();

        assert_eq!(resized, true);
        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement {
                                    calculated_height: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_height: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::Div {
                                element: ContainerElement {
                                    calculated_height: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[2]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        calculated_width: Some(50.0),
                        calculated_height: Some(80.0),
                        ..Default::default()
                    },
                }],
                calculated_width: Some(50.0),
                calculated_height: Some(80.0),
                direction: LayoutDirection::Row,
                ..container
            }
        );
    }

    #[test_log::test]
    fn resize_children_resizes_when_a_new_row_was_shifted_into_view() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let resized = container.resize_children();

        assert_eq!(resized, true);
        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            calculated_height: Some(20.0),
                            ..container.elements[0].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_height: Some(20.0),
                            ..container.elements[1].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_height: Some(20.0),
                            ..container.elements[2].container_element().unwrap().clone()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn resize_children_allows_expanding_height_for_overflow_y_scroll() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0 + f32::from(get_scrollbar_size())),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let resized = container.resize_children();

        assert_eq!(resized, true);
        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            calculated_height: Some(40.0),
                            ..container.elements[0].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_height: Some(40.0),
                            ..container.elements[1].container_element().unwrap().clone()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            calculated_height: Some(40.0),
                            ..container.elements[2].container_element().unwrap().clone()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn handle_overflow_wraps_single_row_overflow_content_correctly() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let mut shifted = false;
        while container.handle_overflow() {
            shifted = true;
        }

        assert_eq!(shifted, true);
        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(25.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(20.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            ..Default::default()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn handle_overflow_wraps_multi_row_overflow_content_correctly() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        let mut shifted = false;
        while container.handle_overflow() {
            shifted = true;
        }

        let row_height = 40.0 / 3.0;

        assert_eq!(shifted, true);
        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(row_height),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(row_height),
                            calculated_x: Some(25.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(row_height),
                            calculated_x: Some(0.0),
                            calculated_y: Some(row_height),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(row_height),
                            calculated_x: Some(25.0),
                            calculated_y: Some(row_height),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 1 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(row_height),
                            calculated_x: Some(0.0),
                            calculated_y: Some(row_height * 2.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 2, col: 0 }),
                            ..Default::default()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn handle_overflow_wraps_row_content_correctly_in_overflow_y_scroll() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        calculated_width: Some(25.0),
                        calculated_height: Some(40.0),
                        ..Default::default()
                    },
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
        while container.handle_overflow() {
            shifted = true;
        }

        assert_eq!(shifted, true);
        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(25.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(40.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            ..Default::default()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_inner_wraps_row_content_correctly() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(25.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            ..Default::default()
                        },
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(20.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            ..Default::default()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_inner_wraps_row_content_with_nested_width_correctly() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        elements: vec![Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                ..Default::default()
                            },
                        }],
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        elements: vec![Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                ..Default::default()
                            },
                        }],
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        elements: vec![Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                ..Default::default()
                            },
                        }],
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        container.calc();

        let remainder = 50.0f32 / 3_f32; // 16.66666

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![Element::Div {
                                element: ContainerElement {
                                    width: Some(Number::Integer(25)),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::default()),
                                    ..Default::default()
                                },
                            }],
                            calculated_width: Some(remainder),
                            calculated_height: Some(40.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            ..Default::default()
                        }
                    },
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![Element::Div {
                                element: ContainerElement {
                                    width: Some(Number::Integer(25)),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::default()),
                                    ..Default::default()
                                },
                            }],
                            calculated_width: Some(remainder),
                            calculated_height: Some(40.0),
                            calculated_x: Some(remainder),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            ..Default::default()
                        }
                    },
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![Element::Div {
                                element: ContainerElement {
                                    width: Some(Number::Integer(25)),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(40.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::default()),
                                    ..Default::default()
                                },
                            }],
                            calculated_width: Some(remainder),
                            calculated_height: Some(40.0),
                            calculated_x: Some(remainder * 2.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 2 }),
                            ..Default::default()
                        }
                    }
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_inner_wraps_row_content_with_nested_explicit_width_correctly() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        elements: vec![Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                ..Default::default()
                            },
                        }],
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        elements: vec![Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                ..Default::default()
                            },
                        }],
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        width: Some(Number::Integer(25)),
                        elements: vec![Element::Div {
                            element: ContainerElement {
                                width: Some(Number::Integer(25)),
                                ..Default::default()
                            },
                        }],
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::default(),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            elements: vec![Element::Div {
                                element: ContainerElement {
                                    width: Some(Number::Integer(25)),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(20.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::default()),
                                    ..Default::default()
                                },
                            }],
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 0 }),
                            ..Default::default()
                        }
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            elements: vec![Element::Div {
                                element: ContainerElement {
                                    width: Some(Number::Integer(25)),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(20.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::default()),
                                    ..Default::default()
                                },
                            }],
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(25.0),
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 0, col: 1 }),
                            ..Default::default()
                        }
                    },
                    Element::Div {
                        element: ContainerElement {
                            width: Some(Number::Integer(25)),
                            elements: vec![Element::Div {
                                element: ContainerElement {
                                    width: Some(Number::Integer(25)),
                                    calculated_width: Some(25.0),
                                    calculated_height: Some(20.0),
                                    calculated_x: Some(0.0),
                                    calculated_y: Some(0.0),
                                    calculated_position: Some(LayoutPosition::default()),
                                    ..Default::default()
                                },
                            }],
                            calculated_width: Some(25.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(20.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            ..Default::default()
                        }
                    }
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_with_row_content_in_right_pane_above_a_vertial_split() {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement::default(),
                            },
                            Element::Div {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::Div {
                                            element: ContainerElement::default(),
                                        },
                                        Element::Div {
                                            element: ContainerElement::default(),
                                        },
                                    ],
                                    direction: LayoutDirection::Row,
                                    ..Default::default()
                                },
                            },
                        ],
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        elements: vec![],
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(40.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
                                },
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(20.0),
                                        calculated_x: Some(50.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        direction: LayoutDirection::Row,
                                        elements: vec![
                                            Element::Div {
                                                element: ContainerElement {
                                                    calculated_width: Some(25.0),
                                                    calculated_height: Some(20.0),
                                                    calculated_x: Some(0.0),
                                                    calculated_y: Some(0.0),
                                                    calculated_position: Some(
                                                        LayoutPosition::Default
                                                    ),
                                                    ..Default::default()
                                                },
                                            },
                                            Element::Div {
                                                element: ContainerElement {
                                                    calculated_width: Some(25.0),
                                                    calculated_height: Some(20.0),
                                                    calculated_x: Some(25.0),
                                                    calculated_y: Some(0.0),
                                                    calculated_position: Some(
                                                        LayoutPosition::Default
                                                    ),
                                                    ..Default::default()
                                                },
                                            },
                                        ],
                                        ..Default::default()
                                    },
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
                    },
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![],
                            calculated_width: Some(100.0),
                            calculated_height: Some(20.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(20.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..Default::default()
                        },
                    }
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_horizontal_split_with_row_content_in_right_pane_above_a_vertial_split_with_a_specified_height(
    ) {
        let mut container = ContainerElement {
            elements: vec![
                Element::Div {
                    element: ContainerElement {
                        elements: vec![
                            Element::Div {
                                element: ContainerElement::default(),
                            },
                            Element::Div {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::Div {
                                            element: ContainerElement::default(),
                                        },
                                        Element::Div {
                                            element: ContainerElement::default(),
                                        },
                                    ],
                                    direction: LayoutDirection::Row,
                                    ..Default::default()
                                },
                            },
                        ],
                        direction: LayoutDirection::Row,
                        ..Default::default()
                    },
                },
                Element::Div {
                    element: ContainerElement {
                        elements: vec![],
                        height: Some(Number::Integer(10)),
                        ..Default::default()
                    },
                },
            ],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(70.0),
                                        calculated_x: Some(0.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        ..Default::default()
                                    },
                                },
                                Element::Div {
                                    element: ContainerElement {
                                        calculated_width: Some(50.0),
                                        calculated_height: Some(70.0),
                                        calculated_x: Some(50.0),
                                        calculated_y: Some(0.0),
                                        calculated_position: Some(LayoutPosition::Default),
                                        direction: LayoutDirection::Row,
                                        elements: vec![
                                            Element::Div {
                                                element: ContainerElement {
                                                    calculated_width: Some(25.0),
                                                    calculated_height: Some(70.0),
                                                    calculated_x: Some(0.0),
                                                    calculated_y: Some(0.0),
                                                    calculated_position: Some(
                                                        LayoutPosition::Default
                                                    ),
                                                    ..Default::default()
                                                },
                                            },
                                            Element::Div {
                                                element: ContainerElement {
                                                    calculated_width: Some(25.0),
                                                    calculated_height: Some(70.0),
                                                    calculated_x: Some(25.0),
                                                    calculated_y: Some(0.0),
                                                    calculated_position: Some(
                                                        LayoutPosition::Default
                                                    ),
                                                    ..Default::default()
                                                },
                                            },
                                        ],
                                        ..Default::default()
                                    },
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
                    },
                    Element::Div {
                        element: ContainerElement {
                            elements: vec![],
                            height: Some(Number::Integer(10)),
                            calculated_width: Some(100.0),
                            calculated_height: Some(10.0),
                            calculated_x: Some(0.0),
                            calculated_y: Some(70.0),
                            calculated_position: Some(LayoutPosition::Default),
                            ..Default::default()
                        },
                    }
                ],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_table_column_and_row_sizes() {
        let mut container = ContainerElement {
            elements: vec![Element::Table {
                element: ContainerElement {
                    elements: vec![
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(40)),
                                                    height: Some(Number::Integer(10)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(30)),
                                                    height: Some(Number::Integer(20)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(10)),
                                                    height: Some(Number::Integer(40)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(20)),
                                                    height: Some(Number::Integer(30)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                    ],
                    ..Default::default()
                },
            }],
            calculated_width: Some(70.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Table {
                    element: ContainerElement {
                        elements: vec![
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(40.0),
                                                        calculated_height: Some(10.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(40.0),
                                                calculated_height: Some(20.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(30.0),
                                                        calculated_height: Some(20.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(30.0),
                                                calculated_height: Some(20.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(70.0),
                                    calculated_height: Some(20.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(10.0),
                                                        calculated_height: Some(40.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(40.0),
                                                calculated_height: Some(40.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(20.0),
                                                        calculated_height: Some(30.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(30.0),
                                                calculated_height: Some(40.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(70.0),
                                    calculated_height: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        calculated_width: Some(70.0),
                        calculated_height: Some(20.0 + 40.0),
                        ..container.elements[0].container_element().unwrap().clone()
                    },
                }],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_table_column_and_row_sizes_and_expand_to_fill_width() {
        let mut container = ContainerElement {
            elements: vec![Element::Table {
                element: ContainerElement {
                    elements: vec![
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(40)),
                                                    height: Some(Number::Integer(10)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(30)),
                                                    height: Some(Number::Integer(20)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(10)),
                                                    height: Some(Number::Integer(40)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(20)),
                                                    height: Some(Number::Integer(30)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                    ],
                    ..Default::default()
                },
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Table {
                    element: ContainerElement {
                        elements: vec![
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(40.0),
                                                        calculated_height: Some(10.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(55.0),
                                                calculated_height: Some(20.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(30.0),
                                                        calculated_height: Some(20.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(45.0),
                                                calculated_height: Some(20.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(20.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(10.0),
                                                        calculated_height: Some(40.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(55.0),
                                                calculated_height: Some(40.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(20.0),
                                                        calculated_height: Some(30.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(45.0),
                                                calculated_height: Some(40.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(40.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(20.0 + 40.0),
                        ..container.elements[0].container_element().unwrap().clone()
                    },
                }],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells() {
        let mut container = ContainerElement {
            elements: vec![Element::Table {
                element: ContainerElement {
                    elements: vec![
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(40)),
                                                    height: Some(Number::Integer(10)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement {
                                                    elements: vec![],
                                                    width: Some(Number::Integer(20)),
                                                    height: Some(Number::Integer(30)),
                                                    ..Default::default()
                                                },
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                    ],
                    ..Default::default()
                },
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Table {
                    element: ContainerElement {
                        elements: vec![
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(40.0),
                                                        calculated_height: Some(10.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(60.0),
                                                calculated_height: Some(10.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(40.0),
                                                calculated_height: Some(10.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(10.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(60.0),
                                                calculated_height: Some(30.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(20.0),
                                                        calculated_height: Some(30.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(40.0),
                                                calculated_height: Some(30.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(30.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(10.0 + 30.0),
                        ..container.elements[0].container_element().unwrap().clone()
                    },
                }],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_table_column_and_row_sizes_and_auto_size_unsized_cells_when_all_are_unsized() {
        let mut container = ContainerElement {
            elements: vec![Element::Table {
                element: ContainerElement {
                    elements: vec![
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement::default(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement::default(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement::default(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Div {
                                                element: ContainerElement::default(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                    ],
                    ..Default::default()
                },
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Table {
                    element: ContainerElement {
                        elements: vec![
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(25.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: vec![Element::Div {
                                                    element: ContainerElement {
                                                        elements: vec![],
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                }],
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(25.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(25.0 + 25.0),
                        ..container.elements[0].container_element().unwrap().clone()
                    },
                }],
                ..container
            }
        );
    }

    #[test_log::test]
    fn calc_can_calc_table_column_and_row_sizes_and_auto_size_raw_data() {
        let mut container = ContainerElement {
            elements: vec![Element::Table {
                element: ContainerElement {
                    elements: vec![
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Raw {
                                                value: "test".to_string(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Raw {
                                                value: "test".to_string(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                        Element::TR {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Raw {
                                                value: "test".to_string(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                    Element::TD {
                                        element: ContainerElement {
                                            elements: vec![Element::Raw {
                                                value: "test".to_string(),
                                            }],
                                            ..ContainerElement::default()
                                        },
                                    },
                                ],
                                ..Default::default()
                            },
                        },
                    ],
                    ..Default::default()
                },
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Table {
                    element: ContainerElement {
                        elements: vec![
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements
                                                    .clone(),
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements
                                                    .clone(),
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(25.0),
                                    ..container.elements[0].container_element().unwrap().elements[0]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                            Element::TR {
                                element: ContainerElement {
                                    elements: vec![
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements
                                                    .clone(),
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                        Element::TD {
                                            element: ContainerElement {
                                                elements: container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements
                                                    .clone(),
                                                calculated_width: Some(50.0),
                                                calculated_height: Some(25.0),
                                                ..container.elements[0]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .elements[1]
                                                    .container_element()
                                                    .unwrap()
                                                    .clone()
                                            },
                                        },
                                    ],
                                    calculated_width: Some(100.0),
                                    calculated_height: Some(25.0),
                                    ..container.elements[0].container_element().unwrap().elements[1]
                                        .container_element()
                                        .unwrap()
                                        .clone()
                                },
                            },
                        ],
                        calculated_width: Some(100.0),
                        calculated_height: Some(25.0 + 25.0),
                        ..container.elements[0].container_element().unwrap().clone()
                    },
                }],
                ..container
            }
        );
    }
    #[test_log::test]
    fn calc_can_calc_table_column_and_row_sizes_with_tbody() {
        let mut container = ContainerElement {
            elements: vec![Element::Table {
                element: ContainerElement {
                    elements: vec![Element::TBody {
                        element: ContainerElement {
                            elements: vec![
                                Element::TR {
                                    element: ContainerElement {
                                        elements: vec![
                                            Element::TD {
                                                element: ContainerElement {
                                                    elements: vec![Element::Raw {
                                                        value: "test".to_string(),
                                                    }],
                                                    ..ContainerElement::default()
                                                },
                                            },
                                            Element::TD {
                                                element: ContainerElement {
                                                    elements: vec![Element::Raw {
                                                        value: "test".to_string(),
                                                    }],
                                                    ..ContainerElement::default()
                                                },
                                            },
                                        ],
                                        ..Default::default()
                                    },
                                },
                                Element::TR {
                                    element: ContainerElement {
                                        elements: vec![
                                            Element::TD {
                                                element: ContainerElement {
                                                    elements: vec![Element::Raw {
                                                        value: "test".to_string(),
                                                    }],
                                                    ..ContainerElement::default()
                                                },
                                            },
                                            Element::TD {
                                                element: ContainerElement {
                                                    elements: vec![Element::Raw {
                                                        value: "test".to_string(),
                                                    }],
                                                    ..ContainerElement::default()
                                                },
                                            },
                                        ],
                                        ..Default::default()
                                    },
                                },
                            ],
                            ..Default::default()
                        },
                    }],
                    ..Default::default()
                },
            }],
            calculated_width: Some(100.0),
            calculated_height: Some(80.0),
            ..Default::default()
        };
        container.calc();

        assert_eq!(
            container.clone(),
            ContainerElement {
                elements: vec![Element::Table {
                    element: ContainerElement {
                        elements: vec![Element::TBody {
                            element: ContainerElement {
                                elements: vec![
                                    Element::TR {
                                        element: ContainerElement {
                                            elements: vec![
                                                Element::TD {
                                                    element: ContainerElement {
                                                        elements: container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements
                                                            .clone(),
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                },
                                                Element::TD {
                                                    element: ContainerElement {
                                                        elements: container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements
                                                            .clone(),
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                },
                                            ],
                                            calculated_width: Some(100.0),
                                            calculated_height: Some(25.0),
                                            ..container.elements[0]
                                                .container_element()
                                                .unwrap()
                                                .elements[0]
                                                .container_element()
                                                .unwrap()
                                                .elements[0]
                                                .container_element()
                                                .unwrap()
                                                .clone()
                                        },
                                    },
                                    Element::TR {
                                        element: ContainerElement {
                                            elements: vec![
                                                Element::TD {
                                                    element: ContainerElement {
                                                        elements: container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements
                                                            .clone(),
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                },
                                                Element::TD {
                                                    element: ContainerElement {
                                                        elements: container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements
                                                            .clone(),
                                                        calculated_width: Some(50.0),
                                                        calculated_height: Some(25.0),
                                                        ..container.elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[0]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .elements[1]
                                                            .container_element()
                                                            .unwrap()
                                                            .clone()
                                                    },
                                                },
                                            ],
                                            calculated_width: Some(100.0),
                                            calculated_height: Some(25.0),
                                            ..container.elements[0]
                                                .container_element()
                                                .unwrap()
                                                .elements[0]
                                                .container_element()
                                                .unwrap()
                                                .elements[1]
                                                .container_element()
                                                .unwrap()
                                                .clone()
                                        },
                                    },
                                ],
                                calculated_width: Some(100.0),
                                calculated_height: Some(25.0 + 25.0),
                                ..container.elements[0].container_element().unwrap().elements[0]
                                    .container_element()
                                    .unwrap()
                                    .clone()
                            },
                        }],
                        ..container.elements[0].container_element().unwrap().clone()
                    },
                }],
                ..container
            }
        );
    }
}
