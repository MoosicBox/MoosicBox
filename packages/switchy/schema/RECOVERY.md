# Migration Recovery Guide

This guide provides comprehensive procedures for recovering from migration failures in the switchy_schema system. Understanding these recovery patterns is essential for maintaining database integrity in production environments.

## Table of Contents

- [Common Failure Scenarios](#common-failure-scenarios)
- [Recovery Procedures](#recovery-procedures)
- [Best Practices](#best-practices)
- [CLI Recovery Commands](#cli-recovery-commands)
- [Schema State Assessment](#schema-state-assessment)

## Common Failure Scenarios

### 1. Network Interruption During Migration

**Symptoms:**
- Migration status shows `in_progress` in the tracking table
- Process terminated unexpectedly
- Partial schema changes may be present

**What Happens:**
When a network interruption occurs during migration execution, the migration is marked as `in_progress` but never completes. The database may be left in a partially migrated state depending on where the interruption occurred.

**Example:**
```sql
-- Check migration status
SELECT * FROM __switchy_migrations WHERE status = 'in_progress';
-- Shows: id='2024-01-15-123456_add_user_table', status='in_progress', failure_reason=NULL
```

### 2. Process Killed During Migration

**Symptoms:**
- Migration status shows `in_progress` in the tracking table
- Application/CLI process was forcibly terminated (SIGKILL, system shutdown, etc.)
- Database connection was abruptly closed

**What Happens:**
Similar to network interruption, the migration tracking remains in `in_progress` state. Depending on transaction boundaries, the schema changes may be partially applied or fully rolled back by the database.

**Example:**
```sql
-- Check for interrupted migrations
SELECT id, run_on, status FROM __switchy_migrations
WHERE status = 'in_progress' AND run_on < NOW() - INTERVAL '1 hour';
```

### 3. SQL Syntax Errors in Migration Files

**Symptoms:**
- Migration status shows `failed` in the tracking table
- Clear error message in `failure_reason` column
- Schema changes were not applied

**What Happens:**
The migration system detects the SQL error during execution and properly marks the migration as failed with the specific error message.

**Example:**
```sql
-- Check failed migrations
SELECT id, failure_reason FROM __switchy_migrations WHERE status = 'failed';
-- Shows: failure_reason='near "COLUMN": syntax error'
```

### 4. Constraint Violations During Data Migration

**Symptoms:**
- Migration status shows `failed` in the tracking table
- Constraint violation error in `failure_reason` column
- Partial data changes may exist depending on transaction scope

**What Happens:**
When migrating existing data that violates new constraints (foreign keys, unique constraints, check constraints), the database rejects the changes and the migration is marked as failed.

**Example:**
```sql
-- Check constraint violation failures
SELECT id, failure_reason FROM __switchy_migrations
WHERE status = 'failed' AND failure_reason LIKE '%constraint%';
-- Shows: failure_reason='FOREIGN KEY constraint failed'
```

## Recovery Procedures

### For In-Progress (Dirty State) Migrations

#### Step 1: Identify the Failure
```bash
# Check migration status
switchy-migrate status --show-failed
```

#### Step 2: Assess Database State
Check if the schema changes were partially applied:
```sql
-- For table creation migration
SELECT name FROM sqlite_master WHERE type='table' AND name='your_new_table';

-- For column addition migration
PRAGMA table_info(your_existing_table);

-- For index creation migration
SELECT name FROM sqlite_master WHERE type='index' AND name='your_new_index';
```

#### Step 3: Choose Recovery Strategy

**Option A: Retry the Migration (Recommended)**
If the interruption was temporary (network, process kill):
```bash
# This will fail due to dirty state
switchy-migrate migrate

# Force retry (dangerous - use with caution)
switchy-migrate migrate --force
```

**Option B: Manual Cleanup and Retry**
If partial changes exist and need manual cleanup:
1. Manually clean up partial schema changes
2. Remove the dirty migration record:
```bash
# Remove the stuck migration record
switchy-migrate mark-completed --force 2024-01-15-123456_add_user_table
```
3. Re-run migrations:
```bash
switchy-migrate migrate
```

### For Failed Migrations

#### Step 1: Identify the Failure
```bash
# Show detailed failure information
switchy-migrate status --show-failed
```

#### Step 2: Assess the Root Cause
Review the `failure_reason` in the output to understand what went wrong:
- **SQL Syntax Error**: Fix the migration file
- **Constraint Violation**: Adjust data or constraints
- **Permission Error**: Check database permissions
- **Resource Error**: Check disk space, memory, etc.

#### Step 3: Fix and Retry

**For SQL Syntax Errors:**
1. Edit the migration file to fix the syntax
2. Retry the specific migration:
```bash
switchy-migrate retry 2024-01-15-123456_add_user_table
```

**For Constraint Violations:**
1. Either fix the existing data or modify the migration
2. Retry the migration:
```bash
switchy-migrate retry 2024-01-15-123456_add_user_table
```

**For Unfixable Migrations:**
If the migration cannot be fixed and you need to mark it as completed without running it:
```bash
# DANGEROUS: Only use if you're certain about the consequences
switchy-migrate mark-completed --force 2024-01-15-123456_add_user_table
```

### When to Retry vs Manual Fix vs Rollback

| Scenario | Recommended Action | Rationale |
|----------|-------------------|-----------|
| Network interruption | Retry with `--force` | Temporary issue, migration logic is sound |
| Process killed | Retry with `--force` | Temporary issue, migration logic is sound |
| SQL syntax error | Fix file and retry | Migration logic needs correction |
| Constraint violation with fixable data | Fix data and retry | Data can be corrected |
| Constraint violation with unfixable data | Modify migration and retry | Migration logic needs adjustment |
| Irrecoverable failure | Mark completed (dangerous) | Last resort, manual schema changes needed |

### How to Clean Up Partial Changes

#### For Table Creation Failures
```sql
-- If table was partially created, drop it
DROP TABLE IF EXISTS your_new_table;
```

#### For Column Addition Failures
```sql
-- SQLite doesn't support DROP COLUMN easily, may need table recreation
-- PostgreSQL/MySQL:
ALTER TABLE your_table DROP COLUMN IF EXISTS your_new_column;
```

#### For Index Creation Failures
```sql
-- Drop the index if it was partially created
DROP INDEX IF EXISTS your_new_index;
```

#### For Data Migration Failures
```sql
-- Restore from backup or manually revert data changes
-- This is why backups before migrations are critical
```

## Best Practices

### Always Backup Before Migrations
```bash
# Example backup commands before migration
# SQLite
cp production.db production_backup_$(date +%Y%m%d_%H%M%S).db

# PostgreSQL
pg_dump -h localhost -U username dbname > backup_$(date +%Y%m%d_%H%M%S).sql

# MySQL
mysqldump -u username -p dbname > backup_$(date +%Y%m%d_%H%M%S).sql
```

### Test Migrations in Staging First
- Always test migration sequences on staging data
- Verify rollback procedures work correctly
- Test with production-like data volumes
- Validate constraint changes with real data patterns

### Monitor Migration Execution
```bash
# Run migration with verbose output
switchy-migrate migrate --dry-run  # Preview changes first

# Monitor in separate terminal
watch -n 1 "switchy-migrate status --show-failed"
```

### Use Transactions Where Possible
The switchy_schema system automatically wraps migrations in transactions where supported by the database. This ensures:
- Failed migrations don't leave partial changes
- Rollback on error is automatic
- Schema consistency is maintained

### Keep Migrations Idempotent When Feasible
Design migrations to be safely re-runnable:
```sql
-- Good: Idempotent table creation
CREATE TABLE IF NOT EXISTS users (
    id INTEGER PRIMARY KEY,
    email TEXT UNIQUE NOT NULL
);

-- Good: Idempotent column addition
ALTER TABLE users ADD COLUMN created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP;
-- Note: This may fail on second run with some databases, plan accordingly

-- Good: Idempotent index creation
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
```

## CLI Recovery Commands

### Check Migration Status
```bash
# Basic status check
switchy-migrate status

# Show detailed status including failed and in-progress migrations
switchy-migrate status --show-failed

# Use custom database and table
switchy-migrate status \
    --database-url "sqlite:///path/to/db.sqlite" \
    --migration-table "__custom_migrations" \
    --show-failed
```

### Retry Failed Migration
```bash
# Retry a specific failed migration
switchy-migrate retry 2024-01-15-123456_add_user_table

# With custom configuration
switchy-migrate retry \
    --database-url "postgresql://user:pass@localhost/db" \
    --migrations-dir "./custom_migrations" \
    --migration-table "__custom_migrations" \
    2024-01-15-123456_add_user_table
```

### Force Mark as Completed (Dangerous!)
```bash
# Mark a migration as completed without running it
# WARNING: Only use this if you're absolutely certain the migration
# has been manually applied or is no longer needed
switchy-migrate mark-completed --force 2024-01-15-123456_add_user_table

# With confirmation prompt (safer)
switchy-migrate mark-completed 2024-01-15-123456_add_user_table
```

### Run Migrations with Dirty State (Dangerous!)
```bash
# Force migration execution even with in-progress migrations
# WARNING: This bypasses safety checks and may cause data corruption
switchy-migrate migrate --force

# Safer approach: First check what's dirty
switchy-migrate status --show-failed
# Then decide if force is appropriate based on the specific situation
```

### Environment Variables
Set these environment variables to avoid repeating common options:
```bash
export SWITCHY_DATABASE_URL="sqlite:///path/to/db.sqlite"
export SWITCHY_MIGRATIONS_DIR="./migrations"
export SWITCHY_MIGRATION_TABLE="__switchy_migrations"

# Now commands are simpler
switchy-migrate status --show-failed
switchy-migrate retry 2024-01-15-123456_add_user_table
```

## Schema State Assessment

### Checking Table Existence
```sql
-- SQLite
SELECT name FROM sqlite_master WHERE type='table' ORDER BY name;

-- PostgreSQL
SELECT tablename FROM pg_tables WHERE schemaname = 'public' ORDER BY tablename;

-- MySQL
SELECT table_name FROM information_schema.tables
WHERE table_schema = DATABASE() ORDER BY table_name;
```

### Checking Column Structure
```sql
-- SQLite
PRAGMA table_info(table_name);

-- PostgreSQL
SELECT column_name, data_type, is_nullable, column_default
FROM information_schema.columns
WHERE table_name = 'your_table' ORDER BY ordinal_position;

-- MySQL
DESCRIBE table_name;
```

### Checking Index Existence
```sql
-- SQLite
SELECT name FROM sqlite_master WHERE type='index' AND tbl_name='your_table';

-- PostgreSQL
SELECT indexname FROM pg_indexes WHERE tablename = 'your_table';

-- MySQL
SHOW INDEX FROM your_table;
```

### Checking Migration History
```sql
-- View all migrations
SELECT id, run_on, finished_on, status, failure_reason
FROM __switchy_migrations
ORDER BY run_on;

-- View only problematic migrations
SELECT id, status, failure_reason, run_on
FROM __switchy_migrations
WHERE status IN ('failed', 'in_progress')
ORDER BY run_on;

-- Check for recent activity
SELECT id, status, run_on, failure_reason
FROM __switchy_migrations
WHERE run_on > datetime('now', '-1 day')
ORDER BY run_on DESC;
```

## Emergency Recovery Scenarios

### Complete Database Corruption
1. **Stop all applications** accessing the database
2. **Restore from backup** to a known good state
3. **Replay migrations** from the restore point
4. **Validate data integrity** before resuming operations

### Migration Table Corruption
1. **Export migration history** if possible:
   ```sql
   .output migration_backup.sql
   SELECT * FROM __switchy_migrations;
   ```
2. **Recreate migration table** using switchy-migrate
3. **Manually restore migration records** from backup
4. **Verify schema matches expected state**

### Schema Drift (Manual Changes)
1. **Document all manual changes** made outside migration system
2. **Create corrective migrations** to align schema
3. **Mark problematic migrations as completed** if they're no longer relevant
4. **Establish processes** to prevent future schema drift

## Related Documentation

- [README.md](./README.md) - Main switchy_schema documentation
- [CLI README](./cli/README.md) - Command-line interface documentation
- [Test Utils README](./test_utils/README.md) - Testing utilities for migrations

For additional help or complex recovery scenarios, consider consulting the switchy_schema source code or opening an issue in the project repository.
