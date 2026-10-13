use super::*;
use crate::schema::write_guard::{
    GuardPredicate, GuardScalar, ObservationColumn, ObservationEvent,
};
fn observation() -> WriteObservation {
    let lookup = UniqueScalarLookup {
        table: "mapping".into(),
        key_column: "identity".into(),
        key: ObservationLookupKey::NewColumn("identity".into()),
        value_column: "run".into(),
        max_bytes: 8,
    };
    WriteObservation {
        name: "lookup_test".into(),
        table: "source".into(),
        event: ObservationEvent::AfterInsert,
        when: GuardPredicate::Equal(
            GuardValue::Constant(GuardScalar::Integer(1)),
            GuardValue::Constant(GuardScalar::Integer(1)),
        ),
        target_table: "observations".into(),
        columns: vec![
            ObservationColumn {
                column: "run".into(),
                value: GuardValue::UniqueScalarLookup(lookup.clone()),
            },
            ObservationColumn {
                column: "status".into(),
                value: GuardValue::UniqueScalarLookupStatus(lookup),
            },
        ],
    }
}
#[test]
fn unique_lookup_retains_status_and_shares_source_rollback() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE mapping(identity TEXT PRIMARY KEY,run); CREATE TABLE source(identity TEXT); CREATE TABLE observations(run,status INTEGER); BEGIN;").unwrap();
    super::super::install_write_observation_on_connection(&connection, &observation()).unwrap();
    connection.execute_batch("INSERT INTO mapping VALUES ('ok','run'),('null',NULL),('large','123456789'),('type',42); INSERT INTO source VALUES ('ok'),('missing'),('null'),('large'),('type');").unwrap();
    let mut stmt = connection
        .prepare("SELECT run,status FROM observations ORDER BY rowid")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            Ok((row.get::<_, Option<String>>(0)?, row.get::<_, i64>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        rows,
        vec![
            (Some("run".into()), 1),
            (None, 0),
            (None, 3),
            (None, 4),
            (None, 5)
        ]
    );
    drop(stmt);
    connection.execute_batch("ROLLBACK;").unwrap();
    assert!(
        !super::super::verify_write_observation_on_connection(&connection, &observation()).unwrap()
    );
}
#[test]
fn lookup_rejects_nonunique_partial_and_collation_indexes() {
    for ddl in [
        "CREATE TABLE mapping(identity TEXT,run)",
        "CREATE TABLE mapping(identity TEXT,run); CREATE UNIQUE INDEX partial ON mapping(identity) WHERE identity IS NOT NULL",
        "CREATE TABLE mapping(identity TEXT COLLATE NOCASE PRIMARY KEY,run)",
    ] {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch(ddl).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE source(identity TEXT); CREATE TABLE observations(run,status); BEGIN",
            )
            .unwrap();
        assert!(
            super::super::install_write_observation_on_connection(&connection, &observation())
                .is_err()
        );
    }
}
#[test]
fn corrupted_mapping_cardinality_does_not_choose_first_match() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE mapping(identity TEXT,run); CREATE UNIQUE INDEX mapping_key ON mapping(identity); CREATE TABLE source(identity TEXT); CREATE TABLE observations(run,status); BEGIN").unwrap();
    super::super::install_write_observation_on_connection(&connection, &observation()).unwrap();
    connection.execute_batch("DROP INDEX mapping_key; INSERT INTO mapping VALUES ('ambiguous','one'),('ambiguous','two'); INSERT INTO source VALUES ('ambiguous')").unwrap();
    let result: (Option<String>, i64) = connection
        .query_row("SELECT run,status FROM observations", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })
        .unwrap();
    assert_eq!(result, (None, 2));
    assert!(
        super::super::verify_write_observation_on_connection(&connection, &observation()).is_err()
    );
}
#[test]
fn persisted_lookup_reopens_verifies_duplicates_and_old_delete() {
    let path = std::env::temp_dir().join(format!(
        "lookup-{}-{}.sqlite",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    {
        let connection = Connection::open(&path).unwrap();
        connection.execute_batch("CREATE TABLE mapping(identity TEXT PRIMARY KEY,run); CREATE TABLE source(identity TEXT); CREATE TABLE observations(run,status); BEGIN").unwrap();
        let definition = observation();
        super::super::install_write_observation_on_connection(&connection, &definition).unwrap();
        super::super::install_write_observation_on_connection(&connection, &definition).unwrap();
        let mut conflict = definition.clone();
        if let GuardValue::UniqueScalarLookup(selector) = &mut conflict.columns[0].value {
            selector.max_bytes = 7;
        }
        assert!(
            super::super::install_write_observation_on_connection(&connection, &conflict).is_err()
        );
        assert!(
            super::super::verify_write_observation_on_connection(&connection, &definition).unwrap()
        );
        let mut deleted = definition;
        deleted.name = "lookup_delete".into();
        deleted.event = ObservationEvent::AfterDelete;
        for column in &mut deleted.columns {
            match &mut column.value {
                GuardValue::UniqueScalarLookup(selector)
                | GuardValue::UniqueScalarLookupStatus(selector) => {
                    selector.key = ObservationLookupKey::OldColumn("identity".into());
                }
                _ => unreachable!(),
            }
        }
        super::super::install_write_observation_on_connection(&connection, &deleted).unwrap();
        connection
            .execute_batch("INSERT INTO mapping VALUES ('ok','run'); COMMIT; BEGIN")
            .unwrap();
        for _ in 0..105 {
            connection
                .execute("INSERT INTO source VALUES ('ok')", [])
                .unwrap();
        }
        connection
            .execute_batch("DELETE FROM source; COMMIT")
            .unwrap();
    }
    let connection = Connection::open(&path).unwrap();
    assert!(
        super::super::verify_write_observation_on_connection(&connection, &observation()).unwrap()
    );
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM observations WHERE run='run' AND status=1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 210);
    drop(connection);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn nullable_keys_and_value_bounds_are_truthful() {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("CREATE TABLE mapping(identity TEXT UNIQUE,run); CREATE TABLE source(identity); CREATE TABLE observations(run,status); BEGIN").unwrap();
    super::super::install_write_observation_on_connection(&connection, &observation()).unwrap();
    connection.execute_batch("INSERT INTO mapping VALUES (NULL,'ignored'),('exact','12345678'); INSERT INTO source VALUES (NULL),('exact'),(X'ff');").unwrap();
    let mut statement = connection
        .prepare("SELECT status FROM observations ORDER BY rowid")
        .unwrap();
    let statuses = statement
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(statuses, vec![0, 1, 5]);
}

#[test]
fn lookup_rejects_unavailable_row_side_and_unbounded_selectors() {
    let mut selector = UniqueScalarLookup {
        table: "mapping".into(),
        key_column: "identity".into(),
        key: ObservationLookupKey::OldColumn("identity".into()),
        value_column: "run".into(),
        max_bytes: 8,
    };
    assert!(render(&selector, GuardEvent::BeforeInsert, false).is_err());
    selector.max_bytes = 0;
    assert!(render(&selector, GuardEvent::BeforeUpdate, false).is_err());
}
