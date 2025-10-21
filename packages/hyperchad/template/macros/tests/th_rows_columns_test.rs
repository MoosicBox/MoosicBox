#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template_macros::container;
use hyperchad_transformer::{Element, Number};

#[test]
fn test_th_no_span() {
    let result = container! {
        table {
            thead {
                tr {
                    th { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_rows() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows="2" { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_columns() {
    let result = container! {
        table {
            thead {
                tr {
                    th columns="3" { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_rows_and_columns() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows="2" columns="3" { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_dynamic_rows() {
    let row_count = 4;
    let result = container! {
        table {
            thead {
                tr {
                    th rows=(row_count.to_string()) { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, .. } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(4)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_dynamic_columns() {
    let col_count = 5;
    let result = container! {
        table {
            thead {
                tr {
                    th columns=(col_count.to_string()) { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { columns, .. } = &th.element {
        assert_eq!(columns, &Some(Number::Integer(5)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_multiple_headers_different_spans() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows="2" { "Header 1" }
                    th columns="3" { "Header 2" }
                    th rows="2" columns="2" { "Header 3" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];

    if let Element::TH { rows, columns } = &tr.children[0].element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TH element");
    }

    if let Element::TH { rows, columns } = &tr.children[1].element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TH element");
    }

    if let Element::TH { rows, columns } = &tr.children[2].element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(2)));
    } else {
        panic!("Expected TH element");
    }
}

#[test]
fn test_th_with_nested_content() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows="2" columns="2" {
                        span { "Nested header" }
                    }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(2)));
        assert_eq!(th.children.len(), 1);
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_other_attributes() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows="2" padding=10 background="blue" font-weight="bold" { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, .. } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert!(th.padding_top.is_some());
        assert!(th.background.is_some());
        assert!(th.font_weight.is_some());
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_mixed_with_td() {
    let result = container! {
        table {
            tr {
                th rows="2" { "Row Header" }
                td columns="3" { "Data Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];

    if let Element::TH { rows, columns } = &tr.children[0].element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TH element");
    }

    if let Element::TD { rows, columns } = &tr.children[1].element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TD element");
    }
}

#[test]
fn test_th_with_unquoted_numeric_rows() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows=2 { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_unquoted_numeric_columns() {
    let result = container! {
        table {
            thead {
                tr {
                    th columns=3 { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_unquoted_numeric_both() {
    let result = container! {
        table {
            thead {
                tr {
                    th rows=2 columns=3 { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}

#[test]
fn test_th_with_variable_values() {
    let row_span = 4;
    let col_span = 5;

    let result = container! {
        table {
            thead {
                tr {
                    th rows=(row_span) columns=(col_span) { "Header" }
                }
            }
        }
    };

    let table = &result[0];
    let thead = &table.children[0];
    let tr = &thead.children[0];
    let th = &tr.children[0];

    if let Element::TH { rows, columns } = &th.element {
        assert_eq!(rows, &Some(Number::Integer(4)));
        assert_eq!(columns, &Some(Number::Integer(5)));
    } else {
        panic!("Expected TH element, got: {:?}", th.element);
    }
}
