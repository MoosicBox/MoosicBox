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
