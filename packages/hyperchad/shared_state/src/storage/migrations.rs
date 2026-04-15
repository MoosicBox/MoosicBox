use switchy_database::{
    Database,
    schema::{Column, DataType, create_index, create_table, drop_index, drop_table},
};
use switchy_schema::{discovery::code::CodeMigrationSource, runner::MigrationRunner};

use crate::SharedStateError;

#[allow(clippy::too_many_lines)]
#[must_use]
pub fn shared_state_migrations() -> CodeMigrationSource<'static> {
    let mut source = CodeMigrationSource::new();

    source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
        "001_create_shared_state_channels".to_string(),
        Box::new(
            create_table("shared_state_channels")
                .column(Column {
                    name: "channel_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "revision".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .column(Column {
                    name: "updated_at_ms".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .primary_key("channel_id"),
        ),
        Some(Box::new(
            drop_table("shared_state_channels").if_exists(true),
        )),
    ));

    source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
        "002_create_shared_state_events".to_string(),
        Box::new(
            create_table("shared_state_events")
                .column(Column {
                    name: "channel_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "revision".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .column(Column {
                    name: "event_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "command_id".to_string(),
                    nullable: true,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "event_name".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "payload_data".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "payload_format".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "payload_storage".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "metadata_data".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "metadata_format".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "metadata_storage".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "created_at_ms".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                }),
        ),
        Some(Box::new(drop_table("shared_state_events").if_exists(true))),
    ));

    source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
        "003_create_shared_state_events_indexes".to_string(),
        Box::new(
            create_index("idx_shared_state_events_channel_revision")
                .table("shared_state_events")
                .columns(vec!["channel_id", "revision"])
                .unique(true),
        ),
        Some(Box::new(
            drop_index(
                "idx_shared_state_events_channel_revision",
                "shared_state_events",
            )
            .if_exists(),
        )),
    ));

    source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
        "004_create_shared_state_snapshots".to_string(),
        Box::new(
            create_table("shared_state_snapshots")
                .column(Column {
                    name: "channel_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "revision".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .column(Column {
                    name: "payload_data".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "payload_format".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "payload_storage".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "created_at_ms".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                }),
        ),
        Some(Box::new(
            drop_table("shared_state_snapshots").if_exists(true),
        )),
    ));

    source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
        "005_create_shared_state_commands".to_string(),
        Box::new(
            create_table("shared_state_commands")
                .column(Column {
                    name: "command_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "channel_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "idempotency_key".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "participant_id".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "expected_revision".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .column(Column {
                    name: "command_name".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "payload_data".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "payload_format".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "payload_storage".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "metadata_data".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "metadata_format".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "metadata_storage".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::SmallInt,
                    default: None,
                })
                .column(Column {
                    name: "status".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::VarChar(32),
                    default: None,
                })
                .column(Column {
                    name: "resulting_revision".to_string(),
                    nullable: true,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .column(Column {
                    name: "error_reason".to_string(),
                    nullable: true,
                    auto_increment: false,
                    data_type: DataType::Text,
                    default: None,
                })
                .column(Column {
                    name: "created_at_ms".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .column(Column {
                    name: "updated_at_ms".to_string(),
                    nullable: false,
                    auto_increment: false,
                    data_type: DataType::BigInt,
                    default: None,
                })
                .primary_key("command_id"),
        ),
        Some(Box::new(
            drop_table("shared_state_commands").if_exists(true),
        )),
    ));

    source.add_migration(switchy_schema::discovery::code::CodeMigration::new(
        "006_create_shared_state_commands_indexes".to_string(),
        Box::new(
            create_index("idx_shared_state_commands_channel_idempotency")
                .table("shared_state_commands")
                .columns(vec!["channel_id", "idempotency_key"])
                .unique(true),
        ),
        Some(Box::new(
            drop_index(
                "idx_shared_state_commands_channel_idempotency",
                "shared_state_commands",
            )
            .if_exists(),
        )),
    ));

    source
}

/// # Errors
///
/// * [`SharedStateError::Migration`] - If migration execution fails
pub async fn migrate_shared_state(db: &dyn Database) -> Result<(), SharedStateError> {
    let runner = MigrationRunner::new(Box::new(shared_state_migrations()))
        .with_table_name("__hyperchad_shared_state_migrations");
    runner.run(db).await?;
    Ok(())
}
