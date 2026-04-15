use hyperchad_shared_state_models::{CommandId, PayloadError, Revision};
use switchy_database::DatabaseError;
use switchy_schema::MigrationError;

#[derive(Debug, thiserror::Error)]
pub enum SharedStateError {
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error(transparent)]
    Migration(#[from] MigrationError),
    #[error(transparent)]
    Payload(#[from] PayloadError),
    #[error("Data conversion failed: {0}")]
    Conversion(String),
    #[error("Revision conflict: expected={expected} actual={actual}")]
    RevisionConflict {
        expected: Revision,
        actual: Revision,
    },
    #[error(
        "Command already applied: command_id={command_id} resulting_revision={resulting_revision}"
    )]
    DuplicateCommandApplied {
        command_id: CommandId,
        resulting_revision: Revision,
    },
    #[error("Command already rejected: command_id={command_id} reason={reason}")]
    DuplicateCommandRejected {
        command_id: CommandId,
        reason: String,
    },
}
