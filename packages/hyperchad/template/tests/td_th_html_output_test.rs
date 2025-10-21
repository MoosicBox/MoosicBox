use hyperchad_renderer_html::{DefaultHtmlTagRenderer, html::elements_to_html};
use hyperchad_template::container;

fn render_to_html(containers: &[hyperchad_transformer::Container]) -> String {
    let mut buf = Vec::new();
    let renderer = DefaultHtmlTagRenderer::default();
    elements_to_html(&mut buf, containers, &renderer, false).unwrap();
    String::from_utf8(buf).unwrap()
}

#[test]
fn test_td_rows_renders_rowspan() {
    let containers = container! {
        table {
            tr {
                td rows="2" { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"2\""));
}

#[test]
fn test_td_columns_renders_colspan() {
    let containers = container! {
        table {
            tr {
                td columns="3" { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("colspan=\"3\""));
}

#[test]
fn test_td_rows_and_columns_renders_both() {
    let containers = container! {
        table {
            tr {
                td rows="2" columns="3" { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"2\""));
    assert!(html.contains("colspan=\"3\""));
}

#[test]
fn test_th_rows_renders_rowspan() {
    let containers = container! {
        table {
            thead {
                tr {
                    th rows="2" { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"2\""));
}

#[test]
fn test_th_columns_renders_colspan() {
    let containers = container! {
        table {
            thead {
                tr {
                    th columns="3" { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("colspan=\"3\""));
}

#[test]
fn test_th_rows_and_columns_renders_both() {
    let containers = container! {
        table {
            thead {
                tr {
                    th rows="2" columns="3" { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"2\""));
    assert!(html.contains("colspan=\"3\""));
}

#[test]
fn test_td_no_span_no_attrs() {
    let containers = container! {
        table {
            tr {
                td { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(!html.contains("rowspan"));
    assert!(!html.contains("colspan"));
}

#[test]
fn test_th_no_span_no_attrs() {
    let containers = container! {
        table {
            thead {
                tr {
                    th { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(!html.contains("rowspan"));
    assert!(!html.contains("colspan"));
}

#[test]
fn test_mixed_table_with_spans() {
    let containers = container! {
        table {
            thead {
                tr {
                    th columns="2" { "Header" }
                    th { "Normal Header" }
                }
            }
            tbody {
                tr {
                    td rows="2" { "Row Header" }
                    td { "Data 1" }
                    td { "Data 2" }
                }
                tr {
                    td { "Data 3" }
                    td { "Data 4" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("colspan=\"2\""));
    assert!(html.contains("rowspan=\"2\""));
}

#[test]
fn test_td_unquoted_rows_renders_rowspan() {
    let containers = container! {
        table {
            tr {
                td rows=2 { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"2\""));
}

#[test]
fn test_td_unquoted_columns_renders_colspan() {
    let containers = container! {
        table {
            tr {
                td columns=3 { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("colspan=\"3\""));
}

#[test]
fn test_th_unquoted_rows_renders_rowspan() {
    let containers = container! {
        table {
            thead {
                tr {
                    th rows=2 { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"2\""));
}

#[test]
fn test_th_unquoted_columns_renders_colspan() {
    let containers = container! {
        table {
            thead {
                tr {
                    th columns=3 { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("colspan=\"3\""));
}

#[test]
fn test_td_with_variables_renders_correctly() {
    let row_count = 4;
    let col_count = 5;

    let containers = container! {
        table {
            tr {
                td rows=(row_count) columns=(col_count) { "Cell" }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"4\""));
    assert!(html.contains("colspan=\"5\""));
}

#[test]
fn test_th_with_variables_renders_correctly() {
    let row_count = 3;
    let col_count = 2;

    let containers = container! {
        table {
            thead {
                tr {
                    th rows=(row_count) columns=(col_count) { "Header" }
                }
            }
        }
    };
    let html = render_to_html(&containers);
    assert!(html.contains("rowspan=\"3\""));
    assert!(html.contains("colspan=\"2\""));
}
