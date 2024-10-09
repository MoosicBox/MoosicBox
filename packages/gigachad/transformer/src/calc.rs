use itertools::Itertools;

use crate::{ContainerElement, Element, LayoutDirection, LayoutOverflow, LayoutPosition, Number};

pub trait Calc {
    fn calc(&mut self);
}

impl Calc for ContainerElement {
    fn calc(&mut self) {
        self.calc_inner();
    }
}

impl ContainerElement {
    fn calc_inner(&mut self) {
        let (Some(container_width), Some(container_height)) =
            (self.calculated_width, self.calculated_height)
        else {
            moosicbox_assert::die_or_panic!(
                "calc_inner requires calculated_width and calculated_height to be set"
            );
        };

        self.calc_hardsized_elements();

        let direction = self.direction;

        let (mut sized_elements, mut unsized_elements): (Vec<_>, Vec<_>) =
            self.elements.iter_mut().partition(|x| {
                x.container_element().is_some_and(|x| match direction {
                    LayoutDirection::Row => x.width.is_some(),
                    LayoutDirection::Column => x.height.is_some(),
                })
            });

        let remainder = Self::calc_sized_element_sizes(
            &mut sized_elements,
            direction,
            container_width,
            container_height,
        );

        Self::calc_unsized_element_sizes(
            &mut unsized_elements,
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

            if let Some(width) = element.width {
                match width {
                    Number::Real(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_width={x} {element:?}"
                        );
                        element.calculated_width.replace(x);
                    }
                    Number::Integer(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_width={x} {element:?}"
                        );
                        #[allow(clippy::cast_precision_loss)]
                        element.calculated_width.replace(x as f32);
                    }
                    Number::RealPercent(_) | Number::IntegerPercent(_) => {}
                }
            }
            if let Some(height) = element.height {
                match height {
                    Number::Real(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_height={x} {element:?}"
                        );
                        element.calculated_height.replace(x);
                    }
                    Number::Integer(x) => {
                        log::trace!(
                            "calc_hardsized_elements: setting calculated_height={x} {element:?}"
                        );
                        #[allow(clippy::cast_precision_loss)]
                        element.calculated_height.replace(x as f32);
                    }
                    Number::RealPercent(_) | Number::IntegerPercent(_) => {}
                }
            }
        }
    }

    fn calc_sized_element_sizes(
        elements: &mut [&mut Element],
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
    ) -> f32 {
        log::debug!("calc_unsized_element_sizes: container_width={container_width} container_height={container_height}");

        let mut remainder = match direction {
            LayoutDirection::Row => container_width,
            LayoutDirection::Column => container_height,
        };

        for element in elements
            .iter_mut()
            .filter_map(|x| x.container_element_mut())
        {
            match direction {
                LayoutDirection::Row => {
                    let width = calc_number(element.width.unwrap(), container_width);
                    let height = element
                        .height
                        .map_or(container_height, |x| calc_number(x, container_height));
                    element.calculated_width.replace(width);
                    element.calculated_height.replace(height);
                    element.calc_inner();
                    remainder -= width;
                }
                LayoutDirection::Column => {
                    let width = element
                        .width
                        .map_or(container_width, |x| calc_number(x, container_width));
                    let height = calc_number(element.height.unwrap(), container_height);
                    element.calculated_width.replace(width);
                    element.calculated_height.replace(height);
                    element.calc_inner();
                    remainder -= height;
                }
            }
        }

        remainder
    }

    fn calc_unsized_element_sizes(
        elements: &mut [&mut Element],
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
        remainder: f32,
    ) {
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

        for element in elements
            .iter_mut()
            .filter_map(|x| x.container_element_mut())
        {
            match direction {
                LayoutDirection::Row => {
                    let height = element
                        .height
                        .map_or(container_height, |x| calc_number(x, container_height));
                    element.calculated_height.replace(height);
                    element
                        .calculated_width
                        .replace(evenly_split_remaining_size);
                    element.calc_inner();
                }
                LayoutDirection::Column => {
                    let width = element
                        .width
                        .map_or(container_width, |x| calc_number(x, container_width));
                    element.calculated_width.replace(width);
                    element
                        .calculated_height
                        .replace(evenly_split_remaining_size);
                    element.calc_inner();
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_overflow(&mut self) -> bool {
        log::trace!("handle_overflow: processing self\n{self:?}");
        let mut layout_shifted = false;

        let direction = self.direction;
        let overflow = self.overflow_x;
        let container_width = self.calculated_width.unwrap_or(0.0);
        let container_height = self.calculated_height.unwrap_or(0.0);

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
                let width = element.calculated_width.unwrap_or(0.0);
                let height = element.calculated_height.unwrap_or(0.0);

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

        self.resize_children();
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

    pub fn contained_sized_width(&self) -> Option<f32> {
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
                                .map(|x| calc_number(x, calculated_width))
                                .or_else(|| x.contained_sized_width())
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
                                    .map(|x| calc_number(x, calculated_width))
                                    .or_else(|| x.contained_sized_width())
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

    pub fn contained_sized_height(&self) -> Option<f32> {
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
                                    .map(|x| calc_number(x, calculated_height))
                                    .or_else(|| x.contained_sized_height())
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
                                .map(|x| calc_number(x, calculated_height))
                                .or_else(|| x.contained_sized_height())
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
                                .and_then(|x| x.calculated_width)
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
                                .and_then(|x| x.calculated_width)
                                .unwrap_or(0.0)
                        })
                        .max_by(order_float)
                        .unwrap_or(0.0)
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

    fn resize_children(&mut self) -> bool {
        if self.elements.is_empty() {
            log::trace!("resize_children: no children");
            return false;
        }
        let (Some(width), Some(height)) = (self.calculated_width, self.calculated_height) else {
            moosicbox_assert::die_or_panic!(
                "ContainerElement missing calculated_width and/or calculated_height: {self:?}"
            );
        };

        let mut resized = false;

        let contained_calculated_width = self.contained_calculated_width();
        let contained_calculated_height = self.contained_calculated_height();

        log::trace!(
            "calculated_width={width} contained_calculated_width={contained_calculated_width} calculated_height={height} contained_calculated_height={contained_calculated_height} {} overflow_x={} overflow_y={} width={:?} height={:?}",
            self.direction,
            self.overflow_x,
            self.overflow_y,
            self.width,
            self.height,
        );

        if width < contained_calculated_width {
            match self.overflow_x {
                LayoutOverflow::Auto | LayoutOverflow::Scroll => {}
                LayoutOverflow::Show => {
                    if self.width.is_none() {
                        self.calculated_width.replace(contained_calculated_width);
                        resized = true;
                    }
                }
                LayoutOverflow::Squash | LayoutOverflow::Wrap => {
                    let contained_sized_width = self.contained_sized_width().unwrap_or(0.0);
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
            match self.overflow_y {
                LayoutOverflow::Auto | LayoutOverflow::Scroll => {}
                LayoutOverflow::Show => {
                    if self.height.is_none() {
                        self.calculated_height.replace(contained_calculated_height);
                        resized = true;
                    }
                }
                LayoutOverflow::Squash | LayoutOverflow::Wrap => {
                    let contained_sized_height = self.contained_sized_height().unwrap_or(0.0);
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

#[allow(clippy::module_name_repetitions)]
#[must_use]
pub fn calc_number(number: Number, container: f32) -> f32 {
    match number {
        Number::Real(x) => x,
        #[allow(clippy::cast_precision_loss)]
        Number::Integer(x) => x as f32,
        Number::RealPercent(x) => container * (x / 100.0),
        #[allow(clippy::cast_precision_loss)]
        Number::IntegerPercent(x) => container * (x as f32 / 100.0),
    }
}

#[cfg(test)]
mod test {
    use pretty_assertions::{assert_eq, assert_ne};

    use crate::{
        calc::Calc as _, ContainerElement, Element, LayoutDirection, LayoutOverflow,
        LayoutPosition, Number,
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
        let width = container.contained_sized_width();
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
        let width = container.contained_sized_width();

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
        let height = container.contained_sized_height();
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
        let height = container.contained_sized_height();

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
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let resized = container.resize_children();

        assert_eq!(resized, false);
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
        let shifted = container.handle_overflow();

        assert_eq!(shifted, false);
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
        let shifted = container.handle_overflow();

        let row_height = 40.0 / 3.0;

        assert_eq!(shifted, false);
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
            calculated_width: Some(50.0),
            calculated_height: Some(40.0),
            direction: LayoutDirection::Row,
            overflow_x: LayoutOverflow::Wrap,
            overflow_y: LayoutOverflow::Scroll,
            ..Default::default()
        };
        let shifted = container.handle_overflow();

        assert_eq!(shifted, false);
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
}
