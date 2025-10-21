#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]

use hyperchad_template_macros::container;
use hyperchad_transformer::{Element, Number};

#[test]
fn test_td_no_span() {
    let result = container! {
        table {
            tr {
                td { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_rows() {
    let result = container! {
        table {
            tr {
                td rows="2" { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_columns() {
    let result = container! {
        table {
            tr {
                td columns="3" { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_rows_and_columns() {
    let result = container! {
        table {
            tr {
                td rows="2" columns="3" { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_dynamic_rows() {
    let row_count = 4;
    let result = container! {
        table {
            tr {
                td rows=(row_count.to_string()) { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, .. } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(4)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_dynamic_columns() {
    let col_count = 5;
    let result = container! {
        table {
            tr {
                td columns=(col_count.to_string()) { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { columns, .. } = &td.element {
        assert_eq!(columns, &Some(Number::Integer(5)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_multiple_cells_different_spans() {
    let result = container! {
        table {
            tr {
                td rows="2" { "Cell 1" }
                td columns="3" { "Cell 2" }
                td rows="2" columns="2" { "Cell 3" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];

    if let Element::TD { rows, columns } = &tr.children[0].element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TD element");
    }

    if let Element::TD { rows, columns } = &tr.children[1].element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TD element");
    }

    if let Element::TD { rows, columns } = &tr.children[2].element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(2)));
    } else {
        panic!("Expected TD element");
    }
}

#[test]
fn test_td_with_nested_content() {
    let result = container! {
        table {
            tr {
                td rows="2" columns="2" {
                    div { "Nested content" }
                }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(2)));
        assert_eq!(td.children.len(), 1);
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_other_attributes() {
    let result = container! {
        table {
            tr {
                td rows="2" padding=10 background="blue" { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, .. } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert!(td.padding_top.is_some());
        assert!(td.background.is_some());
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_unquoted_numeric_rows() {
    let result = container! {
        table {
            tr {
                td rows=2 { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &None);
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_unquoted_numeric_columns() {
    let result = container! {
        table {
            tr {
                td columns=3 { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &None);
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_unquoted_numeric_both() {
    let result = container! {
        table {
            tr {
                td rows=2 columns=3 { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(2)));
        assert_eq!(columns, &Some(Number::Integer(3)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}

#[test]
fn test_td_with_variable_values() {
    let row_span = 4;
    let col_span = 5;

    let result = container! {
        table {
            tr {
                td rows=(row_span) columns=(col_span) { "Cell" }
            }
        }
    };

    let table = &result[0];
    let tr = &table.children[0];
    let td = &tr.children[0];

    if let Element::TD { rows, columns } = &td.element {
        assert_eq!(rows, &Some(Number::Integer(4)));
        assert_eq!(columns, &Some(Number::Integer(5)));
    } else {
        panic!("Expected TD element, got: {:?}", td.element);
    }
}
