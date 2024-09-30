use std::ops::DerefMut as _;

use crate::{Element, ElementList, LayoutDirection, LayoutOverflow, LayoutPosition, Number};

pub trait Calc {
    fn calc(&mut self, width: f32, height: f32);
}

impl Calc for [Element] {
    fn calc(&mut self, width: f32, height: f32) {
        calc_inner(
            self,
            width,
            height,
            LayoutDirection::default(),
            LayoutOverflow::default(),
        );
    }
}

impl Calc for ElementList {
    fn calc(&mut self, width: f32, height: f32) {
        self.deref_mut().calc(width, height);
    }
}

#[allow(clippy::too_many_lines)]
fn calc_inner(
    elements: &mut [Element],
    container_width: f32,
    container_height: f32,
    direction: LayoutDirection,
    overflow: LayoutOverflow,
) {
    let (mut sized_elements, mut unsized_elements): (Vec<_>, Vec<_>) =
        elements.iter_mut().partition(|x| {
            x.container_element().is_some_and(|x| match direction {
                LayoutDirection::Row => x.width.is_some(),
                LayoutDirection::Column => x.height.is_some(),
            })
        });

    let mut remainder = match direction {
        LayoutDirection::Row => container_width,
        LayoutDirection::Column => container_height,
    };

    for element in sized_elements
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
                calc_inner(
                    &mut element.elements,
                    width,
                    height,
                    element.direction,
                    element.overflow,
                );
                remainder -= width;
            }
            LayoutDirection::Column => {
                let width = element
                    .width
                    .map_or(container_width, |x| calc_number(x, container_width));
                let height = calc_number(element.height.unwrap(), container_height);
                element.calculated_width.replace(width);
                element.calculated_height.replace(height);
                calc_inner(
                    &mut element.elements,
                    width,
                    height,
                    element.direction,
                    element.overflow,
                );
                remainder -= height;
            }
        }
    }

    #[allow(clippy::cast_precision_loss)]
    let evenly_split_remaining_size = remainder / unsized_elements.len() as f32;

    for element in unsized_elements
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
                calc_inner(
                    &mut element.elements,
                    evenly_split_remaining_size,
                    height,
                    element.direction,
                    element.overflow,
                );
            }
            LayoutDirection::Column => {
                let width = element
                    .width
                    .map_or(container_width, |x| calc_number(x, container_width));
                element.calculated_width.replace(width);
                element
                    .calculated_height
                    .replace(evenly_split_remaining_size);
                calc_inner(
                    &mut element.elements,
                    width,
                    evenly_split_remaining_size,
                    element.direction,
                    element.overflow,
                );
            }
        }
    }

    let mut x = 0.0;
    let mut y = 0.0;
    let mut max_width = 0.0;
    let mut max_height = 0.0;
    let mut row = 0;
    let mut col = 0;

    for element in elements {
        if let Some(element) = element.container_element_mut() {
            let width = element.calculated_width.unwrap_or(0.0);
            let height = element.calculated_height.unwrap_or(0.0);

            match overflow {
                LayoutOverflow::Scroll | LayoutOverflow::Squash => {
                    element
                        .calculated_position
                        .replace(LayoutPosition::default());
                }
                LayoutOverflow::Wrap => {
                    match direction {
                        LayoutDirection::Row => {
                            if x > 0.0 && x + width > container_width {
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
                            if y > 0.0 && y + height > container_height {
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

                    element
                        .calculated_position
                        .replace(LayoutPosition::Wrap { row, col });
                }
            }

            element.calculated_x.replace(x);
            element.calculated_y.replace(y);

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
        }
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
