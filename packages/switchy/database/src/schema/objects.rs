//! Catalog listings of schema objects besides relations and routines:
//! triggers, sequences, user-defined types, and roles.
//!
//! [`Database::list_triggers`](crate::Database::list_triggers),
//! [`Database::list_sequences`](crate::Database::list_sequences),
//! [`Database::list_user_types`](crate::Database::list_user_types) and
//! [`Database::list_roles`](crate::Database::list_roles) run the catalog
//! SQL built here and map its rows with the `*_from_rows` functions. Each
//! query is a single plain `SELECT` of the system catalog returning text
//! columns, so a read-only connection can run it; schema names are written
//! as quoted string literals (or a quoted identifier for `SQLite`'s
//! attached databases), never spliced in raw.
//!
//! A schema of `None` means the connection's default schema, as in
//! [`Database::list_relations`](crate::Database::list_relations): the
//! search path (`PostgreSQL`, without `pg_catalog`), the current database
//! (`MySQL`), `main` (`SQLite`), `current_schema()` (`DuckDB`),
//! `SCHEMA_NAME()` (SQL Server), or the current schema (Oracle).

use crate::DatabaseValue;

/// A trigger returned by
/// [`Database::list_triggers`](crate::Database::list_triggers)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerInfo {
    /// Schema containing the trigger's table, where the backend has schemas
    pub schema: Option<String>,
    /// Trigger name
    pub name: String,
    /// The table (or view) the trigger belongs to; `None` for triggers on a
    /// schema or database (Oracle's DDL and event triggers)
    pub table: Option<String>,
    /// When it fires relative to the statement, uppercase: `BEFORE`,
    /// `AFTER`, `INSTEAD OF`, or a backend-specific timing (`COMPOUND`)
    pub timing: Option<String>,
    /// The events it fires on, uppercase (`INSERT`, `UPDATE`, `DELETE`,
    /// `TRUNCATE`, ...), in the catalog's order
    pub events: Vec<String>,
    /// Whether it is enabled
    pub enabled: bool,
    /// The full `CREATE TRIGGER` statement, where the catalog keeps one
    /// (`PostgreSQL`, `SQLite`, SQL Server)
    pub definition: Option<String>,
}

/// A sequence returned by
/// [`Database::list_sequences`](crate::Database::list_sequences)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SequenceInfo {
    /// Schema containing the sequence
    pub schema: Option<String>,
    /// Sequence name
    pub name: String,
    /// Its value type as the database renders it (`bigint`, `NUMBER`),
    /// where the catalog reports one
    pub data_type: Option<String>,
}

/// Kind of type returned by
/// [`Database::list_user_types`](crate::Database::list_user_types)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum UserTypeKind {
    /// An enumeration of labels
    Enum,
    /// A domain: another type with constraints
    Domain,
    /// A composite type of named attributes (`PostgreSQL` composite types,
    /// `DuckDB` structs, Oracle object types)
    Composite,
    /// A range type
    Range,
    /// A base type with its own input and output functions (`PostgreSQL`
    /// extension types)
    Base,
    /// A backend-specific kind, lowercase (`alias`, `table`, `clr` on SQL
    /// Server; `collection` on Oracle; `DuckDB`'s logical type name)
    Other(String),
}

impl UserTypeKind {
    /// Interpret a catalog kind code or name (`e`/`enum`, `d`/`domain`,
    /// `c`/`composite`/`struct`/`object`, `r`/`range`, `b`/`base`, ...)
    #[must_use]
    pub fn from_catalog(kind: &str) -> Self {
        let kind = kind.trim().to_ascii_lowercase();
        match kind.as_str() {
            "e" | "enum" => Self::Enum,
            "d" | "domain" => Self::Domain,
            "c" | "composite" | "struct" | "object" => Self::Composite,
            "r" | "range" => Self::Range,
            "b" | "base" => Self::Base,
            _ => Self::Other(kind),
        }
    }
}

/// A user-defined type returned by
/// [`Database::list_user_types`](crate::Database::list_user_types)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserTypeInfo {
    /// Schema containing the type
    pub schema: Option<String>,
    /// Type name
    pub name: String,
    /// Kind of type
    pub kind: UserTypeKind,
}

/// A role or user returned by
/// [`Database::list_roles`](crate::Database::list_roles)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleInfo {
    /// Role or user name as the database renders it (`MySQL` grantees are
    /// `'user'@'host'`)
    pub name: String,
    /// Whether it bypasses permission checks, where the catalog says
    pub superuser: Option<bool>,
    /// Whether it can log in (a user rather than a group role), where the
    /// catalog says
    pub can_login: Option<bool>,
    /// Whether the database provides it (rather than a user)
    pub builtin: bool,
}

/// A string literal.
fn literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// A SQL Server Unicode string literal.
fn nliteral(value: &str) -> String {
    format!("N'{}'", value.replace('\'', "''"))
}

/// `column = 'schema'`, or `column = default` without a schema.
fn equals(column: &str, schema: Option<&str>, default: &str) -> String {
    format!(
        "{column} = {}",
        schema.map_or_else(|| default.to_string(), literal)
    )
}

/// `PostgreSQL`: `n.nspname` is `schema`, or on the search path.
fn pg_schema(schema: Option<&str>) -> String {
    schema.map_or_else(
        || "n.nspname = ANY (pg_catalog.current_schemas(false))".to_string(),
        |schema| format!("n.nspname = {}", literal(schema)),
    )
}

/// SQL Server: `s.name` is `schema`, or the default schema.
fn mssql_schema(schema: Option<&str>) -> String {
    format!(
        "s.name = {}",
        schema.map_or_else(|| "SCHEMA_NAME()".to_string(), nliteral)
    )
}

/// Oracle: `column` is `schema`, or the current schema.
fn oracle_schema(column: &str, schema: Option<&str>) -> String {
    equals(column, schema, "SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')")
}

/// Catalog SQL for [`Database::list_triggers`](crate::Database::list_triggers),
/// returning `schema, name, table, timing, events, enabled, definition` as
/// text, with `events` joined by ` OR ` and `enabled` as `'true'` or
/// `'false'`
pub mod triggers_sql {
    use super::{equals, literal, mssql_schema, oracle_schema, pg_schema};

    /// `PostgreSQL`: `pg_trigger`, without internal (constraint) triggers
    #[must_use]
    pub fn postgres(schema: Option<&str>) -> String {
        format!(
            "SELECT n.nspname::text, t.tgname::text, c.relname::text, \
             (CASE WHEN t.tgtype & 2 <> 0 THEN 'BEFORE' WHEN t.tgtype & 64 <> 0 \
             THEN 'INSTEAD OF' ELSE 'AFTER' END)::text, \
             concat_ws(' OR ', CASE WHEN t.tgtype & 4 <> 0 THEN 'INSERT' END, \
             CASE WHEN t.tgtype & 16 <> 0 THEN 'UPDATE' END, \
             CASE WHEN t.tgtype & 8 <> 0 THEN 'DELETE' END, \
             CASE WHEN t.tgtype & 32 <> 0 THEN 'TRUNCATE' END)::text, \
             CASE WHEN t.tgenabled = 'D' THEN 'false' ELSE 'true' END, \
             pg_catalog.pg_get_triggerdef(t.oid)::text \
             FROM pg_catalog.pg_trigger t \
             JOIN pg_catalog.pg_class c ON c.oid = t.tgrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE {} AND NOT t.tgisinternal ORDER BY 2, 3",
            pg_schema(schema)
        )
    }

    /// `MySQL`/`MariaDB`: `information_schema.TRIGGERS` of `schema` or the
    /// current database
    #[must_use]
    pub fn mysql(schema: Option<&str>) -> String {
        format!(
            "SELECT TRIGGER_SCHEMA, TRIGGER_NAME, EVENT_OBJECT_TABLE, ACTION_TIMING, \
             EVENT_MANIPULATION, 'true', NULL FROM information_schema.TRIGGERS \
             WHERE {} ORDER BY TRIGGER_NAME",
            equals("TRIGGER_SCHEMA", schema, "DATABASE()")
        )
    }

    /// `SQLite`: the triggers of the attached database `schema` (`main`
    /// without one); timing and events are read from each statement
    #[must_use]
    pub fn sqlite(schema: Option<&str>) -> String {
        let schema = schema.unwrap_or("main");
        format!(
            "SELECT {}, name, tbl_name, NULL, NULL, 'true', sql \
             FROM \"{}\".sqlite_master WHERE type = 'trigger' ORDER BY name",
            literal(schema),
            schema.replace('"', "\"\"")
        )
    }

    /// SQL Server: DML triggers on tables and views (`sys.triggers`)
    #[must_use]
    pub fn mssql(schema: Option<&str>) -> String {
        format!(
            "SELECT s.name, tr.name, o.name, \
             CASE WHEN tr.is_instead_of_trigger = 1 THEN 'INSTEAD OF' ELSE 'AFTER' END, \
             STUFF((SELECT ' OR ' + te.type_desc FROM sys.trigger_events te \
             WHERE te.object_id = tr.object_id ORDER BY te.type \
             FOR XML PATH(''), TYPE).value('.', 'nvarchar(max)'), 1, 4, ''), \
             CASE WHEN tr.is_disabled = 1 THEN 'false' ELSE 'true' END, \
             OBJECT_DEFINITION(tr.object_id) \
             FROM sys.triggers tr JOIN sys.objects o ON o.object_id = tr.parent_id \
             JOIN sys.schemas s ON s.schema_id = o.schema_id \
             WHERE tr.parent_class = 1 AND {} ORDER BY tr.name, o.name",
            mssql_schema(schema)
        )
    }

    /// Oracle: `ALL_TRIGGERS` owned by `schema` or the current schema
    #[must_use]
    pub fn oracle(schema: Option<&str>) -> String {
        format!(
            "SELECT owner, trigger_name, table_name, \
             CASE WHEN trigger_type LIKE 'BEFORE%' THEN 'BEFORE' \
             WHEN trigger_type LIKE 'AFTER%' THEN 'AFTER' \
             WHEN trigger_type LIKE 'INSTEAD OF%' THEN 'INSTEAD OF' \
             ELSE trigger_type END, triggering_event, \
             CASE WHEN status = 'ENABLED' THEN 'true' ELSE 'false' END, NULL \
             FROM all_triggers WHERE {} ORDER BY trigger_name, table_name",
            oracle_schema("owner", schema)
        )
    }
}

/// Catalog SQL for
/// [`Database::list_sequences`](crate::Database::list_sequences),
/// returning `schema, name, data_type` as text
pub mod sequences_sql {
    use super::{equals, mssql_schema, oracle_schema, pg_schema};

    /// `PostgreSQL`: `pg_sequence`
    #[must_use]
    pub fn postgres(schema: Option<&str>) -> String {
        format!(
            "SELECT n.nspname::text, c.relname::text, \
             pg_catalog.format_type(s.seqtypid, NULL)::text \
             FROM pg_catalog.pg_sequence s \
             JOIN pg_catalog.pg_class c ON c.oid = s.seqrelid \
             JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
             WHERE {} ORDER BY 2",
            pg_schema(schema)
        )
    }

    /// `MariaDB` (and `TiDB`): sequences are tables of type `SEQUENCE` in
    /// `information_schema.TABLES`; `MySQL` has none
    #[must_use]
    pub fn mysql(schema: Option<&str>) -> String {
        format!(
            "SELECT TABLE_SCHEMA, TABLE_NAME, NULL FROM information_schema.TABLES \
             WHERE TABLE_TYPE = 'SEQUENCE' AND {} ORDER BY TABLE_NAME",
            equals("TABLE_SCHEMA", schema, "DATABASE()")
        )
    }

    /// `DuckDB`: `duckdb_sequences()` of the current database
    #[must_use]
    pub fn duckdb(schema: Option<&str>) -> String {
        format!(
            "SELECT schema_name, sequence_name, NULL FROM duckdb_sequences() \
             WHERE database_name = current_database() AND {} ORDER BY sequence_name",
            equals("schema_name", schema, "current_schema()")
        )
    }

    /// SQL Server: `sys.sequences`
    #[must_use]
    pub fn mssql(schema: Option<&str>) -> String {
        format!(
            "SELECT s.name, sq.name, TYPE_NAME(sq.user_type_id) FROM sys.sequences sq \
             JOIN sys.schemas s ON s.schema_id = sq.schema_id WHERE {} ORDER BY sq.name",
            mssql_schema(schema)
        )
    }

    /// Oracle: `ALL_SEQUENCES` (Oracle sequences are `NUMBER`s)
    #[must_use]
    pub fn oracle(schema: Option<&str>) -> String {
        format!(
            "SELECT sequence_owner, sequence_name, 'NUMBER' FROM all_sequences \
             WHERE {} ORDER BY sequence_name",
            oracle_schema("sequence_owner", schema)
        )
    }
}

/// Catalog SQL for
/// [`Database::list_user_types`](crate::Database::list_user_types),
/// returning `schema, name, kind` as text (see
/// [`UserTypeKind::from_catalog`](super::UserTypeKind::from_catalog))
pub mod user_types_sql {
    use super::{equals, mssql_schema, oracle_schema, pg_schema};

    /// `PostgreSQL`: enums, domains, ranges, standalone composite types and
    /// base types (not array types, multiranges, or the row types of
    /// tables)
    #[must_use]
    pub fn postgres(schema: Option<&str>) -> String {
        format!(
            "SELECT n.nspname::text, t.typname::text, t.typtype::text \
             FROM pg_catalog.pg_type t \
             JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace \
             LEFT JOIN pg_catalog.pg_class c ON c.oid = t.typrelid \
             WHERE {} AND (t.typtype IN ('e', 'd', 'r') \
             OR (t.typtype = 'c' AND c.relkind = 'c') \
             OR (t.typtype = 'b' AND t.typcategory <> 'A')) \
             ORDER BY 2",
            pg_schema(schema)
        )
    }

    /// `DuckDB`: `duckdb_types()` of the current database, without
    /// built-in types
    #[must_use]
    pub fn duckdb(schema: Option<&str>) -> String {
        format!(
            "SELECT schema_name, type_name, lower(logical_type) FROM duckdb_types() \
             WHERE database_name = current_database() AND NOT internal AND {} \
             ORDER BY type_name",
            equals("schema_name", schema, "current_schema()")
        )
    }

    /// SQL Server: alias, table, and CLR types (`sys.types`)
    #[must_use]
    pub fn mssql(schema: Option<&str>) -> String {
        format!(
            "SELECT s.name, t.name, CASE WHEN t.is_table_type = 1 THEN 'table' \
             WHEN t.is_assembly_type = 1 THEN 'clr' ELSE 'alias' END FROM sys.types t \
             JOIN sys.schemas s ON s.schema_id = t.schema_id \
             WHERE t.is_user_defined = 1 AND {} ORDER BY t.name",
            mssql_schema(schema)
        )
    }

    /// Oracle: `ALL_TYPES` owned by `schema` or the current schema
    #[must_use]
    pub fn oracle(schema: Option<&str>) -> String {
        format!(
            "SELECT owner, type_name, LOWER(typecode) FROM all_types WHERE {} \
             ORDER BY type_name",
            oracle_schema("owner", schema)
        )
    }
}

/// Catalog SQL for [`Database::list_roles`](crate::Database::list_roles),
/// returning `name, superuser, can_login, builtin` as text, with the flags
/// as `'true'`, `'false'` or `NULL` (unknown)
pub mod roles_sql {
    /// `PostgreSQL`: `pg_roles`; the `pg_*` roles are built in
    #[must_use]
    pub fn postgres() -> String {
        "SELECT rolname::text, CASE WHEN rolsuper THEN 'true' ELSE 'false' END, \
         CASE WHEN rolcanlogin THEN 'true' ELSE 'false' END, \
         CASE WHEN left(rolname, 3) = 'pg_' THEN 'true' ELSE 'false' END \
         FROM pg_catalog.pg_roles ORDER BY 1"
            .to_string()
    }

    /// `MySQL`/`MariaDB`: the grantees the current user may see
    ///
    /// They come from `information_schema.USER_PRIVILEGES` (`mysql.user`
    /// needs privileges most accounts lack). A grantee with `SUPER` is a
    /// superuser; the
    /// `mysql.*` and `mariadb.sys` accounts are built in.
    #[must_use]
    pub fn mysql() -> String {
        "SELECT GRANTEE, \
         CASE WHEN MAX(PRIVILEGE_TYPE = 'SUPER') = 1 THEN 'true' ELSE 'false' END, NULL, \
         CASE WHEN GRANTEE LIKE '''mysql.%' OR GRANTEE LIKE '''mariadb.sys''%' \
         THEN 'true' ELSE 'false' END \
         FROM information_schema.USER_PRIVILEGES GROUP BY GRANTEE ORDER BY GRANTEE"
            .to_string()
    }

    /// SQL Server: the current database's principals (users and roles)
    ///
    /// Server logins need server permissions to list. Fixed roles,
    /// `public`, `dbo`, `guest`, `sys`, `INFORMATION_SCHEMA` and the
    /// `##...##` certificate users are built in.
    #[must_use]
    pub fn mssql() -> String {
        "SELECT p.name, NULL, CASE WHEN p.type IN ('R', 'A') THEN 'false' ELSE 'true' END, \
         CASE WHEN p.is_fixed_role = 1 OR p.principal_id < 5 OR p.name LIKE '##%' \
         THEN 'true' ELSE 'false' END \
         FROM sys.database_principals p \
         WHERE p.type IN ('R', 'A', 'S', 'U', 'G', 'E', 'X', 'C', 'K') ORDER BY p.name"
            .to_string()
    }

    /// Oracle: the roles granted to the current user (Oracle lists every
    /// role only to administrators) and the users in `ALL_USERS`, whose
    /// Oracle-maintained accounts are built in
    #[must_use]
    pub fn oracle() -> String {
        "SELECT granted_role, NULL, 'false', 'false' FROM user_role_privs \
         UNION ALL SELECT username, NULL, 'true', \
         CASE WHEN oracle_maintained = 'Y' THEN 'true' ELSE 'false' END FROM all_users \
         ORDER BY 1"
            .to_string()
    }
}

/// A catalog cell as text: some drivers return catalog strings as bytes.
fn text(value: Option<&DatabaseValue>) -> Option<String> {
    use crate::DatabaseValue as V;
    match value? {
        V::String(text) | V::StringOpt(Some(text)) | V::Other { text, .. } => Some(text.clone()),
        V::Bytes(bytes) | V::BytesOpt(Some(bytes)) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        _ => None,
    }
}

/// A non-empty text cell.
fn present(value: Option<&DatabaseValue>) -> Option<String> {
    text(value).filter(|text| !text.trim().is_empty())
}

/// A `'true'`/`'false'` cell; `None` for `NULL` or anything else.
fn flag(value: Option<&DatabaseValue>) -> Option<bool> {
    match text(value)?.as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// The words of a statement, keeping quoted names (`"a b"`, `` `a b` ``,
/// `[a b]`, `'a b'`) whole.
fn words(sql: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut chars = sql.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == '(' || c == ')' || c == ';' {
            chars.next();
            continue;
        }
        let mut word = String::new();
        let close = match c {
            '"' => Some('"'),
            '`' => Some('`'),
            '[' => Some(']'),
            '\'' => Some('\''),
            _ => None,
        };
        if let Some(close) = close {
            word.push(c);
            chars.next();
            for c in chars.by_ref() {
                word.push(c);
                if c == close {
                    break;
                }
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '(' || c == ')' || c == ';' {
                    break;
                }
                word.push(c);
                chars.next();
            }
        }
        words.push(word);
    }
    words
}

/// The timing and events of a `SQLite` `CREATE TRIGGER` statement:
/// `BEFORE` when it names no timing, as `SQLite` does.
#[must_use]
pub fn sqlite_trigger_timing(sql: &str) -> (Option<String>, Vec<String>) {
    let words = words(sql);
    let upper: Vec<String> = words.iter().map(|word| word.to_ascii_uppercase()).collect();
    let Some(mut index) = upper.iter().position(|word| word == "TRIGGER") else {
        return (None, Vec::new());
    };
    index += 1;
    if upper.get(index..index + 3) == Some(&["IF".into(), "NOT".into(), "EXISTS".into()]) {
        index += 3;
    }
    // The (possibly qualified) name.
    index += 1;
    let mut timing = None;
    let mut events = Vec::new();
    for (position, word) in upper.iter().enumerate().skip(index) {
        match word.as_str() {
            "ON" => break,
            "BEFORE" | "AFTER" => timing = Some(word.clone()),
            "INSTEAD" if upper.get(position + 1).is_some_and(|next| next == "OF") => {
                timing = Some("INSTEAD OF".to_string());
            }
            "INSERT" | "UPDATE" | "DELETE" => events.push(word.clone()),
            _ => {}
        }
    }
    if events.is_empty() {
        return (None, events);
    }
    (timing.or_else(|| Some("BEFORE".to_string())), events)
}

/// Map rows of a [`triggers_sql`] query to [`TriggerInfo`]s, skipping rows
/// without a name. Rows without timing or events take them from the
/// definition ([`sqlite_trigger_timing`]).
#[must_use]
pub fn triggers_from_rows(rows: &[Vec<DatabaseValue>]) -> Vec<TriggerInfo> {
    rows.iter()
        .filter_map(|row| {
            let definition = present(row.get(6));
            let mut timing = present(row.get(3)).map(|timing| timing.trim().to_ascii_uppercase());
            let mut events: Vec<String> = present(row.get(4))
                .map(|events| {
                    events
                        .split(" OR ")
                        .map(|event| event.trim().to_ascii_uppercase())
                        .filter(|event| !event.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            if (timing.is_none() || events.is_empty())
                && let Some(definition) = &definition
            {
                let (parsed_timing, parsed_events) = sqlite_trigger_timing(definition);
                timing = timing.or(parsed_timing);
                if events.is_empty() {
                    events = parsed_events;
                }
            }
            Some(TriggerInfo {
                schema: present(row.first()),
                name: text(row.get(1))?,
                table: present(row.get(2)),
                timing,
                events,
                enabled: flag(row.get(5)).unwrap_or(true),
                definition,
            })
        })
        .collect()
}

/// Map rows of a [`sequences_sql`] query to [`SequenceInfo`]s, skipping
/// rows without a name
#[must_use]
pub fn sequences_from_rows(rows: &[Vec<DatabaseValue>]) -> Vec<SequenceInfo> {
    rows.iter()
        .filter_map(|row| {
            Some(SequenceInfo {
                schema: present(row.first()),
                name: text(row.get(1))?,
                data_type: present(row.get(2)),
            })
        })
        .collect()
}

/// Map rows of a [`user_types_sql`] query to [`UserTypeInfo`]s, skipping
/// rows without a name
#[must_use]
pub fn user_types_from_rows(rows: &[Vec<DatabaseValue>]) -> Vec<UserTypeInfo> {
    rows.iter()
        .filter_map(|row| {
            Some(UserTypeInfo {
                schema: present(row.first()),
                name: text(row.get(1))?,
                kind: UserTypeKind::from_catalog(&text(row.get(2)).unwrap_or_default()),
            })
        })
        .collect()
}

/// Map rows of a [`roles_sql`] query to [`RoleInfo`]s, skipping rows
/// without a name
#[must_use]
pub fn roles_from_rows(rows: &[Vec<DatabaseValue>]) -> Vec<RoleInfo> {
    rows.iter()
        .filter_map(|row| {
            Some(RoleInfo {
                name: text(row.first())?,
                superuser: flag(row.get(1)),
                can_login: flag(row.get(2)),
                builtin: flag(row.get(3)).unwrap_or(false),
            })
        })
        .collect()
}

/// Run a catalog query and map its rows (the backends' listing methods).
#[cfg(feature = "raw-sql")]
pub(crate) async fn list<D, T>(
    db: &D,
    sql: &str,
    map: fn(&[Vec<DatabaseValue>]) -> Vec<T>,
) -> Result<Vec<T>, crate::DatabaseError>
where
    D: crate::Database + ?Sized,
{
    Ok(map(&db.query_raw_result(sql, &[]).await?.rows))
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn s(value: &str) -> DatabaseValue {
        DatabaseValue::String(value.to_string())
    }

    #[test]
    fn user_type_kind_from_catalog() {
        for (code, kind) in [
            ("e", UserTypeKind::Enum),
            ("ENUM", UserTypeKind::Enum),
            ("d", UserTypeKind::Domain),
            ("c", UserTypeKind::Composite),
            ("struct", UserTypeKind::Composite),
            ("OBJECT", UserTypeKind::Composite),
            ("r", UserTypeKind::Range),
            ("b", UserTypeKind::Base),
            ("Alias", UserTypeKind::Other("alias".to_string())),
        ] {
            assert_eq!(UserTypeKind::from_catalog(code), kind, "{code}");
        }
    }

    #[test]
    fn sqlite_trigger_timing_is_read_from_the_statement() {
        assert_eq!(
            sqlite_trigger_timing(
                "CREATE TRIGGER IF NOT EXISTS \"after insert\" AFTER INSERT ON t BEGIN SELECT 1; END"
            ),
            (Some("AFTER".to_string()), vec!["INSERT".to_string()])
        );
        assert_eq!(
            sqlite_trigger_timing("create temp trigger x instead of update of a, b on v begin end"),
            (Some("INSTEAD OF".to_string()), vec!["UPDATE".to_string()])
        );
        // No timing means BEFORE.
        assert_eq!(
            sqlite_trigger_timing("CREATE TRIGGER main.audit DELETE ON t BEGIN SELECT 1; END"),
            (Some("BEFORE".to_string()), vec!["DELETE".to_string()])
        );
        assert_eq!(sqlite_trigger_timing("SELECT 1"), (None, Vec::new()));
    }

    #[test]
    fn rows_map_to_objects() {
        let triggers = triggers_from_rows(&[
            vec![
                s("public"),
                s("audit"),
                s("orders"),
                s("before"),
                s("INSERT OR UPDATE"),
                s("false"),
                DatabaseValue::Null,
            ],
            vec![
                s("main"),
                s("stamp"),
                s("orders"),
                DatabaseValue::Null,
                DatabaseValue::Null,
                s("true"),
                s("CREATE TRIGGER stamp AFTER UPDATE ON orders BEGIN SELECT 1; END"),
            ],
            vec![DatabaseValue::Null, DatabaseValue::Null],
        ]);
        assert_eq!(triggers.len(), 2);
        assert_eq!(triggers[0].timing.as_deref(), Some("BEFORE"));
        assert_eq!(triggers[0].events, vec!["INSERT", "UPDATE"]);
        assert!(!triggers[0].enabled);
        assert_eq!(triggers[1].timing.as_deref(), Some("AFTER"));
        assert_eq!(triggers[1].events, vec!["UPDATE"]);
        assert!(triggers[1].enabled && triggers[1].definition.is_some());

        let sequences = sequences_from_rows(&[vec![s("shop"), s("order_no"), s("")]]);
        assert_eq!(
            sequences,
            vec![SequenceInfo {
                schema: Some("shop".to_string()),
                name: "order_no".to_string(),
                data_type: None,
            }]
        );

        let types = user_types_from_rows(&[vec![
            DatabaseValue::Bytes(b"shop".to_vec()),
            s("mood"),
            s("e"),
        ]]);
        assert_eq!(types[0].schema.as_deref(), Some("shop"));
        assert_eq!(types[0].kind, UserTypeKind::Enum);

        let roles = roles_from_rows(&[
            vec![s("admin"), s("true"), s("true"), s("false")],
            vec![s("'app'@'%'"), s("false"), DatabaseValue::Null, s("false")],
        ]);
        assert_eq!(roles[0].superuser, Some(true));
        assert_eq!(roles[1].can_login, None);
        assert!(!roles[1].builtin);
    }

    #[test]
    fn schema_names_are_quoted() {
        let sql = triggers_sql::postgres(Some("it's"));
        assert!(sql.contains("n.nspname = 'it''s'"), "{sql}");
        assert!(triggers_sql::postgres(None).contains("current_schemas(false)"));
        let sql = triggers_sql::sqlite(Some("we\"ird"));
        assert!(sql.contains("FROM \"we\"\"ird\".sqlite_master"), "{sql}");
        assert!(sequences_sql::mssql(Some("o'k")).contains("s.name = N'o''k'"));
        assert!(sequences_sql::mssql(None).contains("s.name = SCHEMA_NAME()"));
        assert!(user_types_sql::oracle(None).contains("SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')"));
        assert!(sequences_sql::mysql(None).contains("TABLE_SCHEMA = DATABASE()"));
        assert!(user_types_sql::duckdb(Some("app")).contains("schema_name = 'app'"));
    }
}
