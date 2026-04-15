use std::{collections::BTreeMap, sync::Arc, time::SystemTime};

use async_trait::async_trait;
use hyperchad_shared_state_models::{
    ChannelId, CommandEnvelope, CommandId, EventEnvelope, EventId, IdempotencyKey, PayloadBlob,
    PayloadFormat, PayloadStorage, Revision, SnapshotEnvelope,
};
use switchy_database::{Database, DatabaseValue, query::FilterableQuery as _};

use crate::{
    SharedStateError,
    traits::{
        AppendEventsResult, BeginCommandResult, CommandStore, EventDraft, EventStore, SnapshotStore,
    },
};

#[derive(Debug, Clone)]
pub struct SwitchySharedStateStore {
    db: Arc<Box<dyn Database>>,
}

impl SwitchySharedStateStore {
    #[must_use]
    pub fn new(db: Arc<Box<dyn Database>>) -> Self {
        Self { db }
    }

    #[must_use]
    pub fn from_box(db: Box<dyn Database>) -> Self {
        Self { db: Arc::new(db) }
    }

    #[must_use]
    pub fn database(&self) -> &dyn Database {
        &**self.db
    }

    fn now_unix_ms() -> Result<i64, SharedStateError> {
        let duration = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_err(|e| SharedStateError::Conversion(format!("Invalid system time: {e}")))?;

        i64::try_from(duration.as_millis())
            .map_err(|e| SharedStateError::Conversion(format!("Timestamp overflow: {e}")))
    }

    fn i64_from_revision(revision: Revision) -> Result<i64, SharedStateError> {
        i64::try_from(revision.value())
            .map_err(|e| SharedStateError::Conversion(format!("Revision overflow: {e}")))
    }

    fn revision_from_i64(value: i64) -> Result<Revision, SharedStateError> {
        let value = u64::try_from(value)
            .map_err(|e| SharedStateError::Conversion(format!("Invalid revision value: {e}")))?;
        Ok(Revision::new(value))
    }

    fn required_string(
        row: &switchy_database::Row,
        column: &str,
    ) -> Result<String, SharedStateError> {
        row.get(column)
            .and_then(|x| x.as_str().map(ToOwned::to_owned))
            .ok_or_else(|| {
                SharedStateError::Conversion(format!("Missing or invalid '{column}' column"))
            })
    }

    fn required_i64(row: &switchy_database::Row, column: &str) -> Result<i64, SharedStateError> {
        row.get(column).and_then(|x| x.as_i64()).ok_or_else(|| {
            SharedStateError::Conversion(format!("Missing or invalid '{column}' column"))
        })
    }

    fn optional_string(row: &switchy_database::Row, column: &str) -> Option<String> {
        row.get(column)
            .and_then(|x| x.as_str().map(ToOwned::to_owned))
    }

    fn encode_metadata(
        metadata: &BTreeMap<String, String>,
    ) -> Result<PayloadBlob, SharedStateError> {
        Ok(PayloadBlob::from_serializable(metadata)?)
    }

    fn decode_metadata(
        data: String,
        format: i16,
        storage: i16,
    ) -> Result<BTreeMap<String, String>, SharedStateError> {
        let payload = PayloadBlob {
            data,
            format: PayloadFormat::try_from_i16(format)?,
            storage: PayloadStorage::try_from_i16(storage)?,
        };

        payload.deserialize().map_err(SharedStateError::from)
    }

    fn map_event_row(row: &switchy_database::Row) -> Result<EventEnvelope, SharedStateError> {
        let event_id = EventId::new(Self::required_string(row, "event_id")?);
        let channel_id = ChannelId::new(Self::required_string(row, "channel_id")?);
        let revision = Self::revision_from_i64(Self::required_i64(row, "revision")?)?;
        let command_id = Self::optional_string(row, "command_id").map(CommandId::new);
        let event_name = Self::required_string(row, "event_name")?;
        let payload = PayloadBlob {
            data: Self::required_string(row, "payload_data")?,
            format: PayloadFormat::try_from_i16(
                i16::try_from(Self::required_i64(row, "payload_format")?).map_err(|e| {
                    SharedStateError::Conversion(format!("Invalid payload_format value: {e}"))
                })?,
            )?,
            storage: PayloadStorage::try_from_i16(
                i16::try_from(Self::required_i64(row, "payload_storage")?).map_err(|e| {
                    SharedStateError::Conversion(format!("Invalid payload_storage value: {e}"))
                })?,
            )?,
        };
        let metadata = Self::decode_metadata(
            Self::required_string(row, "metadata_data")?,
            i16::try_from(Self::required_i64(row, "metadata_format")?).map_err(|e| {
                SharedStateError::Conversion(format!("Invalid metadata_format value: {e}"))
            })?,
            i16::try_from(Self::required_i64(row, "metadata_storage")?).map_err(|e| {
                SharedStateError::Conversion(format!("Invalid metadata_storage value: {e}"))
            })?,
        )?;
        let created_at_ms = Self::required_i64(row, "created_at_ms")?;

        Ok(EventEnvelope {
            event_id,
            channel_id,
            revision,
            command_id,
            event_name,
            payload,
            metadata,
            created_at_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommandStatus {
    Pending,
    Applied,
    Rejected,
}

impl CommandStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Applied => "APPLIED",
            Self::Rejected => "REJECTED",
        }
    }

    fn parse(value: &str) -> Result<Self, SharedStateError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "APPLIED" => Ok(Self::Applied),
            "REJECTED" => Ok(Self::Rejected),
            _ => Err(SharedStateError::Conversion(format!(
                "Unknown command status '{value}'"
            ))),
        }
    }
}

#[async_trait]
impl CommandStore for SwitchySharedStateStore {
    #[allow(clippy::too_many_lines)]
    async fn begin_command(
        &self,
        command: &CommandEnvelope,
    ) -> Result<BeginCommandResult, SharedStateError> {
        if let Some(existing) = self
            .db
            .select("shared_state_commands")
            .columns(&["command_id", "status", "resulting_revision", "error_reason"])
            .where_eq("channel_id", command.channel_id.as_str())
            .where_eq("idempotency_key", command.idempotency_key.as_str())
            .execute_first(&**self.db)
            .await?
        {
            let existing_command_id =
                CommandId::new(Self::required_string(&existing, "command_id")?);
            let status = CommandStatus::parse(&Self::required_string(&existing, "status")?)?;

            return match status {
                CommandStatus::Pending => Ok(BeginCommandResult::DuplicateRejected {
                    command_id: existing_command_id,
                    reason: "Command with idempotency key already pending".to_string(),
                }),
                CommandStatus::Applied => Ok(BeginCommandResult::DuplicateApplied {
                    command_id: existing_command_id,
                    resulting_revision: Self::revision_from_i64(Self::required_i64(
                        &existing,
                        "resulting_revision",
                    )?)?,
                }),
                CommandStatus::Rejected => Ok(BeginCommandResult::DuplicateRejected {
                    command_id: existing_command_id,
                    reason: Self::optional_string(&existing, "error_reason")
                        .unwrap_or_else(|| "Command already rejected".to_string()),
                }),
            };
        }

        let payload = &command.payload;
        let metadata_payload = Self::encode_metadata(&command.metadata)?;

        match self
            .db
            .insert("shared_state_commands")
            .value("command_id", command.command_id.as_str())
            .value("channel_id", command.channel_id.as_str())
            .value("idempotency_key", command.idempotency_key.as_str())
            .value("participant_id", command.participant_id.as_str())
            .value(
                "expected_revision",
                Self::i64_from_revision(command.expected_revision)?,
            )
            .value("command_name", command.command_name.as_str())
            .value("payload_data", payload.data.as_str())
            .value("payload_format", i64::from(payload.format.as_i16()))
            .value("payload_storage", i64::from(payload.storage.as_i16()))
            .value("metadata_data", metadata_payload.data.as_str())
            .value(
                "metadata_format",
                i64::from(metadata_payload.format.as_i16()),
            )
            .value(
                "metadata_storage",
                i64::from(metadata_payload.storage.as_i16()),
            )
            .value("status", CommandStatus::Pending.as_str())
            .value("resulting_revision", DatabaseValue::Null)
            .value("error_reason", DatabaseValue::Null)
            .value("created_at_ms", command.created_at_ms)
            .value("updated_at_ms", command.created_at_ms)
            .execute(&**self.db)
            .await
        {
            Ok(_row) => Ok(BeginCommandResult::New),
            Err(insert_error) => {
                if let Some(existing) = self
                    .db
                    .select("shared_state_commands")
                    .columns(&["command_id", "status", "resulting_revision", "error_reason"])
                    .where_eq("channel_id", command.channel_id.as_str())
                    .where_eq("idempotency_key", command.idempotency_key.as_str())
                    .execute_first(&**self.db)
                    .await?
                {
                    let existing_command_id =
                        CommandId::new(Self::required_string(&existing, "command_id")?);
                    let status =
                        CommandStatus::parse(&Self::required_string(&existing, "status")?)?;

                    match status {
                        CommandStatus::Pending => Ok(BeginCommandResult::DuplicateRejected {
                            command_id: existing_command_id,
                            reason: "Command with idempotency key already pending".to_string(),
                        }),
                        CommandStatus::Applied => Ok(BeginCommandResult::DuplicateApplied {
                            command_id: existing_command_id,
                            resulting_revision: Self::revision_from_i64(Self::required_i64(
                                &existing,
                                "resulting_revision",
                            )?)?,
                        }),
                        CommandStatus::Rejected => Ok(BeginCommandResult::DuplicateRejected {
                            command_id: existing_command_id,
                            reason: Self::optional_string(&existing, "error_reason")
                                .unwrap_or_else(|| "Command already rejected".to_string()),
                        }),
                    }
                } else {
                    Err(SharedStateError::Database(insert_error))
                }
            }
        }
    }

    async fn mark_applied(
        &self,
        command_id: &CommandId,
        resulting_revision: Revision,
    ) -> Result<(), SharedStateError> {
        self.db
            .update("shared_state_commands")
            .value("status", CommandStatus::Applied.as_str())
            .value(
                "resulting_revision",
                Self::i64_from_revision(resulting_revision)?,
            )
            .value("updated_at_ms", Self::now_unix_ms()?)
            .where_eq("command_id", command_id.as_str())
            .execute(&**self.db)
            .await?;

        Ok(())
    }

    async fn mark_rejected(
        &self,
        command_id: &CommandId,
        reason: &str,
    ) -> Result<(), SharedStateError> {
        self.db
            .update("shared_state_commands")
            .value("status", CommandStatus::Rejected.as_str())
            .value("error_reason", reason)
            .value("updated_at_ms", Self::now_unix_ms()?)
            .where_eq("command_id", command_id.as_str())
            .execute(&**self.db)
            .await?;

        Ok(())
    }

    async fn load_by_idempotency_key(
        &self,
        channel_id: &ChannelId,
        idempotency_key: &IdempotencyKey,
    ) -> Result<Option<CommandEnvelope>, SharedStateError> {
        let row = self
            .db
            .select("shared_state_commands")
            .columns(&[
                "command_id",
                "channel_id",
                "participant_id",
                "idempotency_key",
                "expected_revision",
                "command_name",
                "payload_data",
                "payload_format",
                "payload_storage",
                "metadata_data",
                "metadata_format",
                "metadata_storage",
                "created_at_ms",
            ])
            .where_eq("channel_id", channel_id.as_str())
            .where_eq("idempotency_key", idempotency_key.as_str())
            .execute_first(&**self.db)
            .await?;

        row.map(|row| {
            let payload = PayloadBlob {
                data: Self::required_string(&row, "payload_data")?,
                format: PayloadFormat::try_from_i16(
                    i16::try_from(Self::required_i64(&row, "payload_format")?).map_err(|e| {
                        SharedStateError::Conversion(format!("Invalid payload_format value: {e}"))
                    })?,
                )?,
                storage: PayloadStorage::try_from_i16(
                    i16::try_from(Self::required_i64(&row, "payload_storage")?).map_err(|e| {
                        SharedStateError::Conversion(format!("Invalid payload_storage value: {e}"))
                    })?,
                )?,
            };

            Ok(CommandEnvelope {
                command_id: CommandId::new(Self::required_string(&row, "command_id")?),
                channel_id: ChannelId::new(Self::required_string(&row, "channel_id")?),
                participant_id: hyperchad_shared_state_models::ParticipantId::new(
                    Self::required_string(&row, "participant_id")?,
                ),
                idempotency_key: IdempotencyKey::new(Self::required_string(
                    &row,
                    "idempotency_key",
                )?),
                expected_revision: Self::revision_from_i64(Self::required_i64(
                    &row,
                    "expected_revision",
                )?)?,
                command_name: Self::required_string(&row, "command_name")?,
                payload,
                metadata: Self::decode_metadata(
                    Self::required_string(&row, "metadata_data")?,
                    i16::try_from(Self::required_i64(&row, "metadata_format")?).map_err(|e| {
                        SharedStateError::Conversion(format!("Invalid metadata_format value: {e}"))
                    })?,
                    i16::try_from(Self::required_i64(&row, "metadata_storage")?).map_err(|e| {
                        SharedStateError::Conversion(format!("Invalid metadata_storage value: {e}"))
                    })?,
                )?,
                created_at_ms: Self::required_i64(&row, "created_at_ms")?,
            })
        })
        .transpose()
    }
}

#[async_trait]
impl EventStore for SwitchySharedStateStore {
    #[allow(clippy::too_many_lines)]
    async fn append_events(
        &self,
        command: &CommandEnvelope,
        drafts: &[EventDraft],
    ) -> Result<AppendEventsResult, SharedStateError> {
        let tx = self.db.begin_transaction().await?;

        let channel_row = tx
            .select("shared_state_channels")
            .columns(&["revision"])
            .where_eq("channel_id", command.channel_id.as_str())
            .execute_first(&*tx)
            .await?;

        let actual_revision = if let Some(channel_row) = channel_row {
            Self::revision_from_i64(Self::required_i64(&channel_row, "revision")?)?
        } else {
            match tx
                .insert("shared_state_channels")
                .value("channel_id", command.channel_id.as_str())
                .value("revision", 0_i64)
                .value("updated_at_ms", Self::now_unix_ms()?)
                .execute(&*tx)
                .await
            {
                Ok(_row) => Revision::new(0),
                Err(_error) => {
                    let reloaded = tx
                        .select("shared_state_channels")
                        .columns(&["revision"])
                        .where_eq("channel_id", command.channel_id.as_str())
                        .execute_first(&*tx)
                        .await?
                        .ok_or_else(|| {
                            SharedStateError::Conversion(
                                "Failed to initialize shared_state_channels row".to_string(),
                            )
                        })?;

                    Self::revision_from_i64(Self::required_i64(&reloaded, "revision")?)?
                }
            }
        };

        if actual_revision != command.expected_revision {
            tx.rollback().await?;
            return Ok(AppendEventsResult::Conflict { actual_revision });
        }

        if drafts.is_empty() {
            tx.commit().await?;
            return Ok(AppendEventsResult::Appended {
                from_revision: command.expected_revision,
                to_revision: command.expected_revision,
                events: Vec::new(),
            });
        }

        let now_ms = Self::now_unix_ms()?;
        let mut events = Vec::with_capacity(drafts.len());

        for (index, draft) in drafts.iter().enumerate() {
            let offset = u64::try_from(index)
                .map_err(|e| SharedStateError::Conversion(format!("Index overflow: {e}")))?
                + 1;
            let revision = command.expected_revision.incremented_by(offset);
            let event_id = EventId::new(format!("{}:{}", command.command_id, revision));
            let metadata_payload = Self::encode_metadata(&draft.metadata)?;

            tx.insert("shared_state_events")
                .value("channel_id", command.channel_id.as_str())
                .value("revision", Self::i64_from_revision(revision)?)
                .value("event_id", event_id.as_str())
                .value("command_id", command.command_id.as_str())
                .value("event_name", draft.event_name.as_str())
                .value("payload_data", draft.payload.data.as_str())
                .value("payload_format", i64::from(draft.payload.format.as_i16()))
                .value("payload_storage", i64::from(draft.payload.storage.as_i16()))
                .value("metadata_data", metadata_payload.data.as_str())
                .value(
                    "metadata_format",
                    i64::from(metadata_payload.format.as_i16()),
                )
                .value(
                    "metadata_storage",
                    i64::from(metadata_payload.storage.as_i16()),
                )
                .value("created_at_ms", now_ms)
                .execute(&*tx)
                .await?;

            events.push(EventEnvelope {
                event_id,
                channel_id: command.channel_id.clone(),
                revision,
                command_id: Some(command.command_id.clone()),
                event_name: draft.event_name.clone(),
                payload: draft.payload.clone(),
                metadata: draft.metadata.clone(),
                created_at_ms: now_ms,
            });
        }

        let to_revision = command.expected_revision.incremented_by(
            u64::try_from(drafts.len())
                .map_err(|e| SharedStateError::Conversion(format!("Draft count overflow: {e}")))?,
        );

        let updated = tx
            .update("shared_state_channels")
            .value("revision", Self::i64_from_revision(to_revision)?)
            .value("updated_at_ms", now_ms)
            .where_eq("channel_id", command.channel_id.as_str())
            .where_eq(
                "revision",
                Self::i64_from_revision(command.expected_revision)?,
            )
            .execute(&*tx)
            .await?;

        if updated.is_empty() {
            tx.rollback().await?;
            let actual_revision = self
                .latest_revision(&command.channel_id)
                .await?
                .unwrap_or_default();
            return Ok(AppendEventsResult::Conflict { actual_revision });
        }

        tx.commit().await?;

        Ok(AppendEventsResult::Appended {
            from_revision: command.expected_revision,
            to_revision,
            events,
        })
    }

    async fn read_events(
        &self,
        channel_id: &ChannelId,
        from_exclusive_revision: Option<Revision>,
        limit: u32,
    ) -> Result<Vec<EventEnvelope>, SharedStateError> {
        let mut query = self
            .db
            .select("shared_state_events")
            .columns(&[
                "event_id",
                "channel_id",
                "revision",
                "command_id",
                "event_name",
                "payload_data",
                "payload_format",
                "payload_storage",
                "metadata_data",
                "metadata_format",
                "metadata_storage",
                "created_at_ms",
            ])
            .where_eq("channel_id", channel_id.as_str())
            .sort("revision", switchy_database::query::SortDirection::Asc)
            .limit(usize::try_from(limit).map_err(|e| {
                SharedStateError::Conversion(format!("Invalid event read limit: {e}"))
            })?);

        if let Some(from_exclusive_revision) = from_exclusive_revision {
            query = query.where_gt(
                "revision",
                Self::i64_from_revision(from_exclusive_revision)?,
            );
        }

        query
            .execute(&**self.db)
            .await?
            .iter()
            .map(Self::map_event_row)
            .collect()
    }

    async fn latest_revision(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Option<Revision>, SharedStateError> {
        self.db
            .select("shared_state_channels")
            .columns(&["revision"])
            .where_eq("channel_id", channel_id.as_str())
            .execute_first(&**self.db)
            .await?
            .map(|row| Self::revision_from_i64(Self::required_i64(&row, "revision")?))
            .transpose()
    }
}

#[async_trait]
impl SnapshotStore for SwitchySharedStateStore {
    async fn load_latest_snapshot(
        &self,
        channel_id: &ChannelId,
    ) -> Result<Option<SnapshotEnvelope>, SharedStateError> {
        let row = self
            .db
            .select("shared_state_snapshots")
            .columns(&[
                "channel_id",
                "revision",
                "payload_data",
                "payload_format",
                "payload_storage",
                "created_at_ms",
            ])
            .where_eq("channel_id", channel_id.as_str())
            .sort("revision", switchy_database::query::SortDirection::Desc)
            .limit(1)
            .execute_first(&**self.db)
            .await?;

        row.map(|row| {
            Ok(SnapshotEnvelope {
                channel_id: ChannelId::new(Self::required_string(&row, "channel_id")?),
                revision: Self::revision_from_i64(Self::required_i64(&row, "revision")?)?,
                payload: PayloadBlob {
                    data: Self::required_string(&row, "payload_data")?,
                    format: PayloadFormat::try_from_i16(
                        i16::try_from(Self::required_i64(&row, "payload_format")?).map_err(
                            |e| {
                                SharedStateError::Conversion(format!(
                                    "Invalid payload_format value: {e}"
                                ))
                            },
                        )?,
                    )?,
                    storage: PayloadStorage::try_from_i16(
                        i16::try_from(Self::required_i64(&row, "payload_storage")?).map_err(
                            |e| {
                                SharedStateError::Conversion(format!(
                                    "Invalid payload_storage value: {e}"
                                ))
                            },
                        )?,
                    )?,
                },
                created_at_ms: Self::required_i64(&row, "created_at_ms")?,
            })
        })
        .transpose()
    }

    async fn put_snapshot(&self, snapshot: &SnapshotEnvelope) -> Result<(), SharedStateError> {
        self.db
            .upsert("shared_state_snapshots")
            .value("channel_id", snapshot.channel_id.as_str())
            .value("revision", Self::i64_from_revision(snapshot.revision)?)
            .value("payload_data", snapshot.payload.data.as_str())
            .value(
                "payload_format",
                i64::from(snapshot.payload.format.as_i16()),
            )
            .value(
                "payload_storage",
                i64::from(snapshot.payload.storage.as_i16()),
            )
            .value("created_at_ms", snapshot.created_at_ms)
            .where_eq("channel_id", snapshot.channel_id.as_str())
            .where_eq("revision", Self::i64_from_revision(snapshot.revision)?)
            .unique(&["channel_id", "revision"])
            .execute(&**self.db)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use hyperchad_shared_state_models::{
        ChannelId, CommandEnvelope, CommandId, IdempotencyKey, ParticipantId, PayloadBlob, Revision,
    };

    use crate::{
        storage::{SwitchySharedStateStore, migrate_shared_state},
        traits::{
            AppendEventsResult, BeginCommandResult, CommandStore, EventDraft, EventStore,
            SnapshotStore,
        },
    };

    #[test_log::test(switchy_async::test)]
    async fn round_trip_append_and_replay() -> Result<(), crate::SharedStateError> {
        let db = switchy_database_connection::init_sqlite_sqlx(None)
            .await
            .map_err(|e| {
                crate::SharedStateError::Conversion(format!("Failed to init sqlite db: {e}"))
            })?;
        migrate_shared_state(&*db).await?;

        let store = SwitchySharedStateStore::from_box(db);

        let command = CommandEnvelope {
            command_id: CommandId::new("cmd-1"),
            channel_id: ChannelId::new("channel-a"),
            participant_id: ParticipantId::new("participant-a"),
            idempotency_key: IdempotencyKey::new("idem-1"),
            expected_revision: Revision::new(0),
            command_name: "APPLY_DELTA".to_string(),
            payload: PayloadBlob::from_serializable(&vec![1_u8, 2_u8, 3_u8])?,
            metadata: BTreeMap::new(),
            created_at_ms: 1,
        };

        let begin = store.begin_command(&command).await?;
        assert_eq!(begin, BeginCommandResult::New);

        let drafts = vec![EventDraft::new(
            "DELTA_APPLIED",
            PayloadBlob::from_serializable(&42_u32)?,
            BTreeMap::new(),
        )];

        let append = store.append_events(&command, &drafts).await?;
        let resulting_revision = match append {
            AppendEventsResult::Appended {
                from_revision,
                to_revision,
                events,
            } => {
                assert_eq!(from_revision, Revision::new(0));
                assert_eq!(events.len(), 1);
                to_revision
            }
            AppendEventsResult::Conflict { actual_revision } => {
                panic!("Unexpected conflict revision={actual_revision}");
            }
        };

        store
            .mark_applied(&command.command_id, resulting_revision)
            .await?;

        let loaded = store.read_events(&command.channel_id, None, 100).await?;
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].event_name, "DELTA_APPLIED");

        let snapshot_payload = PayloadBlob::from_serializable(&vec!["state"])?;
        store
            .put_snapshot(&hyperchad_shared_state_models::SnapshotEnvelope {
                channel_id: command.channel_id.clone(),
                revision: resulting_revision,
                payload: snapshot_payload,
                created_at_ms: 2,
            })
            .await?;

        let snapshot = store.load_latest_snapshot(&command.channel_id).await?;
        assert!(snapshot.is_some());

        Ok(())
    }
}
