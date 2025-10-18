# Migration Recovery Commands

This document provides detailed documentation for the migration recovery commands implemented in Phase 11.2.4.

## Overview

The switchy-migrate CLI now includes comprehensive recovery commands for handling failed or problematic migrations:

- `status --show-failed` - Enhanced status display with failure details
- `retry <migration_id>` - Retry failed migrations
- `mark-completed <migration_id>` - Manually mark migrations as completed
- `migrate --force` - Force migrations despite dirty state

## Commands Reference

### Enhanced Status Command

```bash
switchy-migrate status --show-failed --database-url <DATABASE_URL>
```

**Features:**

- Colored status indicators: ✓ Completed (green), ✗ Failed (red), ⚠ In Progress (yellow), - Pending
- Displays timestamps (started/finished) for applied migrations
- Shows failure reasons for failed migrations
- Warns about in-progress migrations that may indicate interrupted operations

**Example Output:**

```
Migration Status
================
Migrations directory: ./migrations
Migration table: __switchy_migrations

✓ Completed    20240101_initial_schema
               Started: 2024-01-01 10:00:00
               Finished: 2024-01-01 10:00:05

✗ Failed       20240102_add_users_table
               Started: 2024-01-02 14:30:00
               Finished: 2024-01-02 14:30:15
               Error: column "user_id" already exists

⚠ In Progress  20240103_update_indexes
               Started: 2024-01-03 09:15:00

- Pending      20240104_cleanup_data

Summary:
  Completed: 3
  Failed: 0
  In Progress: 1 (Phase 11.4.12 - Development Workflow Documentation)
  Pending: 0
  Total: 4

⚠️  WARNING: Found migrations in progress - this may indicate interrupted operations
❌ ERROR: Found failed migrations - use 'switchy-migrate retry <migration_id>' to retry
```

### Retry Command

```bash
switchy-migrate retry <migration_id> --database-url <DATABASE_URL>
```

**Purpose:** Retry a failed migration after fixing the underlying issue.

**Behavior:**

1. Validates the migration is in failed state
2. Removes the failed migration record
3. Re-executes the migration
4. Updates status to completed or failed

**Example:**

```bash
$ switchy-migrate retry 20240102_add_users_table --database-url sqlite://app.db

Retrying migration: 20240102_add_users_table
Migrations directory: ./migrations

✓ Successfully retried migration '20240102_add_users_table'
```

**Error Cases:**

- Migration not found: Clear error with available migrations context
- Migration not in failed state: Shows current status
- Migration exists but not in source: Indicates missing migration file

### Mark Completed Command

```bash
switchy-migrate mark-completed <migration_id> [--force] --database-url <DATABASE_URL>
```

**Purpose:** Manually mark a migration as completed without running it (dangerous operation).

**Behavior:**

- **Without --force**: Shows warning and interactive Y/n confirmation
- **With --force**: Bypasses confirmation dialog

**Interactive Mode Example:**

```bash
$ switchy-migrate mark-completed 20240103_problematic_migration --database-url sqlite://app.db

⚠️  Marking migration as completed: 20240103_problematic_migration
Migrations directory: ./migrations

⚠️  WARNING: This is a dangerous operation!
Marking a migration as completed without running it can lead to:
- Database schema inconsistencies
- Failed future migrations
- Data corruption

Are you sure you want to mark this migration as completed? [y/N]: y

✓ Migration '20240103_problematic_migration' marked as completed
```

**Force Mode Example:**

```bash
$ switchy-migrate mark-completed 20240103_problematic_migration --force --database-url sqlite://app.db

⚠️  Marking migration as completed: 20240103_problematic_migration
Migrations directory: ./migrations

⚠️  WARNING: This is a dangerous operation!
[... warnings ...]

✓ Migration '20240103_problematic_migration' marked as completed
```

### Mark All Completed Command

```bash
switchy-migrate mark-all-completed [OPTIONS] --database-url <DATABASE_URL>
```

**Purpose:** Mark migrations as completed without executing them. The scope of affected migrations is controlled by flags.

#### Available Scopes

**Default: Pending Only (Safest)**

```bash
switchy-migrate mark-all-completed --database-url <DATABASE_URL>
```

Only marks untracked migrations as completed. This is the safest option and recommended for initialization scenarios.

**Include Failed Migrations**

```bash
switchy-migrate mark-all-completed --include-failed --database-url <DATABASE_URL>
```

Marks both untracked and failed migrations as completed. Use when failed migrations were manually fixed.

**Include In-Progress Migrations**

```bash
switchy-migrate mark-all-completed --include-in-progress --database-url <DATABASE_URL>
```

Marks both untracked and in-progress migrations as completed. Use when migration process crashed but migrations actually completed.

**All Migrations (Most Dangerous)**

```bash
switchy-migrate mark-all-completed --all --database-url <DATABASE_URL>
```

Marks all migrations as completed regardless of state. Use for complete tracking table reset/sync.

**Drop and Recreate Table (CRITICAL)**

```bash
switchy-migrate mark-all-completed --drop --database-url <DATABASE_URL>
```

**CRITICAL OPERATION** - Drops the entire migration tracking table before marking. This permanently deletes all migration history.

**What happens:**

1. Drops `__switchy_migrations` table
2. Recreates fresh table with current schema
3. Marks all source migrations as completed with new checksums

**What you lose:**

- All migration execution timestamps
- Failure reasons and error messages
- Old checksums for validation
- All status history (completed/failed/in-progress)

**Use when:**

- ✅ Migration tracking table is corrupted
- ✅ Table schema is incompatible with code
- ✅ Need complete history reset
- ❌ **NOT** for normal recovery (use scopes instead)

**Can be combined with scopes:**

```bash
# Drop table, then mark only pending migrations
switchy-migrate mark-all-completed --drop --database-url <DATABASE_URL>

# Drop table, then mark including failed migrations
switchy-migrate mark-all-completed --drop --include-failed --database-url <DATABASE_URL>
```

#### Behavior Matrix

| Scope                     | Untracked           | Completed           | Failed                | InProgress            | Special                      |
| ------------------------- | ------------------- | ------------------- | --------------------- | --------------------- | ---------------------------- |
| **Default**               | ✅ Mark → Completed | ⏭️ Skip (unchanged) | ⏭️ Skip (unchanged)   | ⏭️ Skip (unchanged)   | -                            |
| **--include-failed**      | ✅ Mark → Completed | ⏭️ Skip (unchanged) | ⚠️ Update → Completed | ⏭️ Skip (unchanged)   | -                            |
| **--include-in-progress** | ✅ Mark → Completed | ⏭️ Skip (unchanged) | ⏭️ Skip (unchanged)   | ⚠️ Update → Completed | -                            |
| **--all**                 | ✅ Mark → Completed | ⏭️ Skip (unchanged) | ⚠️ Update → Completed | ⚠️ Update → Completed | -                            |
| **--drop**                | ✅ Mark → Completed | N/A (table dropped) | N/A (table dropped)   | N/A (table dropped)   | 🗑️ Deletes all history first |

#### Interactive Confirmation

The confirmation process adapts to the danger level:

**Default Scope:**

- Moderate danger warning
- Single confirmation required
- Shows what will be marked vs preserved

**Dangerous Scopes (--include-failed, --include-in-progress):**

- High danger warning
- Double confirmation required
- Detailed breakdown of state changes

**All Scope (--all):**

- Extreme danger warning
- Double confirmation required
- Comprehensive warnings about consequences

**Drop Flag (--drop):**

- CRITICAL danger warning
- Triple confirmation required
- Explicit warnings about permanent data loss
- Lists exactly what will be deleted

**Force Mode:**

```bash
switchy-migrate mark-all-completed --all --force --database-url <DATABASE_URL>
switchy-migrate mark-all-completed --drop --force --database-url <DATABASE_URL>
```

Bypasses all confirmations. Use with extreme caution, especially with `--drop`.

#### Example: Default Scope (Pending Only)

```bash
$ switchy-migrate mark-all-completed --database-url sqlite://app.db

⚠️  Marking migrations as completed
Migrations directory: ./migrations
Scope: PendingOnly
Danger level: MODERATE

⚠️  WARNING: This will mark untracked migrations as completed!
This is relatively safe but can still lead to issues if:
  • Database schema doesn't match migrations
  • Migrations haven't been manually applied

This operation will:
  ✓ Mark untracked migrations as completed
  ⏭ Leave completed migrations unchanged
  ⏭ Leave failed migrations unchanged
  ⏭ Leave in-progress migrations unchanged

Only use this if:
  • You're initializing a tracking table for an existing database
  • You've manually applied migrations and need to sync
  • You're recovering from schema table corruption

Are you sure you want to mark untracked migrations as completed? [y/N] y

✓ Operation completed successfully!

Summary:
  Total migrations found:              47
  Already completed:                   12
  Newly marked as completed:           30
  Failed migrations skipped:           3
  In-Progress migrations skipped:      2
```

#### Example: Include Failed Scope

```bash
$ switchy-migrate mark-all-completed --include-failed --database-url sqlite://app.db

⚠️  Marking migrations as completed
Migrations directory: ./migrations
Scope: IncludeFailed
Danger level: HIGH

⚠️  DANGER: This will mark untracked AND FAILED migrations as completed!
══════════════════════════════════════════════════════════════════════
This operation will:
  ✓ Mark untracked migrations as completed
  ⚠ Mark FAILED migrations as completed
  ⏭ Leave completed migrations unchanged
  ⏭ Leave in-progress migrations unchanged
══════════════════════════════════════════════════════════════════════

Use this only if:
  • Failed migrations were manually fixed
  • You want to skip multiple failed migrations

Only use this if:
  • You're initializing a tracking table for an existing database
  • You've manually applied migrations and need to sync
  • You're recovering from schema table corruption

Are you SURE you want to proceed with this dangerous operation? [y/N] y
Last chance: Proceed? [y/N] y

✓ Operation completed successfully!

Summary:
  Total migrations found:              47
  Already completed:                   12
  Newly marked as completed:           30
  Failed → Completed:                  3
  In-Progress migrations skipped:      2
```

#### Example: All Scope

```bash
$ switchy-migrate mark-all-completed --all --database-url sqlite://app.db

⚠️  Marking migrations as completed
Migrations directory: ./migrations
Scope: All
Danger level: EXTREME

🚨 EXTREME DANGER: THIS WILL MARK ALL MIGRATIONS AS COMPLETED! 🚨
══════════════════════════════════════════════════════════════════════
This operation will:
  ✓ Mark untracked migrations as completed
  ⚠ Mark FAILED migrations as completed
  ⚠ Mark IN-PROGRESS migrations as completed
  ⏭ Leave completed migrations unchanged
══════════════════════════════════════════════════════════════════════

This can lead to:
  ✗ Database schema inconsistencies
  ✗ Failed future migrations
  ✗ Data corruption
  ✗ Application crashes

Only use this if:
  • You're initializing a tracking table for an existing database
  • You've manually applied migrations and need to sync
  • You're recovering from schema table corruption

Are you ABSOLUTELY CERTAIN you want to mark ALL migrations as completed? [y/N] y
Last chance: Proceed? [y/N] y

✓ Operation completed successfully!

Summary:
  Total migrations found:              47
  Already completed:                   12
  Newly marked as completed:           30
  Failed → Completed:                  3
  In-Progress → Completed:             2
```

#### Example: Drop Flag (CRITICAL)

```bash
$ switchy-migrate mark-all-completed --drop --database-url sqlite://app.db

⚠️  Marking migrations as completed
Migrations directory: ./migrations
Scope: PendingOnly
Drop table: YES (CRITICAL)
Danger level: CRITICAL

🔥 CRITICAL: THIS WILL DELETE ALL MIGRATION HISTORY! 🔥
██████████████████████████████████████████████████████████████████████

⚠️  ALL DATA IN THE MIGRATION TABLE WILL BE PERMANENTLY DELETED:
  ✗ Migration execution status (completed/failed/in-progress)
  ✗ Execution timestamps (when migrations ran)
  ✗ Failure reasons and error messages
  ✗ Stored checksums for validation

This operation will:
  1️⃣  DROP the entire '__switchy_migrations' table
  2️⃣  CREATE a fresh migration tracking table
  3️⃣  MARK all source migrations as completed with new checksums
██████████████████████████████████████████████████████████████████████

⚠️  THIS CANNOT BE UNDONE!

Only use this if:
  • The migration tracking table is corrupted
  • The table schema is incompatible with the current code
  • You need to completely reset migration history

⚠️ Type 'yes' if you want to DELETE ALL HISTORY and start fresh [y/N] yes

⚠️ Are you ABSOLUTELY sure? This will PERMANENTLY DELETE all migration history! [y/N] y

⚙ Dropping migration tracking table...
✓ Table dropped successfully
⚙ Creating fresh migration tracking table...
✓ Fresh table created

✓ Operation completed successfully!

Summary:
  Total migrations found:              47
  Already completed:                   0
  Newly marked as completed:           47
  Failed → Completed:                  0
  In-Progress → Completed:             0
```

Note: After `--drop`, all migrations are "newly marked" because the table was recreated from scratch.

#### API Usage

```rust
use switchy_schema::runner::{MigrationRunner, MarkCompletedScope};

// Default: Only mark pending (safest)
let summary = runner
    .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
    .await?;
println!("Marked {} new migrations", summary.newly_marked);
println!("Skipped {} failed migrations", summary.failed_skipped);

// Include failed migrations
let summary = runner
    .mark_all_migrations_completed(&*db, MarkCompletedScope::IncludeFailed)
    .await?;
println!("Marked {} failed migrations", summary.failed_marked);

// Include in-progress migrations
let summary = runner
    .mark_all_migrations_completed(&*db, MarkCompletedScope::IncludeInProgress)
    .await?;
println!("Marked {} in-progress migrations", summary.in_progress_marked);

// All migrations (dangerous)
let summary = runner
    .mark_all_migrations_completed(&*db, MarkCompletedScope::All)
    .await?;
println!("Total: {}, New: {}, Failed: {}, InProgress: {}",
         summary.total, summary.newly_marked,
         summary.failed_marked, summary.in_progress_marked);

// Drop and recreate table (CRITICAL - use with extreme caution)
runner.drop_tracking_table(&*db).await?;
runner.ensure_tracking_table_exists(&*db).await?;
let summary = runner
    .mark_all_migrations_completed(&*db, MarkCompletedScope::PendingOnly)
    .await?;
println!("Fresh table created with {} migrations", summary.newly_marked);
```

#### Return Type

```rust
pub struct MarkAllCompletedSummary {
    /// Total number of migrations found
    pub total: usize,
    /// Number of migrations that were already completed
    pub already_completed: usize,
    /// Number of migrations newly marked as completed (were untracked)
    pub newly_marked: usize,
    /// Number of failed migrations updated to completed
    pub failed_marked: usize,
    /// Number of in-progress migrations updated to completed
    pub in_progress_marked: usize,
    /// Number of failed migrations that were skipped (not included in scope)
    pub failed_skipped: usize,
    /// Number of in-progress migrations that were skipped (not included in scope)
    pub in_progress_skipped: usize,
}
```

The summary provides detailed statistics showing:

- What was marked as completed
- What was updated from a different state
- What was explicitly skipped due to scope constraints

#### Use Case Guide

**When to use each scope:**

| Scenario                                  | Recommended Scope       | Reasoning                                               |
| ----------------------------------------- | ----------------------- | ------------------------------------------------------- |
| Initialize tracking for existing database | Default (no flags)      | Only marks new migrations, preserves any existing state |
| Multiple failed migrations manually fixed | `--include-failed`      | Marks failed migrations as complete after manual fixes  |
| Migration process crashed mid-execution   | `--include-in-progress` | Marks stuck in-progress migrations as complete          |
| Complete tracking table rebuild           | `--all`                 | Nuclear option - marks everything as complete           |
| Read-only deployment initialization       | Use env var instead     | `MOOSICBOX_SKIP_MIGRATION_EXECUTION=1`                  |

**When NOT to use:**

❌ **Don't use `--all` if:**

- You're unsure about the current schema state
- Migrations haven't been applied
- You're trying to "skip" migrations during development
- Any migrations are legitimately failed (fix them first!)

✅ **Instead:**

- Use default scope for initialization
- Fix failed migrations and retry them
- Use `MOOSICBOX_SKIP_MIGRATION_EXECUTION=1` for read-only deployments

#### Safety Best Practices

1. **Always backup before using any scope**
2. **Start with default scope** - it's the safest
3. **Check what will be affected** - review the summary before confirming
4. **Use force sparingly** - confirmations exist for a reason
5. **Verify after operation** - run `status --show-failed` to check results
6. **Document your actions** - note why you used a particular scope

#### Related Commands

- `status --show-failed` - Check current migration state before marking
- `mark-completed <id>` - Mark a single migration (more targeted)
- `retry <id>` - Properly retry a failed migration (preferred over marking)
- `validate` - Check migration checksums after marking

#### Warnings

🚨 **CRITICAL SAFETY NOTICE** 🚨

**Default scope (no flags):**

- ✅ Safe for initialization
- ✅ Preserves failed/in-progress states
- ⚠️ Still dangerous if schema doesn't match

**With flags:**

- ⚠️ Dangerous: Changes migration states
- ⚠️ Information loss: Failure reasons remain but state changes
- ⚠️ May hide real problems: Failed migrations marked as complete

**Best Practice:**
Always backup your database before using this command with any scope.

### Force Migration

```bash
switchy-migrate migrate --force --database-url <DATABASE_URL>
```

**Purpose:** Run migrations even when there are migrations in "in_progress" or "failed" state (dirty state).

**Behavior:**

1. Shows warning about potential data corruption
2. Bypasses dirty state check
3. Proceeds with normal migration execution

**Example:**

```bash
$ switchy-migrate migrate --force --database-url sqlite://app.db

⚠️  WARNING: Bypassing dirty state check - this may cause data corruption!
Proceeding with --force flag...

Running migrations from: ./migrations
Strategy: Apply all pending migrations

Applied migration: 20240104_cleanup_data (completed in 45ms)

Successfully applied 1 migration(s).
```

## Recovery Workflows

### Scenario 1: Failed Migration

1. **Identify the issue:**

    ```bash
    switchy-migrate status --show-failed --database-url <DATABASE_URL>
    ```

2. **Fix the underlying problem** (e.g., fix SQL syntax, resolve conflicts)

3. **Retry the migration:**
    ```bash
    switchy-migrate retry <migration_id> --database-url <DATABASE_URL>
    ```

### Scenario 2: Stuck In-Progress Migration

1. **Verify the migration is truly stuck** (not just taking a long time)

2. **Choose recovery approach:**
    - **If migration can be safely retried:** Use `retry` command
    - **If migration partially completed:** Use `mark-completed` to skip it
    - **If you want to continue with new migrations:** Use `migrate --force`

### Scenario 3: Migration Applied Outside System

1. **Manually applied migration needs to be recorded:**

    ```bash
    switchy-migrate mark-completed <migration_id> --database-url <DATABASE_URL>
    ```

2. **Verify system state:**
    ```bash
    switchy-migrate status --database-url <DATABASE_URL>
    ```

## Best Practices

### Before Recovery Operations

1. **Backup your database** before any recovery operation
2. **Understand the migration content** - review the SQL files
3. **Check application compatibility** - ensure your app can handle the current state
4. **Use `--dry-run` when available** to preview changes

### During Recovery

1. **Read error messages carefully** - they often contain the solution
2. **Fix root causes, not just symptoms** - address underlying issues
3. **Test in development first** - never try recovery procedures in production first
4. **Use `--force` sparingly** - it's dangerous and should be a last resort

### After Recovery

1. **Verify system state** with `status --show-failed`
2. **Test application functionality** to ensure everything works
3. **Document what happened** for future reference
4. **Consider preventive measures** to avoid similar issues

## Error Messages Reference

### Common Error Messages

**Migration Not Found:**

```
❌ Migration '20240999_missing' not found in migration source
```

**Migration Wrong State:**

```
❌ Migration '20240101_initial' is in Completed state, not failed
```

**Migration Not Tracked:**

```
ℹ Migration '20240102_new' has not been run yet
Use 'migrate' command to run it for the first time.
```

**Migration In Progress:**

```
⚠ Migration '20240103_running' is currently in progress
Wait for it to complete or fail before retrying.
```

### Recovery Suggestions

The CLI provides context-aware suggestions based on the error:

- For completed migrations: No action needed
- For failed migrations: Use `retry` command
- For in-progress migrations: Wait or investigate if stuck
- For missing migrations: Check migration directory and file names
- For not-yet-run migrations: Use `migrate` command instead of `retry`

## Safety Features

### Confirmations

- Interactive prompts for dangerous operations
- Clear warnings about potential consequences
- Ability to cancel operations with Ctrl+C

### Validation

- Checks migration exists in source before operations
- Validates current migration state
- Prevents operations on migrations in wrong state

### Logging

- Clear success/failure messages
- Detailed error information with context
- Timestamps for all operations

## Environment Variables

All commands support the standard environment variables:

- `SWITCHY_DATABASE_URL` - Default database connection
- `SWITCHY_MIGRATIONS_DIR` - Default migrations directory
- `SWITCHY_MIGRATION_TABLE` - Default migration tracking table name

## Troubleshooting

### Command Not Found

Ensure `switchy-migrate` is built and in your PATH:

```bash
cargo build -p switchy_schema_cli
export PATH="$PATH:target/debug"
```

### Permission Denied

Check database connection permissions and file system access to migrations directory.

### Migration Files Missing

Ensure migration files exist in the migrations directory and have correct naming format.

### Database Connection Issues

Verify database URL format and connection parameters. Test with a simple query tool first.

## Related Documentation

- [Generic Schema Migrations Plan](plan.md) - Full implementation specification
- [MoosicBox Architecture](../../README.md) - Overall system architecture
- [Database Documentation](../../packages/database/) - Database abstraction layer

## Support

For issues with recovery commands:

1. Check this documentation for common scenarios
2. Review error messages for specific guidance
3. Test recovery procedures in development environment
4. Report bugs or feature requests via GitHub issues
