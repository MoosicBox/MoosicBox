use crate::Container;

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
        log::trace!("calc: container={container}");

        fit_width();
        fit_heights();
        grow_shrink_sizing();
        wrap_text();
        position_elements();

        false
    }
}

const fn fit_width() {}

const fn fit_heights() {}

const fn grow_shrink_sizing() {}

const fn wrap_text() {}

const fn position_elements() {}

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
    #[ignore]
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
    #[ignore]
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
        CALCULATOR.calc(&mut container);
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
    #[ignore]
    fn calc_can_calc_horizontal_split_above_a_vertial_split() {
        let mut container = Container {
            children: vec![
                Container {
                    children: vec![Container::default(), Container::default()],
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
    #[ignore]
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
        CALCULATOR.calc(&mut container);
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
    #[ignore]
    fn handle_overflow_y_squash_handles_justify_content_space_evenly_with_padding_and_wraps_elements_properly()
     {
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
    #[ignore]
    fn handle_overflow_y_expand_handles_justify_content_space_evenly_with_padding_and_wraps_elements_properly()
     {
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
    #[ignore]
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
                overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
    #[ignore]
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
    #[ignore]
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
        CALCULATOR.calc(&mut container);
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
        CALCULATOR.calc(&mut container);
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
    #[ignore]
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
            overflow_x: LayoutOverflow::Wrap,
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
