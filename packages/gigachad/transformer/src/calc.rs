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
        let direction = self.direction;
        let container_width = self.calculated_width.unwrap_or(0.0);
        let container_height = self.calculated_height.unwrap_or(0.0);

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

    fn calc_sized_element_sizes(
        elements: &mut [&mut Element],
        direction: LayoutDirection,
        container_width: f32,
        container_height: f32,
    ) -> f32 {
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

    #[allow(clippy::similar_names)]
    fn handle_overflow(&mut self) -> bool {
        let mut layout_shifted = false;

        let direction = self.direction;
        let overflow = self.overflow;
        let container_width = self.calculated_width.unwrap_or(0.0);
        let container_height = self.calculated_height.unwrap_or(0.0);

        let mut x = 0.0;
        let mut y = 0.0;
        let mut max_width = 0.0;
        let mut max_height = 0.0;
        let mut row = 0;
        let mut col = 0;

        for element in &mut self.elements {
            // TODO:
            // need to handle non container elements that have a width/height that is the split
            // remainder of the container width/height
            if let Some(element) = element.container_element_mut() {
                let width = element.calculated_width.unwrap_or(0.0);
                let height = element.calculated_height.unwrap_or(0.0);

                element.calculated_x.replace(x);
                element.calculated_y.replace(y);

                match overflow {
                    LayoutOverflow::Scroll | LayoutOverflow::Squash => {
                        element
                            .calculated_position
                            .replace(LayoutPosition::default());
                    }
                    LayoutOverflow::Wrap => {
                        if let Some(LayoutPosition::Wrap {
                            row: old_row,
                            col: old_col,
                        }) = element.calculated_position
                        {
                            if row != old_row || col != old_col {
                                layout_shifted = true;
                            }
                        } else {
                            layout_shifted = true;
                        }

                        element
                            .calculated_position
                            .replace(LayoutPosition::Wrap { row, col });

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
                                } else {
                                    x += width;
                                    col += 1;
                                }
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
                                } else {
                                    y += height;
                                    row += 1;
                                }
                            }
                        }
                    }
                }

                max_height = if max_height > height {
                    max_height
                } else {
                    height
                };
                max_width = if max_width > width { max_width } else { width };

                match direction {
                    LayoutDirection::Row => {
                        x += element.calculated_width.unwrap_or(0.0);
                    }
                    LayoutDirection::Column => {
                        y += element.calculated_height.unwrap_or(0.0);
                    }
                }

                layout_shifted = layout_shifted || element.resize_children();
            }
        }

        layout_shifted || self.resize_children()
    }

    fn contained_sized_width(&self) -> f32 {
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
                                .and_then(|x| x.width)
                                .map_or(0.0, |x| {
                                    calc_number(x, self.calculated_width.unwrap_or(0.0))
                                })
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
                                .and_then(|x| x.width)
                                .map_or(0.0, |x| {
                                    calc_number(x, self.calculated_width.unwrap_or(0.0))
                                })
                        })
                        .max_by(order_float)
                        .unwrap_or(0.0)
                })
                .max_by(order_float)
                .unwrap_or(0.0),
        }
    }

    fn contained_sized_height(&self) -> f32 {
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
                                .and_then(|x| x.height)
                                .map_or(0.0, |x| {
                                    calc_number(x, self.calculated_height.unwrap_or(0.0))
                                })
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
                                .and_then(|x| x.height)
                                .map_or(0.0, |x| {
                                    calc_number(x, self.calculated_height.unwrap_or(0.0))
                                })
                        })
                        .max_by(order_float)
                        .unwrap_or(0.0)
                })
                .max_by(order_float)
                .unwrap_or(0.0),
        }
    }

    fn contained_calculated_width(&self) -> f32 {
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

    fn contained_calculated_height(&self) -> f32 {
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

    fn resize_children(&mut self) -> bool {
        let (Some(width), Some(height)) = (self.calculated_width, self.calculated_height) else {
            moosicbox_assert::die_or_panic!(
                "ContainerElement missing calculated_width and/or calculated_height: {self:?}"
            );
        };
        let mut layout_shifted = false;

        let contained_calculated_width = self.contained_calculated_width();
        let contained_calculated_height = self.contained_calculated_height();

        log::trace!(
            "width={width} contained_calculated_width={contained_calculated_width} height={height} contained_calculated_height={contained_calculated_height} {}",
            self.direction,
        );

        if width < contained_calculated_width {
            let contained_sized_width = self.contained_sized_width();
            log::trace!(
                "resize_children: {} updated width from {width} -> {contained_calculated_width} (contained_sized_width={contained_sized_width})",
                self.direction,
            );
            self.calculated_width.replace(contained_calculated_width);
            layout_shifted = true;
        }
        if height < contained_calculated_height {
            let contained_sized_height = self.contained_sized_height();
            log::trace!(
                "resize_children: {} updated height from {height} -> {contained_calculated_height} (contained_sized_height={contained_sized_height})",
                self.direction,
            );

            self.calculated_height.replace(contained_calculated_height);
            layout_shifted = true;
        }

        layout_shifted
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
    use pretty_assertions::assert_eq;

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
            overflow: LayoutOverflow::default(),
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
            overflow: LayoutOverflow::Wrap,
            ..Default::default()
        };
        let width = container.contained_sized_width();
        let expected = 50.0;

        assert_eq!(
            (width - expected).abs() < 0.0001,
            true,
            "width expected to be {expected} (actual={width})"
        );
    }

    #[test_log::test]
    fn contained_sized_height_calculates_wrapped_height_correctly() {
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
            overflow: LayoutOverflow::Wrap,
            ..Default::default()
        };
        let height = container.contained_sized_height();
        let expected = 0.0;

        assert_eq!(
            (height - expected).abs() < 0.0001,
            true,
            "height expected to be {expected} (actual={height})"
        );
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
            overflow: LayoutOverflow::Wrap,
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
            overflow: LayoutOverflow::Wrap,
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

    #[ignore]
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
            overflow: LayoutOverflow::Wrap,
            ..Default::default()
        };
        let shifted = container.resize_children();

        assert_eq!(shifted, true);
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

    #[ignore]
    #[test_log::test]
    fn handle_overflow_wraps_row_content_correctly() {
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
            overflow: LayoutOverflow::Wrap,
            ..Default::default()
        };
        let shifted = container.handle_overflow();

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
                            calculated_y: Some(0.0),
                            calculated_position: Some(LayoutPosition::Wrap { row: 1, col: 0 }),
                            ..Default::default()
                        },
                    },
                ],
                ..container
            }
        );
    }

    #[ignore]
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
            overflow: LayoutOverflow::Wrap,
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
                            calculated_y: Some(0.0),
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
}
