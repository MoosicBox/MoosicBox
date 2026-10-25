use clippier::matrix_schedule::{Receipt, schedule, verify};
use serde_json::{Map, Value, json};

fn case(index: usize, package: usize) -> Map<String, Value> {
    json!({"name": format!("package-{package}"), "path": format!("packages/{package}"),
        "os": "ubuntu-latest", "features": [format!("feature-{index}")],
        "requiredFeatures": "required", "env": "FLAG=value"})
    .as_object()
    .unwrap()
    .clone()
}

#[test]
fn preserves_exact_multiset_and_balances_at_boundaries() {
    for size in [0, 1, 256, 257, 259, 513] {
        let input = (0..size).map(|i| case(i, i % 3)).collect::<Vec<_>>();
        let jobs = schedule(input.clone(), 256).unwrap();
        assert_eq!(jobs.len(), size.min(256));
        let mut flattened = jobs
            .iter()
            .flat_map(|j| &j.cases)
            .map(|c| c.package.clone())
            .collect::<Vec<_>>();
        let mut expected = input.clone();
        flattened.sort_by_cached_key(|p| serde_json::to_string(p).unwrap());
        expected.sort_by_cached_key(|p| serde_json::to_string(p).unwrap());
        assert_eq!(flattened, expected, "size {size}");
        for package in 0..3 {
            let lengths = jobs
                .iter()
                .filter(|j| j.setup["name"] == format!("package-{package}"))
                .map(|j| j.cases.len())
                .collect::<Vec<_>>();
            if !lengths.is_empty() {
                assert!(lengths.iter().max().unwrap() - lengths.iter().min().unwrap() <= 1);
            }
        }
        let mut reversed = input;
        reversed.reverse();
        assert_eq!(
            serde_json::to_value(&jobs).unwrap(),
            serde_json::to_value(schedule(reversed, 256).unwrap()).unwrap()
        );
        let receipts = jobs
            .iter()
            .flat_map(|j| &j.cases)
            .map(|case| Receipt {
                case: case.clone(),
                exit_code: 0,
            })
            .collect::<Vec<_>>();
        verify(&jobs, &receipts).unwrap();
        if !receipts.is_empty() {
            assert!(verify(&jobs, &receipts[1..]).is_err());
        }
    }
}

#[test]
fn never_unions_empty_or_enabled_feature_configurations() {
    let mut empty = case(0, 0);
    empty.insert("features".into(), json!([]));
    let jobs = schedule(vec![empty.clone(), case(1, 0), case(1, 0)], 1).unwrap();
    assert_eq!(jobs[0].cases.len(), 3);
    assert!(jobs[0].cases.iter().any(|c| c.package == empty));
    assert!(!jobs[0].setup.contains_key("features"));
    let ids = jobs[0]
        .cases
        .iter()
        .map(|c| &c.id)
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(ids.len(), 3);
}

#[test]
fn refuses_incompatible_or_fresh_runner_setups() {
    for (field, value) in [
        ("os", json!("windows-latest")),
        ("env", json!("OTHER=1")),
        ("toolchains", json!("node")),
        ("nightly", json!(true)),
        ("ciSteps", json!("touch state")),
        ("dependencies", json!("install something")),
        ("freshRunner", json!(true)),
        ("gitSubmodules", json!(true)),
    ] {
        let mut changed = case(0, 0);
        changed.insert(field.into(), value);
        assert!(
            schedule(vec![case(1, 0), changed.clone()], 1).is_err(),
            "{field}"
        );
        if ["ciSteps", "dependencies", "freshRunner", "gitSubmodules"].contains(&field) {
            assert!(
                schedule(vec![changed.clone(), changed], 1).is_err(),
                "{field}"
            );
        }
    }
    assert!(schedule(vec![], 0).is_err());
    let mut invalid = case(0, 0);
    invalid.insert("features".into(), json!(null));
    assert!(schedule(vec![invalid], 1).is_err());
}

#[test]
fn rejects_changed_duplicate_unexpected_and_failed_receipts() {
    let jobs = schedule(vec![case(0, 0)], 1).unwrap();
    let original = jobs[0].cases[0].clone();
    for mutate in [0, 1, 2] {
        let mut receipt = Receipt {
            case: original.clone(),
            exit_code: 0,
        };
        match mutate {
            0 => receipt
                .case
                .package
                .insert("features".into(), json!(["changed"])),
            1 => {
                receipt.case.id = "unexpected".into();
                None
            }
            _ => {
                receipt.exit_code = 7;
                None
            }
        };
        assert!(verify(&jobs, &[receipt]).is_err());
    }
    assert!(
        verify(
            &jobs,
            &[
                Receipt {
                    case: original.clone(),
                    exit_code: 0
                },
                Receipt {
                    case: original,
                    exit_code: 0
                }
            ]
        )
        .is_err()
    );
}
