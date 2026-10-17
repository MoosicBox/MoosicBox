use super::*;

fn text(value: &str) -> Container {
    Container {
        element: Element::Text {
            value: value.into(),
        },
        ..Container::default()
    }
}
fn options(width: u16, height: u16) -> LayoutOptions {
    LayoutOptions {
        viewport: Size::new(width, height),
        ..LayoutOptions::default()
    }
}
fn tree(root: &Container, width: u16, height: u16) -> LayoutTree {
    layout(root, &options(width, height), &DefaultLeafMeasure).unwrap()
}
fn fixed(width: i64, height: i64) -> Container {
    Container {
        width: Some(Number::Integer(width * 8)),
        height: Some(Number::Integer(height * 16)),
        ..Container::default()
    }
}
fn cell(content: Vec<Container>, rows: Option<i64>, columns: Option<i64>) -> Container {
    Container {
        element: Element::TD {
            rows: rows.map(Number::Integer),
            columns: columns.map(Number::Integer),
        },
        children: content,
        ..Container::default()
    }
}
fn row(cells: Vec<Container>) -> Container {
    Container {
        element: Element::TR,
        children: cells,
        ..Container::default()
    }
}

#[test]
fn stable_keys_prefer_source_identifiers() {
    let mut node = Container::default();
    assert_eq!(node_key(&node, &[1, 2]), "@1/2");
    node.id = 42;
    assert_eq!(node_key(&node, &[1, 2]), "42");
    node.str_id = Some("title".into());
    assert_eq!(node_key(&node, &[1, 2]), "title");
}

#[test]
fn immutable_source_and_axis_specific_document_metrics() {
    let root = Container {
        children: vec![fixed(3, 2)],
        ..Container::default()
    };
    let before = format!("{root:?}");
    let layout = tree(&root, 20, 8);
    let child = layout.at_path(&[0]).unwrap();
    assert_eq!(child.bounds, CellRect::new(0, 0, 3, 2));
    assert_eq!(before, format!("{root:?}"));
}

#[test]
fn percentages_and_cross_axis_viewport_units() {
    let root = Container {
        children: vec![Container {
            width: Some(Number::IntegerPercent(50)),
            height: Some(Number::IntegerVw(10)),
            ..Container::default()
        }],
        ..Container::default()
    };
    let layout = tree(&root, 40, 20);
    let child = layout.at_path(&[0]).unwrap();
    assert_eq!(child.bounds.width, 20);
    assert_eq!(child.bounds.height, 2); // 10% * (40 * 8) / 16
}

#[test]
fn min_max_content_box_and_padding() {
    let root = Container {
        children: vec![Container {
            width: Some(Number::Integer(800)),
            max_width: Some(Number::Integer(32)),
            min_height: Some(Number::Integer(32)),
            padding_left: Some(Number::Integer(8)),
            padding_right: Some(Number::Integer(8)),
            children: vec![text("x")],
            ..Container::default()
        }],
        ..Container::default()
    };
    let layout = tree(&root, 20, 10);
    let child = layout.at_path(&[0]).unwrap();
    assert_eq!(child.bounds.width, 6);
    assert_eq!(child.content_bounds.width, 4);
    assert_eq!(child.bounds.height, 2);
}

#[test]
fn normal_whitespace_collapses_across_spans() {
    let root = Container {
        children: vec![
            text("  a  "),
            Container {
                element: Element::Span,
                children: vec![text(" \n b ")],
                ..Container::default()
            },
        ],
        ..Container::default()
    };
    let layout = tree(&root, 20, 10);
    let text: String = layout.nodes[0]
        .text
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect();
    assert_eq!(text, "a b");
}

#[test]
fn grapheme_crossing_style_boundary_has_both_source_ranges() {
    let root = Container {
        children: vec![
            text("e"),
            Container {
                element: Element::Span,
                font_weight: Some(FontWeight::Bold),
                children: vec![text("\u{301}")],
                ..Container::default()
            },
        ],
        ..Container::default()
    };
    let layout = tree(&root, 10, 5);
    assert_eq!(layout.nodes[0].text.len(), 1);
    let fragment = &layout.nodes[0].text[0];
    assert_eq!(fragment.text, "e\u{301}");
    assert_eq!(fragment.bounds.width, 1);
    assert_eq!(fragment.sources.len(), 2);
}

#[test]
fn unicode_wrapping_never_splits_emoji_or_wide_cells() {
    let root = Container {
        overflow_wrap: Some(OverflowWrap::Anywhere),
        children: vec![text("界👩‍💻x")],
        ..Container::default()
    };
    let layout = tree(&root, 2, 10);
    let fragments = &layout.nodes[0].text;
    assert_eq!(fragments.len(), 3);
    assert_eq!(fragments[0].bounds, CellRect::new(0, 0, 2, 1));
    assert_eq!(fragments[1].bounds, CellRect::new(0, 1, 2, 1));
    assert_eq!(fragments[2].bounds, CellRect::new(0, 2, 1, 1));
}

#[test]
fn preserve_tabs_crlf_and_control_sanitization() {
    let root = Container {
        white_space: Some(WhiteSpace::Preserve),
        children: vec![text("a\tb\r\nc\x1b")],
        ..Container::default()
    };
    let layout = tree(&root, 20, 8);
    let fragments = &layout.nodes[0].text;
    assert_eq!(fragments[1].bounds.width, 7);
    assert_eq!(fragments[2].bounds.x, 8);
    assert_eq!(fragments[3].bounds.y, 1);
    assert_eq!(fragments[4].text, "�");
}

#[test]
fn ellipsis_and_source_provenance() {
    let root = Container {
        white_space: Some(WhiteSpace::Preserve),
        text_overflow: Some(TextOverflow::Ellipsis),
        children: vec![text("abcdef")],
        ..Container::default()
    };
    let layout = tree(&root, 4, 3);
    let text: String = layout.nodes[0]
        .text
        .iter()
        .map(|fragment| fragment.text.as_str())
        .collect();
    assert_eq!(text, "abc…");
    assert_ne!(layout.nodes[0].text.last().unwrap().sources, []);
}

#[test]
fn spans_retain_hierarchy_and_exact_multiline_hit_regions() {
    let root = Container {
        children: vec![Container {
            element: Element::Anchor {
                target: None,
                href: Some("/next".into()),
            },
            overflow_wrap: Some(OverflowWrap::Anywhere),
            children: vec![text("abcdef")],
            ..Container::default()
        }],
        ..Container::default()
    };
    let layout = tree(&root, 4, 8);
    let link = layout.at_path(&[0]).unwrap();
    assert_eq!(link.bounds.height, 2);
    assert_eq!(link.hit_regions.len(), 6);
    assert_eq!(
        layout.at_path(&[0, 0]).unwrap().parent_key.as_ref(),
        Some(&link.key)
    );
    assert!(
        layout
            .hit_test(3, 1)
            .is_some_and(|node| node.path.is_empty())
    );
}

#[test]
fn row_flex_residual_cells_are_conserved() {
    let mut a = Container {
        flex: Some(hyperchad_transformer::Flex::default()),
        ..fixed(0, 1)
    };
    a.width = None;
    let root = Container {
        direction: LayoutDirection::Row,
        children: vec![a.clone(), a.clone(), a],
        ..Container::default()
    };
    let layout = tree(&root, 10, 5);
    let children: Vec<_> = layout
        .nodes
        .iter()
        .filter(|node| node.path.len() == 1)
        .collect();
    assert_eq!(
        children.iter().map(|node| node.bounds.width).sum::<u64>(),
        10
    );
    assert_eq!(children.last().unwrap().bounds.right(), 10);
}

#[test]
fn wrap_grid_and_row_gaps() {
    let root = Container {
        direction: LayoutDirection::Row,
        overflow_x: LayoutOverflow::Wrap { grid: true },
        grid_cell_size: Some(Number::Integer(24)),
        column_gap: Some(Number::Integer(8)),
        row_gap: Some(Number::Integer(16)),
        children: vec![fixed(3, 1), fixed(3, 1), fixed(3, 1)],
        ..Container::default()
    };
    let layout = tree(&root, 7, 8);
    assert_eq!(layout.at_path(&[1]).unwrap().bounds.x, 4);
    assert_eq!(layout.at_path(&[2]).unwrap().bounds.y, 2);
}

#[test]
fn details_collapsed_state_uses_same_node_key() {
    let root = Container {
        children: vec![Container {
            element: Element::Details { open: Some(false) },
            str_id: Some("more".into()),
            children: vec![
                Container {
                    element: Element::Summary,
                    children: vec![text("summary")],
                    ..Container::default()
                },
                text("body"),
            ],
            ..Container::default()
        }],
        ..Container::default()
    };
    let mut opts = options(20, 8);
    let closed = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    assert!(closed.at_path(&[0, 1]).is_none());
    opts.open_details.insert("more".into(), true);
    let open = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    assert!(open.at_path(&[0, 1]).is_some());
}

#[test]
fn fixed_position_ignores_scroll_and_ancestor_clip() {
    let root = Container {
        overflow_y: LayoutOverflow::Scroll,
        children: vec![
            fixed(1, 10),
            Container {
                position: Some(Position::Fixed),
                right: Some(Number::Integer(0)),
                top: Some(Number::Integer(0)),
                ..fixed(2, 1)
            },
        ],
        ..Container::default()
    };
    let mut opts = options(10, 3);
    opts.scroll_offsets
        .insert(node_key(&root, &[]), ScrollOffset { x: 0, y: 4 });
    let layout = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    assert_eq!(layout.at_path(&[0]).unwrap().bounds.y, -4);
    assert_eq!(
        layout.at_path(&[1]).unwrap().bounds,
        CellRect::new(8, 0, 2, 1)
    );
}

#[test]
fn absolute_uses_nearest_positioned_ancestor() {
    let root = Container {
        children: vec![Container {
            position: Some(Position::Relative),
            children: vec![Container {
                children: vec![Container {
                    position: Some(Position::Absolute),
                    right: Some(Number::Integer(0)),
                    ..fixed(2, 1)
                }],
                ..fixed(5, 2)
            }],
            ..fixed(10, 4)
        }],
        ..Container::default()
    };
    let layout = tree(&root, 20, 10);
    assert_eq!(layout.at_path(&[0, 0, 0]).unwrap().bounds.x, 8);
}

#[test]
fn nested_scroll_offsets_are_clamped_and_composed() {
    let root = Container {
        overflow_y: LayoutOverflow::Scroll,
        children: vec![
            fixed(1, 2),
            Container {
                overflow_y: LayoutOverflow::Scroll,
                children: vec![fixed(1, 10)],
                ..fixed(5, 3)
            },
            fixed(1, 10),
        ],
        ..Container::default()
    };
    let mut opts = options(10, 4);
    opts.scroll_offsets
        .insert(node_key(&root, &[]), ScrollOffset { x: 0, y: 1 });
    opts.scroll_offsets.insert(
        node_key(&root.children[1], &[1]),
        ScrollOffset { x: 0, y: 99 },
    );
    let layout = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    assert_eq!(layout.at_path(&[1]).unwrap().scroll_offset.y, 7);
    assert_eq!(layout.at_path(&[1, 0]).unwrap().bounds.y, -6);
    assert_eq!(layout.at_path(&[1, 0]).unwrap().clip.y, 1);
}

#[test]
fn native_measurement_drives_media_and_control_geometry() {
    let root = Container {
        children: vec![Container {
            element: Element::Image {
                source: Some("x".into()),
                alt: None,
                fit: None,
                source_set: None,
                sizes: None,
                loading: None,
            },
            ..Container::default()
        }],
        ..Container::default()
    };
    let measurer =
        |_: &Container, constraints: Constraints| constraints.constrain(LogicalSize::new(7, 4));
    let layout = layout(&root, &options(20, 8), &measurer).unwrap();
    assert_eq!(
        layout.at_path(&[0]).unwrap().content_extents,
        LogicalSize::new(7, 4)
    );
    assert_eq!(layout.at_path(&[0]).unwrap().bounds.height, 4);
}

#[test]
fn tables_share_columns_and_preserve_rich_cell_subtrees() {
    let table = Container {
        element: Element::Table,
        children: vec![
            row(vec![
                cell(vec![text("first")], None, None),
                cell(
                    vec![Container {
                        element: Element::Button { r#type: None },
                        children: vec![text("go")],
                        ..Container::default()
                    }],
                    None,
                    None,
                ),
            ]),
            row(vec![
                cell(vec![text("x")], None, None),
                cell(vec![text("second")], None, None),
            ]),
        ],
        ..Container::default()
    };
    let layout = tree(&table, 20, 10);
    let first = layout.at_path(&[0, 1]).unwrap();
    let second = layout.at_path(&[1, 1]).unwrap();
    assert_eq!(first.bounds.x, second.bounds.x);
    assert_eq!(first.bounds.width, second.bounds.width);
    assert!(layout.at_path(&[0, 1, 0]).is_some());
    assert_eq!(
        layout.at_path(&[0, 1, 0]).unwrap().style.style,
        Style::default()
    );
}

#[test]
fn table_spans_reserve_slots_and_expand_row_heights() {
    let table = Container {
        element: Element::Table,
        children: vec![
            row(vec![
                cell(vec![fixed(2, 4)], Some(2), None),
                cell(vec![text("a")], None, Some(2)),
            ]),
            row(vec![
                cell(vec![text("b")], None, None),
                cell(vec![text("c")], None, None),
            ]),
        ],
        ..Container::default()
    };
    let layout = tree(&table, 12, 10);
    assert_eq!(layout.at_path(&[0, 0]).unwrap().bounds.height, 4);
    assert!(layout.at_path(&[1, 0]).unwrap().bounds.x > 0);
    assert_eq!(layout.at_path(&[0, 1]).unwrap().bounds.right(), 12);
}

#[test]
fn table_sections_preserve_source_paths() {
    let table = Container {
        element: Element::Table,
        children: vec![Container {
            element: Element::TBody,
            children: vec![row(vec![cell(vec![text("x")], None, None)])],
            ..Container::default()
        }],
        ..Container::default()
    };
    let layout = tree(&table, 12, 8);
    assert!(layout.at_path(&[0, 0, 0, 0]).is_some());
    assert_eq!(
        layout.at_path(&[0, 0]).unwrap().parent_key,
        Some("@0".into())
    );
}

#[test]
fn invalid_inputs_are_errors_not_panics() {
    let mut opts = options(10, 4);
    opts.metrics.cell_width = f32::NAN;
    assert_eq!(
        layout(&Container::default(), &opts, &DefaultLeafMeasure).unwrap_err(),
        LayoutError::InvalidMetrics
    );
    let root = Container {
        children: vec![
            Container {
                str_id: Some("x".into()),
                ..Container::default()
            },
            Container {
                str_id: Some("x".into()),
                ..Container::default()
            },
        ],
        ..Container::default()
    };
    assert_eq!(
        layout(&root, &options(10, 4), &DefaultLeafMeasure).unwrap_err(),
        LayoutError::DuplicateKey("x".into())
    );
    let table = Container {
        element: Element::Table,
        children: vec![row(vec![cell(vec![], Some(0), None)])],
        ..Container::default()
    };
    assert!(matches!(
        layout(&table, &options(10, 4), &DefaultLeafMeasure),
        Err(LayoutError::InvalidTableSpan { .. })
    ));
}

#[test]
fn hidden_nodes_do_not_take_flow_space_and_visibility_retains_geometry() {
    let root = Container {
        children: vec![
            Container {
                hidden: Some(true),
                ..fixed(1, 3)
            },
            Container {
                visibility: Some(Visibility::Hidden),
                ..fixed(2, 1)
            },
            fixed(1, 1),
        ],
        ..Container::default()
    };
    let layout = tree(&root, 20, 5);
    assert!(layout.at_path(&[0]).is_none());
    assert!(!layout.at_path(&[1]).unwrap().visible);
    assert_eq!(layout.at_path(&[2]).unwrap().bounds.y, 1);
}

#[test]
fn cumulative_percentage_quantization_conserves_shared_edges() {
    let child = Container {
        width: Some(Number::RealPercent(100.0 / 3.0)),
        ..fixed(0, 1)
    };
    let root = Container {
        direction: LayoutDirection::Row,
        children: vec![child.clone(), child.clone(), child],
        ..Container::default()
    };
    let layout = tree(&root, 10, 5);
    assert_eq!(layout.at_path(&[0]).unwrap().bounds.width, 3);
    assert_eq!(layout.at_path(&[1]).unwrap().bounds.width, 4);
    assert_eq!(layout.at_path(&[2]).unwrap().bounds.right(), 10);
}

#[test]
fn calculation_reuses_document_unit_expression_semantics() {
    use hyperchad_transformer::Calculation;
    let child = Container {
        width: Some(Number::Calc(Calculation::Subtract(
            Box::new(Calculation::Number(Box::new(Number::IntegerPercent(100)))),
            Box::new(Calculation::Number(Box::new(Number::Integer(16)))),
        ))),
        ..fixed(0, 1)
    };
    let root = Container {
        children: vec![child],
        ..Container::default()
    };
    assert_eq!(tree(&root, 20, 5).at_path(&[0]).unwrap().bounds.width, 18);
}

#[test]
fn column_wrap_and_flex_height_allocate_terminal_rows() {
    let child = Container {
        width: Some(Number::Integer(16)),
        height: Some(Number::Integer(32)),
        ..Container::default()
    };
    let root = Container {
        overflow_y: LayoutOverflow::Wrap { grid: false },
        children: vec![child.clone(), child.clone(), child],
        ..Container::default()
    };
    let layout = tree(&root, 10, 4);
    assert_eq!(layout.at_path(&[1]).unwrap().bounds.y, 2);
    assert_eq!(layout.at_path(&[2]).unwrap().bounds.x, 2);
    let child = Container {
        flex: Some(hyperchad_transformer::Flex::default()),
        ..Container::default()
    };
    let root = Container {
        children: vec![child.clone(), child],
        ..Container::default()
    };
    let layout = tree(&root, 10, 5);
    assert_eq!(
        layout.at_path(&[0]).unwrap().bounds.height + layout.at_path(&[1]).unwrap().bounds.height,
        5
    );
}

#[test]
fn positioned_percentages_resolve_against_actual_containing_block() {
    let root = Container {
        children: vec![Container {
            children: vec![Container {
                position: Some(Position::Absolute),
                width: Some(Number::IntegerPercent(50)),
                left: Some(Number::IntegerPercent(25)),
                ..fixed(0, 1)
            }],
            ..fixed(4, 3)
        }],
        ..Container::default()
    };
    let layout = tree(&root, 20, 8);
    let positioned = layout.at_path(&[0, 0]).unwrap();
    assert_eq!(positioned.bounds.width, 10);
    assert_eq!(positioned.bounds.x, 5);
}

#[test]
fn sticky_bottom_and_right_follow_scrollport_and_parent_boundary() {
    let root = Container {
        overflow_x: LayoutOverflow::Scroll,
        overflow_y: LayoutOverflow::Scroll,
        children: vec![
            fixed(20, 8),
            Container {
                position: Some(Position::Sticky),
                bottom: Some(Number::Integer(16)),
                right: Some(Number::Integer(8)),
                margin_left: Some(Number::Integer(12 * 8)),
                ..fixed(2, 1)
            },
            fixed(1, 4),
        ],
        ..Container::default()
    };
    let mut opts = options(10, 5);
    let before = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    assert_eq!(before.at_path(&[1]).unwrap().bounds.x, 7);
    assert_eq!(before.at_path(&[1]).unwrap().bounds.y, 3);
    opts.scroll_offsets
        .insert(node_key(&root, &[]), ScrollOffset { x: 4, y: 2 });
    let after = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    assert_eq!(after.at_path(&[1]).unwrap().bounds.x, 7);
    assert_eq!(after.at_path(&[1]).unwrap().bounds.y, 3);
    assert_eq!(sticky_axis(20, 2, 0, 10, 4, 4, None, Some(0)), 6);
    assert_eq!(sticky_axis(-10, 2, 0, 10, -3, 4, Some(0), None), -1);
    assert_eq!(sticky_axis(20, 2, 0, 4, 0, 30, Some(3), Some(3)), 3);
}

#[test]
fn absolute_descendants_expand_scroll_extents_through_static_ancestors() {
    let root = Container {
        overflow_x: LayoutOverflow::Scroll,
        overflow_y: LayoutOverflow::Scroll,
        children: vec![Container {
            children: vec![Container {
                position: Some(Position::Absolute),
                left: Some(Number::Integer(18 * 8)),
                top: Some(Number::Integer(12 * 16)),
                ..fixed(4, 3)
            }],
            ..fixed(2, 1)
        }],
        ..Container::default()
    };
    let mut opts = options(10, 5);
    opts.scroll_offsets
        .insert(node_key(&root, &[]), ScrollOffset { x: 100, y: 100 });
    let scene = layout(&root, &opts, &DefaultLeafMeasure).unwrap();
    let root_node = scene.at_path(&[]).unwrap();
    assert_eq!(root_node.content_extents, LogicalSize::new(22, 15));
    assert_eq!(root_node.scroll_offset, ScrollOffset { x: 12, y: 10 });
    assert_eq!(
        scene.at_path(&[0, 0]).unwrap().bounds,
        CellRect::new(6, 2, 4, 3)
    );
    let mut reset = opts.clone();
    reset.scroll_offsets.clear();
    let unscrolled = layout(&root, &reset, &DefaultLeafMeasure).unwrap();
    assert_eq!(
        unscrolled.at_path(&[]).unwrap().content_extents,
        root_node.content_extents
    );
}

#[test]
fn fixed_overflow_is_excluded_and_clipped_absolute_overflow_stops_per_axis() {
    let root = Container {
        overflow_x: LayoutOverflow::Scroll,
        overflow_y: LayoutOverflow::Scroll,
        children: vec![
            Container {
                position: Some(Position::Relative),
                overflow_x: LayoutOverflow::Hidden,
                overflow_y: LayoutOverflow::Scroll,
                children: vec![Container {
                    position: Some(Position::Absolute),
                    left: Some(Number::Integer(30 * 8)),
                    top: Some(Number::Integer(30 * 16)),
                    ..fixed(2, 2)
                }],
                ..fixed(4, 3)
            },
            Container {
                position: Some(Position::Fixed),
                left: Some(Number::Integer(100 * 8)),
                top: Some(Number::Integer(100 * 16)),
                ..fixed(4, 4)
            },
        ],
        ..Container::default()
    };
    let scene = tree(&root, 10, 5);
    assert_eq!(
        scene.at_path(&[0]).unwrap().content_extents,
        LogicalSize::new(32, 32)
    );
    assert_eq!(
        scene.at_path(&[]).unwrap().content_extents,
        LogicalSize::new(4, 3)
    );
}

#[test]
fn expanded_tabs_and_justified_spaces_match_fragment_bounds() {
    let root = Container {
        white_space: Some(WhiteSpace::Preserve),
        children: vec![text("a\tb")],
        ..Container::default()
    };
    let scene = tree(&root, 20, 5);
    for fragment in &scene.nodes[0].text {
        assert_eq!(
            bmux_tui::text_width::display_width(&fragment.text) as u64,
            fragment.bounds.width
        );
    }
    let root = Container {
        text_align: Some(TextAlign::Justify),
        children: vec![text("a b c d")],
        ..Container::default()
    };
    let scene = tree(&root, 6, 5);
    for fragment in &scene.nodes[0].text {
        assert_eq!(
            bmux_tui::text_width::display_width(&fragment.text) as u64,
            fragment.bounds.width
        );
    }
}

#[test]
fn zero_width_has_finite_geometry() {
    let root = Container {
        overflow_wrap: Some(OverflowWrap::Anywhere),
        children: vec![text("界a")],
        ..Container::default()
    };
    let layout = tree(&root, 0, 0);
    assert_eq!(layout.nodes[0].clip.width, 0);
    assert!(layout.hit_test(0, 0).is_none());
}
