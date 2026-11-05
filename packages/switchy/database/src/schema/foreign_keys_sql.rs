//! Catalog SQL for [`Database::list_foreign_keys`](crate::Database::list_foreign_keys):
//! every foreign key from or to one table, or from or to any table of a
//! schema, with every column of composite keys and the schema of both sides.
//!
//! Each backend's query returns, one row per column pair: `name`,
//! `table_schema`, `table_name`, `column_name`, `referenced_schema`,
//! `referenced_table`, `referenced_column`, `position` (1-based within the
//! key), `on_update` and `on_delete`, all as text. Rows of one key share
//! `table_schema`, `table_name` and `name`; [`foreign_keys_from_rows`] groups
//! them into [`TableForeignKey`]s. Values (schema and table names) are bound
//! as parameters, except where `SQLite` needs the schema as a name.

use crate::DatabaseValue;

use super::{ForeignKeyInfo, ForeignKeyScope, RelationRef, TableForeignKey};

/// A query and its parameters
pub type Query = (String, Vec<DatabaseValue>);

/// The scope as `(schema, table)`
fn parts(scope: &ForeignKeyScope) -> (Option<&str>, Option<&str>) {
    match scope {
        ForeignKeyScope::Relation(relation) => {
            (relation.schema.as_deref(), Some(relation.name.as_str()))
        }
        ForeignKeyScope::Schema(schema) => (schema.as_deref(), None),
    }
}

/// Bind `value` as the next parameter
fn bind(params: &mut Vec<DatabaseValue>, value: &str) -> usize {
    params.push(DatabaseValue::String(value.to_string()));
    params.len()
}

/// `PostgreSQL` (and wire-compatible servers): `pg_constraint`, across
/// schemas
#[must_use]
pub fn postgres(scope: &ForeignKeyScope) -> Query {
    let (schema, table) = parts(scope);
    let mut params = Vec::new();
    // Numbered placeholders: one parameter serves both directions.
    let schema = schema.map_or_else(
        || "current_schema()".to_string(),
        |schema| format!("${}::text", bind(&mut params, schema)),
    );
    let filter = table.map_or_else(
        || format!("n.nspname::text = {schema} OR rn.nspname::text = {schema}"),
        |table| {
            let name = format!("${}::text", bind(&mut params, table));
            format!(
                "(n.nspname::text = {schema} AND c.relname::text = {name}) \
                 OR (rn.nspname::text = {schema} AND rc.relname::text = {name})"
            )
        },
    );
    let action = |column: &str| {
        format!(
            "CASE con.{column} WHEN 'a' THEN 'NO ACTION' WHEN 'r' THEN 'RESTRICT' \
             WHEN 'c' THEN 'CASCADE' WHEN 'n' THEN 'SET NULL' WHEN 'd' THEN 'SET DEFAULT' END"
        )
    };
    let sql = format!(
        "SELECT con.conname::text, n.nspname::text, c.relname::text, a.attname::text, \
         rn.nspname::text, rc.relname::text, ra.attname::text, k.ord::text, {}, {} \
         FROM pg_catalog.pg_constraint con \
         CROSS JOIN LATERAL unnest(con.conkey, con.confkey) \
         WITH ORDINALITY AS k(att, ref_att, ord) \
         JOIN pg_catalog.pg_class c ON c.oid = con.conrelid \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_attribute a ON a.attrelid = con.conrelid AND a.attnum = k.att \
         JOIN pg_catalog.pg_class rc ON rc.oid = con.confrelid \
         JOIN pg_catalog.pg_namespace rn ON rn.oid = rc.relnamespace \
         JOIN pg_catalog.pg_attribute ra ON ra.attrelid = con.confrelid \
         AND ra.attnum = k.ref_att \
         WHERE con.contype = 'f' AND ({filter}) \
         ORDER BY 2, 3, 1, k.ord",
        action("confupdtype"),
        action("confdeltype"),
    );
    (sql, params)
}

/// `MySQL`/`MariaDB`: `information_schema.KEY_COLUMN_USAGE`, across
/// databases
#[must_use]
pub fn mysql(scope: &ForeignKeyScope) -> Query {
    let (schema, table) = parts(scope);
    let mut params = Vec::new();
    // `?` placeholders bind in order: values are repeated.
    let schema_sql = |params: &mut Vec<DatabaseValue>| {
        schema.map_or_else(
            || "DATABASE()".to_string(),
            |schema| {
                bind(params, schema);
                "?".to_string()
            },
        )
    };
    // One direction, then the other: `? = ...` binds in the order written.
    let mut side = |schema_column: &str, table_column: &str| {
        let schema = schema_sql(&mut params);
        table.map_or_else(
            || format!("k.{schema_column} = {schema}"),
            |table| {
                bind(&mut params, table);
                format!("(k.{schema_column} = {schema} AND k.{table_column} = ?)")
            },
        )
    };
    let referencing = side("TABLE_SCHEMA", "TABLE_NAME");
    let referenced = side("REFERENCED_TABLE_SCHEMA", "REFERENCED_TABLE_NAME");
    let filter = format!("{referencing} OR {referenced}");
    let sql = format!(
        "SELECT CAST(k.CONSTRAINT_NAME AS CHAR), CAST(k.TABLE_SCHEMA AS CHAR), \
         CAST(k.TABLE_NAME AS CHAR), CAST(k.COLUMN_NAME AS CHAR), \
         CAST(k.REFERENCED_TABLE_SCHEMA AS CHAR), CAST(k.REFERENCED_TABLE_NAME AS CHAR), \
         CAST(k.REFERENCED_COLUMN_NAME AS CHAR), CAST(k.ORDINAL_POSITION AS CHAR), \
         CAST(rc.UPDATE_RULE AS CHAR), CAST(rc.DELETE_RULE AS CHAR) \
         FROM information_schema.KEY_COLUMN_USAGE k \
         LEFT JOIN information_schema.REFERENTIAL_CONSTRAINTS rc \
         ON rc.CONSTRAINT_SCHEMA = k.CONSTRAINT_SCHEMA \
         AND rc.CONSTRAINT_NAME = k.CONSTRAINT_NAME AND rc.TABLE_NAME = k.TABLE_NAME \
         WHERE k.REFERENCED_TABLE_NAME IS NOT NULL AND ({filter}) \
         ORDER BY 2, 3, 1, k.ORDINAL_POSITION"
    );
    (sql, params)
}

/// `SQLite` (and libSQL): `pragma_foreign_key_list` of every table of
/// one attached database (keys never leave their database)
///
/// Unnamed keys are named `{table}_{column}_{referenced_table}_{referenced_column}` after
/// their first column pair, as table introspection names them; keys that
/// reference a primary key implicitly report its columns.
#[must_use]
pub fn sqlite(scope: &ForeignKeyScope) -> Query {
    let (schema, table) = parts(scope);
    let schema = schema.unwrap_or("main");
    // Pragmas take the schema as a value, the catalog as a name.
    let catalog = format!("\"{}\"", schema.replace('"', "\"\""));
    let literal = format!("'{}'", schema.replace('\'', "''"));
    let mut params = Vec::new();
    let filter = table.map_or_else(String::new, |table| {
        bind(&mut params, table);
        bind(&mut params, table);
        " AND (m.name = ? OR r.name = ?)".to_string()
    });
    let referenced_column = format!(
        "COALESCE(p.\"to\", (SELECT k.name FROM pragma_table_info(r.name, {literal}) k \
         WHERE k.pk = p.seq + 1))"
    );
    let sql = format!(
        "SELECT (SELECT m.name || '_' || f.\"from\" || '_' || f.\"table\" || '_' || \
         COALESCE(f.\"to\", (SELECT k.name FROM pragma_table_info(r.name, {literal}) k \
         WHERE k.pk = 1)) FROM pragma_foreign_key_list(m.name, {literal}) f \
         WHERE f.id = p.id AND f.seq = 0), \
         {literal}, m.name, p.\"from\", {literal}, r.name, {referenced_column}, \
         CAST(p.seq + 1 AS TEXT), p.on_update, p.on_delete \
         FROM {catalog}.sqlite_master m \
         JOIN pragma_foreign_key_list(m.name, {literal}) p \
         JOIN {catalog}.sqlite_master r ON r.type = 'table' \
         AND r.name = p.\"table\" COLLATE NOCASE \
         WHERE m.type = 'table'{filter} \
         ORDER BY 3, p.id, p.seq"
    );
    (sql, params)
}

/// `DuckDB`: `duckdb_constraints()` of the current database (keys
/// reference tables in their own schema; referential actions aren't
/// reported)
#[must_use]
pub fn duckdb(scope: &ForeignKeyScope) -> Query {
    let (schema, table) = parts(scope);
    let mut params = Vec::new();
    let schema = schema.map_or_else(
        || "current_schema()".to_string(),
        |schema| {
            bind(&mut params, schema);
            "?".to_string()
        },
    );
    let filter = table.map_or_else(String::new, |table| {
        bind(&mut params, table);
        bind(&mut params, table);
        " AND (c.table_name = ? OR t.table_name = ?)".to_string()
    });
    let sql = format!(
        "SELECT c.constraint_name, c.schema_name, c.table_name, \
         unnest(c.constraint_column_names), c.schema_name, t.table_name, \
         unnest(c.referenced_column_names), \
         CAST(unnest(generate_series(1, len(c.constraint_column_names))) AS VARCHAR), \
         CAST(NULL AS VARCHAR), CAST(NULL AS VARCHAR) \
         FROM duckdb_constraints() c \
         JOIN duckdb_tables() t ON t.database_name = c.database_name \
         AND t.schema_name = c.schema_name \
         AND lower(t.table_name) = lower(c.referenced_table) \
         WHERE c.constraint_type = 'FOREIGN KEY' \
         AND c.database_name = current_database() AND c.schema_name = {schema}{filter} \
         ORDER BY 2, 3, 1"
    );
    (sql, params)
}

/// SQL Server: `sys.foreign_keys` of the current database, across schemas
#[must_use]
pub fn mssql(scope: &ForeignKeyScope) -> Query {
    let (schema, table) = parts(scope);
    let mut params = Vec::new();
    // Numbered placeholders: one parameter serves both directions.
    let schema = schema.map_or_else(
        || "SCHEMA_NAME()".to_string(),
        |schema| format!("@P{}", bind(&mut params, schema)),
    );
    let filter = table.map_or_else(
        || format!("ps.name = {schema} OR rs.name = {schema}"),
        |table| {
            let name = format!("@P{}", bind(&mut params, table));
            format!(
                "(ps.name = {schema} AND pt.name = {name}) \
                 OR (rs.name = {schema} AND rt.name = {name})"
            )
        },
    );
    let sql = format!(
        "SELECT fk.name, ps.name, pt.name, pc.name, rs.name, rt.name, rc.name, \
         CAST(fkc.constraint_column_id AS nvarchar(10)), \
         REPLACE(fk.update_referential_action_desc, '_', ' '), \
         REPLACE(fk.delete_referential_action_desc, '_', ' ') \
         FROM sys.foreign_keys fk \
         JOIN sys.foreign_key_columns fkc ON fkc.constraint_object_id = fk.object_id \
         JOIN sys.tables pt ON pt.object_id = fk.parent_object_id \
         JOIN sys.schemas ps ON ps.schema_id = pt.schema_id \
         JOIN sys.columns pc ON pc.object_id = fkc.parent_object_id \
         AND pc.column_id = fkc.parent_column_id \
         JOIN sys.tables rt ON rt.object_id = fk.referenced_object_id \
         JOIN sys.schemas rs ON rs.schema_id = rt.schema_id \
         JOIN sys.columns rc ON rc.object_id = fkc.referenced_object_id \
         AND rc.column_id = fkc.referenced_column_id \
         WHERE {filter} ORDER BY 2, 3, 1, fkc.constraint_column_id"
    );
    (sql, params)
}

/// Oracle: `ALL_CONSTRAINTS`, across schemas (Oracle has no `ON UPDATE`
/// actions)
#[must_use]
pub fn oracle(scope: &ForeignKeyScope) -> Query {
    let (schema, table) = parts(scope);
    let mut params = Vec::new();
    // Numbered placeholders: one parameter serves both directions.
    let schema = schema.map_or_else(
        || "SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA')".to_string(),
        |schema| format!(":{}", bind(&mut params, schema)),
    );
    let filter = table.map_or_else(
        || format!("c.owner = {schema} OR r.owner = {schema}"),
        |table| {
            let name = format!(":{}", bind(&mut params, table));
            format!(
                "(c.owner = {schema} AND c.table_name = {name}) \
                 OR (r.owner = {schema} AND r.table_name = {name})"
            )
        },
    );
    let sql = format!(
        "SELECT c.constraint_name, c.owner, c.table_name, cc.column_name, \
         r.owner, r.table_name, rc.column_name, TO_CHAR(cc.position), \
         CAST(NULL AS VARCHAR2(1)), c.delete_rule \
         FROM all_constraints c \
         JOIN all_cons_columns cc ON cc.owner = c.owner \
         AND cc.constraint_name = c.constraint_name \
         JOIN all_constraints r ON r.owner = c.r_owner \
         AND r.constraint_name = c.r_constraint_name \
         JOIN all_cons_columns rc ON rc.owner = r.owner \
         AND rc.constraint_name = r.constraint_name AND rc.position = cc.position \
         WHERE c.constraint_type = 'R' AND ({filter}) ORDER BY 2, 3, 1, cc.position"
    );
    (sql, params)
}

/// A text cell (numbers are rendered as text)
fn text(value: Option<&DatabaseValue>) -> Option<String> {
    use DatabaseValue as V;
    match value? {
        V::String(text) | V::StringOpt(Some(text)) | V::Other { text, .. } => Some(text.clone()),
        V::Bytes(bytes) | V::BytesOpt(Some(bytes)) => {
            Some(String::from_utf8_lossy(bytes).into_owned())
        }
        value => value.as_i64().map(|number| number.to_string()),
    }
}

/// A referential action as its SQL keywords (`set_null` -> `SET NULL`)
fn action(value: Option<&DatabaseValue>) -> Option<String> {
    text(value)
        .map(|action| action.trim().replace('_', " ").to_ascii_uppercase())
        .filter(|action| !action.is_empty())
}

/// Group rows of a query of this module into keys, every column pair in
/// key order, sorted by referencing schema, table and key name. Rows
/// without a name, table or columns are skipped.
#[must_use]
pub fn foreign_keys_from_rows(rows: &[Vec<DatabaseValue>]) -> Vec<TableForeignKey> {
    // (key, position, referencing column, referenced column)
    let mut parts: Vec<(TableForeignKey, u32, String, String)> = rows
        .iter()
        .filter_map(|row| {
            let cell = |index: usize| text(row.get(index)).filter(|text| !text.is_empty());
            let mut foreign_key = ForeignKeyInfo::new(cell(0)?, cell(5)?);
            foreign_key.referenced_schema = cell(4);
            foreign_key.on_update = action(row.get(8));
            foreign_key.on_delete = action(row.get(9));
            let position = cell(7)
                .and_then(|position| position.trim().parse().ok())
                .unwrap_or(0);
            Some((
                TableForeignKey {
                    table: RelationRef {
                        schema: cell(1),
                        name: cell(2)?,
                    },
                    foreign_key,
                },
                position,
                cell(3)?,
                cell(6)?,
            ))
        })
        .collect();
    parts.sort_by(|a, b| {
        (&a.0.table, &a.0.foreign_key.name, a.1).cmp(&(&b.0.table, &b.0.foreign_key.name, b.1))
    });
    let mut keys: Vec<TableForeignKey> = Vec::new();
    for (key, _, column, referenced_column) in parts {
        match keys.last_mut() {
            Some(last)
                if last.table == key.table && last.foreign_key.name == key.foreign_key.name =>
            {
                last.foreign_key.push_pair(column, referenced_column);
            }
            _ => {
                let mut key = key;
                key.foreign_key.push_pair(column, referenced_column);
                keys.push(key);
            }
        }
    }
    keys
}

/// Whether `found` (reported with its schema) is `wanted`, whose schema may
/// be left to the connection's default
#[must_use]
pub fn same_relation(found: &RelationRef, wanted: &RelationRef) -> bool {
    found.name == wanted.name
        && (wanted.schema.is_none() || found.schema.is_none() || found.schema == wanted.schema)
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn row(cells: [&str; 10]) -> Vec<DatabaseValue> {
        cells
            .iter()
            .map(|cell| {
                if cell.is_empty() {
                    DatabaseValue::Null
                } else {
                    DatabaseValue::String((*cell).to_string())
                }
            })
            .collect()
    }

    fn strings(values: &[DatabaseValue]) -> Vec<String> {
        values
            .iter()
            .map(|value| value.as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn composite_keys_group_in_key_order() {
        let rows = vec![
            row([
                "plays_fk",
                "shop",
                "track_plays",
                "track_no",
                "shop",
                "album_tracks",
                "track_no",
                "2",
                "NO ACTION",
                "cascade",
            ]),
            row([
                "tracks_fk",
                "shop",
                "album_tracks",
                "album_id",
                "media",
                "albums",
                "id",
                "1",
                "",
                "SET_NULL",
            ]),
            row([
                "plays_fk",
                "shop",
                "track_plays",
                "album_id",
                "shop",
                "album_tracks",
                "album_id",
                "1",
                "NO ACTION",
                "cascade",
            ]),
        ];
        let keys = foreign_keys_from_rows(&rows);
        assert_eq!(keys.len(), 2);
        let tracks = &keys[0];
        assert_eq!(
            tracks.table,
            RelationRef::with_schema("shop", "album_tracks")
        );
        assert_eq!(
            tracks.referenced(),
            RelationRef::with_schema("media", "albums")
        );
        assert_eq!(tracks.foreign_key.on_update, None);
        assert_eq!(tracks.foreign_key.on_delete.as_deref(), Some("SET NULL"));
        let plays = &keys[1].foreign_key;
        assert_eq!(plays.columns, vec!["album_id", "track_no"]);
        assert_eq!(plays.referenced_columns, vec!["album_id", "track_no"]);
        assert_eq!(
            (plays.column.as_str(), plays.referenced_column.as_str()),
            ("album_id", "album_id")
        );
        assert_eq!(plays.on_update.as_deref(), Some("NO ACTION"));
        assert_eq!(plays.on_delete.as_deref(), Some("CASCADE"));
        assert_eq!(
            plays.pairs().collect::<Vec<_>>(),
            vec![("album_id", "album_id"), ("track_no", "track_no")]
        );
    }

    #[test]
    fn parameters_follow_each_backends_placeholders() {
        let albums = ForeignKeyScope::Relation(RelationRef::with_schema("shop", "albums"));
        assert_eq!(strings(&postgres(&albums).1), vec!["shop", "albums"]);
        assert_eq!(
            strings(&mysql(&albums).1),
            vec!["shop", "albums", "shop", "albums"]
        );
        // SQLite names the schema in the statement (pragmas, catalogs).
        assert_eq!(strings(&sqlite(&albums).1), vec!["albums", "albums"]);
        assert_eq!(
            strings(&duckdb(&albums).1),
            vec!["shop", "albums", "albums"]
        );
        assert_eq!(strings(&mssql(&albums).1), vec!["shop", "albums"]);
        assert_eq!(strings(&oracle(&albums).1), vec!["shop", "albums"]);
        let (sql, _) = mssql(&albums);
        assert!(sql.contains("(ps.name = @P1 AND pt.name = @P2)"), "{sql}");
        let (sql, _) = oracle(&albums);
        assert!(
            sql.contains("(r.owner = :1 AND r.table_name = :2)"),
            "{sql}"
        );
    }

    #[test]
    fn the_default_schema_comes_from_the_connection() {
        let albums = ForeignKeyScope::Relation(RelationRef::new("albums"));
        let (sql, params) = mysql(&albums);
        assert_eq!(strings(&params), vec!["albums", "albums"]);
        assert!(sql.contains("k.TABLE_SCHEMA = DATABASE()"), "{sql}");
        let (sql, params) = postgres(&albums);
        assert_eq!(strings(&params), vec!["albums"]);
        assert!(sql.contains("n.nspname::text = current_schema()"), "{sql}");
        assert!(sql.contains("c.relname::text = $1::text"), "{sql}");
        let (sql, _) = sqlite(&albums);
        assert!(sql.contains("FROM \"main\".sqlite_master m"), "{sql}");
        let (sql, params) = mssql(&ForeignKeyScope::Schema(None));
        assert_eq!(strings(&params), Vec::<String>::new());
        assert!(
            sql.contains("ps.name = SCHEMA_NAME() OR rs.name = SCHEMA_NAME()"),
            "{sql}"
        );
    }

    #[test]
    fn schema_scopes_filter_by_schema_only() {
        let shop = ForeignKeyScope::Schema(Some("shop".to_string()));
        assert_eq!(strings(&postgres(&shop).1), vec!["shop"]);
        assert_eq!(strings(&mysql(&shop).1), vec!["shop", "shop"]);
        assert_eq!(strings(&sqlite(&shop).1), Vec::<String>::new());
        assert_eq!(strings(&duckdb(&shop).1), vec!["shop"]);
        let (sql, _) = sqlite(&ForeignKeyScope::Schema(None));
        assert!(sql.contains("WHERE m.type = 'table' ORDER BY"), "{sql}");
    }

    #[test]
    fn sqlite_schema_names_are_quoted_where_they_are_spliced() {
        let odd = ForeignKeyScope::Relation(RelationRef::with_schema("we\"ird'", "t"));
        let (sql, _) = sqlite(&odd);
        assert!(sql.contains("FROM \"we\"\"ird'\".sqlite_master"), "{sql}");
        assert!(sql.contains("'we\"ird'''"), "{sql}");
    }

    #[test]
    fn relations_match_with_or_without_a_schema() {
        let found = RelationRef::with_schema("shop", "albums");
        assert!(same_relation(&found, &RelationRef::new("albums")));
        assert!(same_relation(
            &found,
            &RelationRef::with_schema("shop", "albums")
        ));
        assert!(!same_relation(
            &found,
            &RelationRef::with_schema("media", "albums")
        ));
        assert!(!same_relation(&found, &RelationRef::new("tracks")));
    }
}
