//! A conservative T-SQL tokenizer, the read-only statement check built on
//! it, and placeholder rewriting.
//!
//! The tokenizer understands exactly what it needs to find the words of a
//! statement: string literals (`'...'`, `N'...'`), quoted identifiers
//! (`[...]`, `"..."`), comments (`-- ...` and nested `/* ... */`),
//! variables (`@x`, `@@x`) and numbers. Anything it can't read to the end
//! (an unterminated string, identifier or comment) is an error, and the
//! read-only check refuses what it can't tokenize.
//!
//! It splits text where SQL Server does. A `--` comment ends at a carriage
//! return as well as a line feed. A number ends where T-SQL's number syntax
//! ends (`1.5e-3`, `0xFF`, `$1.50`), and the server then starts a new token
//! even without a space, so `1DELETE` is a number followed by `DELETE`;
//! a number running straight into a word is refused rather than read either
//! way.
//!
//! # The read-only check
//!
//! SQL Server sessions have no read-only switch, so a read-only
//! [`MssqlDatabase`](super::MssqlDatabase) runs every statement inside a
//! transaction that is always rolled back. A rollback can't undo everything:
//! a statement can end the transaction itself (`COMMIT`, a procedure that
//! commits, a trigger fired by a write), or do something no transaction
//! covers (`BACKUP`, `ALTER DATABASE`, `DBCC`, `KILL`, `xp_cmdshell`, linked
//! servers, advancing a sequence). The `@@TRANCOUNT` check after a
//! statement only detects that it ended the transaction (a commit has then
//! already happened), so this check is what prevents writes, and
//! [`check_read_only`] only lets through statements that are reads by
//! construction:
//!
//! * one statement (a `;` may only end it), starting with `SELECT`, `WITH`
//!   or `(`;
//! * containing none of the [`DENIED_WORDS`] anywhere outside literals,
//!   comments and quoted identifiers: every keyword that starts or makes a
//!   statement other than a query (`INSERT`, `INTO`, `EXEC`, `SET`,
//!   `DECLARE`, `BEGIN`, `COMMIT`, `WAITFOR`, ...) or reaches outside the
//!   database (`OPENROWSET`, `OPENQUERY`, `OPENDATASOURCE`, `BULK`);
//! * without `NEXT VALUE FOR` (sequences advance outside transactions) and
//!   without four-part names (`server.database.schema.object`, which reach
//!   linked servers);
//! * or `EXEC` of one of the read-only catalog procedures in
//!   [`ALLOWED_PROCEDURES`], unqualified or qualified by `sys` (another
//!   schema, such as `dbo.sp_who`, can hold a user procedure of the same
//!   name), with literal arguments only.
//!
//! Inside a transaction, `SET SHOWPLAN_XML ON` / `OFF` is also accepted: it
//! makes the server return estimated plans instead of running statements.
//!
//! The check is deliberately stricter than it needs to be: a word in the
//! list is refused even where it would be harmless (a bare column named
//! `receive`); bracketing the name (`[receive]`) makes it an identifier.

use std::fmt::Write as _;

/// A token's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A bare word: a keyword or an unquoted identifier (also `#temp`).
    Word,
    /// A quoted identifier: `[name]` or `"name"`.
    Quoted,
    /// A variable or parameter: `@name`, `@@name`.
    Variable,
    /// A string literal: `'...'` or `N'...'`.
    String,
    /// A number, binary (`0x..`) or money (`$1.50`) literal.
    Number,
    /// Any other character (`;`, `.`, `(`, `,`, operators, ...).
    Symbol,
}

/// A token and where it is in the statement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Token<'a> {
    /// What kind of token it is.
    pub kind: Kind,
    /// The token's text, as written (quotes included).
    pub text: &'a str,
    /// Byte offset of the token in the statement.
    pub start: usize,
}

impl Token<'_> {
    /// Whether this is the bare word `word` (case-insensitive).
    fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(word)
    }

    /// Whether this is the symbol `symbol`.
    fn is_symbol(&self, symbol: char) -> bool {
        self.kind == Kind::Symbol && self.text.starts_with(symbol)
    }

    /// Whether this token can be part of a dotted name.
    const fn is_name(&self) -> bool {
        matches!(self.kind, Kind::Word | Kind::Quoted)
    }
}

/// Characters that may continue a bare word.
fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '@' | '#' | '$')
}

/// Characters that may start a bare word.
fn word_start(ch: char) -> bool {
    ch.is_alphabetic() || matches!(ch, '_' | '#')
}

/// Split `sql` into tokens, skipping whitespace and comments.
///
/// # Errors
///
/// * A string literal, quoted identifier or block comment that isn't closed
pub(super) fn tokenize(sql: &str) -> Result<Vec<Token<'_>>, String> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut chars = sql.char_indices().peekable();
    while let Some(&(start, ch)) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        let next = bytes.get(start + ch.len_utf8()).copied();
        // Line comment.
        // SQL Server ends it at a carriage return as well as a line feed.
        if ch == '-' && next == Some(b'-') {
            for (_, c) in chars.by_ref() {
                if matches!(c, '\n' | '\r') {
                    break;
                }
            }
            continue;
        }
        // Block comments nest in T-SQL.
        if ch == '/' && next == Some(b'*') {
            chars.next();
            chars.next();
            let mut depth = 1_usize;
            while depth > 0 {
                let Some((index, c)) = chars.next() else {
                    return Err("a /* comment is not closed".to_string());
                };
                let following = bytes.get(index + 1).copied();
                if c == '/' && following == Some(b'*') {
                    chars.next();
                    depth += 1;
                } else if c == '*' && following == Some(b'/') {
                    chars.next();
                    depth -= 1;
                }
            }
            continue;
        }
        let (kind, end) = match ch {
            '\'' => (Kind::String, quoted_end(sql, start, '\'', '\'')?),
            '[' => (Kind::Quoted, quoted_end(sql, start, '[', ']')?),
            '"' => (Kind::Quoted, quoted_end(sql, start, '"', '"')?),
            'N' | 'n' if next == Some(b'\'') => {
                (Kind::String, quoted_end(sql, start + 1, '\'', '\'')?)
            }
            '@' => (Kind::Variable, scan(sql, start + 1, word_char)),
            '$' if next.is_some_and(|b| b.is_ascii_digit() || b == b'.') => {
                (Kind::Number, number_end(sql, start + 1)?)
            }
            '.' if next.is_some_and(|b| b.is_ascii_digit()) => {
                (Kind::Number, number_end(sql, start)?)
            }
            c if c.is_ascii_digit() => (Kind::Number, number_end(sql, start)?),
            c if word_start(c) || c == '$' => (Kind::Word, scan(sql, start + 1, word_char)),
            c => (Kind::Symbol, start + c.len_utf8()),
        };
        tokens.push(Token {
            kind,
            text: &sql[start..end],
            start,
        });
        while chars.peek().is_some_and(|&(index, _)| index < end) {
            chars.next();
        }
    }
    Ok(tokens)
}

/// The end of the number or binary literal at `start` (`1`, `1.`, `.5`,
/// `1.5e-3`, `1e`, `0xFF`), read as T-SQL reads it.
///
/// # Errors
///
/// * The literal runs straight into a word (`1DELETE`, `0x1UPDATE`): the
///   server would start a new token there, so it is refused rather than
///   guessed at
fn number_end(sql: &str, start: usize) -> Result<usize, String> {
    let bytes = sql.as_bytes();
    let digits = |mut at: usize, hex: bool| {
        while bytes.get(at).is_some_and(|b| {
            if hex {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        }) {
            at += 1;
        }
        at
    };
    let end =
        if bytes.get(start) == Some(&b'0') && matches!(bytes.get(start + 1), Some(b'x' | b'X')) {
            digits(start + 2, true)
        } else {
            let mut end = digits(start, false);
            if bytes.get(end) == Some(&b'.') {
                end = digits(end + 1, false);
            }
            if matches!(bytes.get(end), Some(b'e' | b'E')) {
                end += 1;
                if matches!(bytes.get(end), Some(b'+' | b'-')) {
                    end += 1;
                }
                end = digits(end, false);
            }
            end
        };
    if sql[end..].chars().next().is_some_and(word_char) {
        return Err(format!(
            "the number {:?} runs straight into a word",
            &sql[start..end]
        ));
    }
    Ok(end)
}

/// The end of a run of `continues` characters starting at `from`.
fn scan(sql: &str, from: usize, continues: fn(char) -> bool) -> usize {
    sql[from..]
        .char_indices()
        .find(|&(_, c)| !continues(c))
        .map_or(sql.len(), |(offset, _)| from + offset)
}

/// The end of a quoted token opened by `open` at `start`, closed by `close`
/// (a doubled `close` is an escaped one).
fn quoted_end(sql: &str, start: usize, open: char, close: char) -> Result<usize, String> {
    let body = start + open.len_utf8();
    let mut chars = sql[body..].char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        if c == close {
            if chars.peek().is_some_and(|&(_, next)| next == close) {
                chars.next();
                continue;
            }
            return Ok(body + offset + close.len_utf8());
        }
    }
    Err(match open {
        '\'' => "a string literal is not closed".to_string(),
        _ => format!("a {open}quoted{close} identifier is not closed"),
    })
}

/// Words refused anywhere in a statement on a read-only connection: they
/// start (or are part of) statements that write, change the session, run
/// code, control transactions, or reach outside the database.
pub(super) const DENIED_WORDS: &[&str] = &[
    // Data and schema changes.
    "INSERT",
    "UPDATE",
    "DELETE",
    "MERGE",
    "INTO",
    "TRUNCATE",
    "CREATE",
    "ALTER",
    "DROP",
    "WRITETEXT",
    "UPDATETEXT",
    "READTEXT",
    "TRIGGER",
    // Permissions and identity.
    "GRANT",
    "REVOKE",
    "DENY",
    "SETUSER",
    "REVERT",
    // Code: procedures, dynamic SQL, control flow.
    "EXEC",
    "EXECUTE",
    "PROCEDURE",
    "PROC",
    "FUNCTION",
    "DECLARE",
    "SET",
    "IF",
    "WHILE",
    "GOTO",
    "RETURN",
    "BREAK",
    "CONTINUE",
    "THROW",
    "RAISERROR",
    "PRINT",
    "WAITFOR",
    "GO",
    // Transactions.
    "BEGIN",
    "COMMIT",
    "ROLLBACK",
    "SAVE",
    "TRAN",
    "TRANSACTION",
    // Cursors and keys.
    "OPEN",
    "CLOSE",
    "DEALLOCATE",
    // Server and database administration.
    "USE",
    "BACKUP",
    "RESTORE",
    "DBCC",
    "KILL",
    "SHUTDOWN",
    "RECONFIGURE",
    "CHECKPOINT",
    "DUMP",
    "LOAD",
    "ADD",
    // Service Broker (RECEIVE removes messages from a queue).
    "RECEIVE",
    "CONVERSATION",
    // Reaching outside the database or the server.
    "BULK",
    "OPENROWSET",
    "OPENDATASOURCE",
    "OPENQUERY",
    "OPENXML",
];

/// Catalog procedures a read-only connection may `EXEC`.
///
/// They only read catalog views (some fill temporary tables, which the
/// rolled-back transaction discards). Procedures that can change anything — even
/// optionally, like `sp_spaceused @updateusage` — are not listed.
///
/// A read-only connection runs them only unqualified or qualified by `sys`
/// (`sys.sp_help`, `master.sys.sp_help`), so they always name the system
/// procedure. Exported as
/// [`READ_ONLY_PROCEDURES`](super::READ_ONLY_PROCEDURES).
pub const ALLOWED_PROCEDURES: &[&str] = &[
    "sp_column_privileges",
    "sp_columns",
    "sp_databases",
    "sp_datatype_info",
    "sp_depends",
    "sp_describe_first_result_set",
    "sp_describe_undeclared_parameters",
    "sp_fkeys",
    "sp_help",
    "sp_helpconstraint",
    "sp_helpfile",
    "sp_helpfilegroup",
    "sp_helpindex",
    "sp_helprole",
    "sp_helprolemember",
    "sp_helprotect",
    "sp_helpstats",
    "sp_helptext",
    "sp_helptrigger",
    "sp_helpuser",
    "sp_lock",
    "sp_pkeys",
    "sp_server_info",
    "sp_special_columns",
    "sp_sproc_columns",
    "sp_statistics",
    "sp_stored_procedures",
    "sp_table_privileges",
    "sp_tables",
    "sp_who",
    "sp_who2",
];

/// Session options under which the server stops running statements: it
/// only compiles them (`SHOWPLAN_*`, `NOEXEC`, `PARSEONLY`) or returns
/// their result shapes (`FMTONLY`). A write sent then reports success
/// without happening.
pub(super) const NO_EXECUTE_OPTIONS: &[&str] = &[
    "SHOWPLAN_XML",
    "SHOWPLAN_ALL",
    "SHOWPLAN_TEXT",
    "NOEXEC",
    "PARSEONLY",
    "FMTONLY",
];

/// Which [`NO_EXECUTE_OPTIONS`] a writable connection's session has on, as
/// far as the statements sent on it say.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(super) struct SessionModes {
    on: std::collections::BTreeSet<&'static str>,
    /// A statement changed (or may have changed) one in a way that can't be
    /// followed, or failed part-way through changing one.
    unknown: bool,
}

impl SessionModes {
    /// Whether the session is known to run statements normally.
    pub(super) fn is_normal(&self) -> bool {
        self.on.is_empty() && !self.unknown
    }

    /// Follow `sql`, which ran (`succeeded`) or failed.
    pub(super) fn apply(&mut self, sql: &str, succeeded: bool) {
        match no_execute_changes(sql) {
            Ok(changes) if changes.is_empty() => {}
            Ok(changes) if succeeded => {
                for (option, on) in changes {
                    if on {
                        self.on.insert(option);
                    } else {
                        self.on.remove(option);
                    }
                }
            }
            Ok(_) | Err(()) => self.unknown = true,
        }
    }
}

/// The [`NO_EXECUTE_OPTIONS`] `sql` turns on (`true`) or off, in order.
///
/// # Errors
///
/// * `sql` names one of them other than as `SET option [, option ...] ON|OFF`
///   (or can't be tokenized and mentions one), so its effect is unknown
pub(super) fn no_execute_changes(sql: &str) -> Result<Vec<(&'static str, bool)>, ()> {
    let option = |token: &Token<'_>| {
        NO_EXECUTE_OPTIONS
            .iter()
            .copied()
            .find(|option| token.is_word(option))
    };
    let Ok(tokens) = tokenize(sql) else {
        let upper = sql.to_ascii_uppercase();
        return if NO_EXECUTE_OPTIONS
            .iter()
            .any(|option| upper.contains(option))
        {
            Err(())
        } else {
            Ok(Vec::new())
        };
    };
    let mut changes = Vec::new();
    let mut followed = vec![false; tokens.len()];
    for (index, token) in tokens.iter().enumerate() {
        if !token.is_word("SET") {
            continue;
        }
        // SET name [, name ...] ON|OFF
        let mut names = Vec::new();
        let mut at = index + 1;
        while tokens.get(at).is_some_and(|t| t.kind == Kind::Word) {
            names.push(at);
            at += 1;
            if !tokens.get(at).is_some_and(|t| t.is_symbol(',')) {
                break;
            }
            at += 1;
        }
        let Some(value) = tokens.get(at) else {
            continue;
        };
        let on = if value.is_word("ON") {
            true
        } else if value.is_word("OFF") {
            false
        } else {
            continue;
        };
        for name in names {
            if let Some(option) = option(&tokens[name]) {
                changes.push((option, on));
                followed[name] = true;
            }
        }
    }
    let unfollowed = tokens
        .iter()
        .zip(&followed)
        .any(|(token, &followed)| !followed && option(token).is_some());
    if unfollowed { Err(()) } else { Ok(changes) }
}

/// What a statement the read-only check accepted does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReadOnlyStatement {
    /// A query (or an allowed catalog procedure): run it in the rolled-back
    /// transaction.
    Read,
    /// `SET SHOWPLAN_XML ON` (transactions only).
    ShowplanOn,
    /// `SET SHOWPLAN_XML OFF` (transactions only).
    ShowplanOff,
    /// `SET STATISTICS XML ON` (transactions only): queries still run, and
    /// return their actual plans as a further result set.
    StatisticsOn,
    /// `SET STATISTICS XML OFF` (transactions only).
    StatisticsOff,
}

/// Check that `sql` is a statement a read-only connection may run (see the
/// [module documentation](self)). `in_transaction` allows `SET SHOWPLAN_XML`
/// and `SET STATISTICS XML`.
///
/// # Errors
///
/// * The reason the statement is refused
pub(super) fn check_read_only(
    sql: &str,
    in_transaction: bool,
) -> Result<ReadOnlyStatement, String> {
    let tokens =
        tokenize(sql).map_err(|error| format!("the statement can't be checked: {error}"))?;
    let mut end = tokens.len();
    while end > 0 && tokens[end - 1].is_symbol(';') {
        end -= 1;
    }
    let tokens = &tokens[..end];
    let Some(first) = tokens.first() else {
        return Err("the statement is empty".to_string());
    };
    if tokens.iter().any(|token| token.is_symbol(';')) {
        return Err("only one statement can run at a time".to_string());
    }
    // T-SQL needs no `;` between statements: a label (`name:`) or a
    // `FETCH` outside `OFFSET ... ROWS FETCH` would start another one.
    for (index, token) in tokens.iter().enumerate() {
        let before = index.checked_sub(1).map(|i| &tokens[i]);
        let after = tokens.get(index + 1);
        if token.is_symbol(':')
            && !before.is_some_and(|t| t.is_symbol(':'))
            && !after.is_some_and(|t| t.is_symbol(':'))
        {
            return Err(
                "only one statement can run at a time (a label starts another)".to_string(),
            );
        }
        if token.is_word("FETCH") && !before.is_some_and(|t| t.is_word("ROW") || t.is_word("ROWS"))
        {
            return Err("only one statement can run at a time (FETCH starts another)".to_string());
        }
    }
    if let Some(showplan) = showplan(tokens) {
        if !in_transaction {
            return Err(
                "SET SHOWPLAN_XML and SET STATISTICS XML change the session, so they only run \
                 inside a transaction"
                    .to_string(),
            );
        }
        return Ok(showplan);
    }
    if first.is_word("EXEC") || first.is_word("EXECUTE") {
        return allowed_procedure(&tokens[1..]).map(|()| ReadOnlyStatement::Read);
    }
    let leading = tokens
        .iter()
        .find(|token| !token.is_symbol('('))
        .unwrap_or(first);
    if !(leading.is_word("SELECT") || leading.is_word("WITH")) {
        return Err(format!(
            "only queries (SELECT or WITH) can run, not {}",
            describe(first)
        ));
    }
    check_words(tokens)?;
    Ok(ReadOnlyStatement::Read)
}

/// How a refused token is named in a message.
fn describe(token: &Token<'_>) -> String {
    match token.kind {
        Kind::Word => token.text.to_ascii_uppercase(),
        _ => format!("{:?}", token.text),
    }
}

/// `SET SHOWPLAN_XML ON|OFF` or `SET STATISTICS XML ON|OFF`, if that is the
/// whole statement.
fn showplan(tokens: &[Token<'_>]) -> Option<ReadOnlyStatement> {
    match tokens {
        [set, statistics, xml, value]
            if set.is_word("SET") && statistics.is_word("STATISTICS") && xml.is_word("XML") =>
        {
            if value.is_word("ON") {
                Some(ReadOnlyStatement::StatisticsOn)
            } else if value.is_word("OFF") {
                Some(ReadOnlyStatement::StatisticsOff)
            } else {
                None
            }
        }
        [set, option, value]
            if set.is_word("SET") && option.is_word("SHOWPLAN_XML") && value.is_word("ON") =>
        {
            Some(ReadOnlyStatement::ShowplanOn)
        }
        [set, option, value]
            if set.is_word("SET") && option.is_word("SHOWPLAN_XML") && value.is_word("OFF") =>
        {
            Some(ReadOnlyStatement::ShowplanOff)
        }
        _ => None,
    }
}

/// Refuse denied words, sequences and linked-server names.
fn check_words(tokens: &[Token<'_>]) -> Result<(), String> {
    for (index, token) in tokens.iter().enumerate() {
        if token.kind == Kind::Word {
            if let Some(word) = DENIED_WORDS
                .iter()
                .find(|word| token.text.eq_ignore_ascii_case(word))
            {
                return Err(format!(
                    "{word} isn't allowed on a read-only connection (it writes, changes the \
                     session, runs code or reaches outside the database)"
                ));
            }
            if token.is_word("NEXT") && tokens.get(index + 1).is_some_and(|t| t.is_word("VALUE")) {
                return Err(
                    "NEXT VALUE FOR advances a sequence, which a rollback doesn't undo".to_string(),
                );
            }
        }
    }
    if let Some(parts) = longest_name(tokens)
        && parts > 3
    {
        return Err(
            "four-part names reach linked servers, which a read-only connection doesn't allow"
                .to_string(),
        );
    }
    Ok(())
}

/// The most parts in a dotted name (`a.b.c` has 3, `a..c` too).
fn longest_name(tokens: &[Token<'_>]) -> Option<usize> {
    let mut longest = None;
    let mut index = 0;
    while index < tokens.len() {
        if !tokens[index].is_name() {
            index += 1;
            continue;
        }
        let mut parts = 1;
        let mut cursor = index + 1;
        while cursor < tokens.len() && tokens[cursor].is_symbol('.') {
            parts += 1;
            cursor += 1;
            if cursor < tokens.len() && tokens[cursor].is_name() {
                cursor += 1;
            }
        }
        longest = longest.max(Some(parts));
        index = cursor.max(index + 1);
    }
    longest
}

/// The unquoted text of a name part (`[x]`, `"x"` or `x`).
fn name_part(token: &Token<'_>) -> Option<String> {
    match token.kind {
        Kind::Word => Some(token.text.to_string()),
        Kind::Quoted => {
            let inner = &token.text[1..token.text.len() - 1];
            Some(if token.text.starts_with('[') {
                inner.replace("]]", "]")
            } else {
                inner.replace("\"\"", "\"")
            })
        }
        _ => None,
    }
}

/// `EXEC <procedure> [args]`: the procedure must be an allowed catalog
/// procedure, unqualified or qualified by `sys` (or `master.sys`), and every
/// argument a literal (`@name = literal` allowed).
///
/// Other qualifiers are refused because they can name a user procedure of
/// the same name: `dbo.sp_who` (or `master..sp_who`) runs a procedure
/// someone created in that schema, which can contain anything. An
/// unqualified `sp_` name resolves to the system procedure first, and
/// nobody can create objects in the `sys` schema.
fn allowed_procedure(tokens: &[Token<'_>]) -> Result<(), String> {
    let refuse = || {
        "EXEC runs code a rollback may not undo; on a read-only connection only these catalog \
         procedures can run, with literal arguments: "
            .to_string()
            + &ALLOWED_PROCEDURES.join(", ")
    };
    let mut parts = Vec::new();
    let mut index = 0;
    loop {
        let part = tokens.get(index).and_then(name_part).ok_or_else(refuse)?;
        parts.push(part);
        index += 1;
        if tokens.get(index).is_some_and(|token| token.is_symbol('.')) {
            index += 1;
        } else {
            break;
        }
    }
    let (procedure, qualifiers) = parts.split_last().ok_or_else(refuse)?;
    let qualified_ok = match qualifiers {
        [] => true,
        [schema] => schema.eq_ignore_ascii_case("sys"),
        [database, schema] => {
            database.eq_ignore_ascii_case("master") && schema.eq_ignore_ascii_case("sys")
        }
        _ => false,
    };
    if !qualified_ok
        || !ALLOWED_PROCEDURES
            .iter()
            .any(|allowed| allowed.eq_ignore_ascii_case(procedure))
    {
        return Err(refuse());
    }
    // Arguments: [@name =] literal, separated by commas.
    let mut expect_value = true;
    let mut rest = tokens[index..].iter().peekable();
    while let Some(token) = rest.next() {
        if expect_value {
            if token.kind == Kind::Variable && rest.peek().is_some_and(|next| next.is_symbol('=')) {
                rest.next();
                let value = rest.next().ok_or_else(refuse)?;
                if !literal(value) {
                    return Err(refuse());
                }
            } else if !literal(token) {
                return Err(refuse());
            }
            expect_value = false;
        } else if token.is_symbol(',') {
            expect_value = true;
        } else {
            return Err(refuse());
        }
    }
    Ok(())
}

/// Whether `token` is a literal procedure argument.
fn literal(token: &Token<'_>) -> bool {
    matches!(token.kind, Kind::String | Kind::Number | Kind::Quoted)
        || token.is_word("NULL")
        || token.is_word("DEFAULT")
}

/// Rewrite the `@P<n>` placeholders in `sql` whose value `inline` renders as
/// SQL (`SYSUTCDATETIME()` for "now"), renumbering the rest so they stay
/// `@P1..@Pk` in order. Returns the statement and the indexes of the values
/// still bound. Placeholders inside literals, comments and quoted
/// identifiers are left alone.
///
/// # Errors
///
/// * The statement can't be tokenized
/// * A placeholder refers to a value that wasn't given
pub(super) fn inline_placeholders(
    sql: &str,
    count: usize,
    inline: impl Fn(usize) -> Option<String>,
) -> Result<(String, Vec<usize>), String> {
    let inlined: Vec<Option<String>> = (0..count).map(&inline).collect();
    if inlined.iter().all(Option::is_none) {
        return Ok((sql.to_string(), (0..count).collect()));
    }
    let mut kept = Vec::new();
    let mut numbers = vec![None; count];
    for (index, value) in inlined.iter().enumerate() {
        if value.is_none() {
            kept.push(index);
            numbers[index] = Some(kept.len());
        }
    }
    let mut out = String::with_capacity(sql.len());
    let mut copied = 0;
    for token in tokenize(sql)? {
        let Some(number) = placeholder_number(&token) else {
            continue;
        };
        let index = number
            .checked_sub(1)
            .filter(|&index| index < count)
            .ok_or_else(|| format!("{} has no value", token.text))?;
        out.push_str(&sql[copied..token.start]);
        match (&inlined[index], numbers[index]) {
            (Some(expression), _) => out.push_str(expression),
            (None, Some(number)) => {
                let _ = write!(out, "@P{number}");
            }
            (None, None) => unreachable!("every bound value has a number"),
        }
        copied = token.start + token.text.len();
    }
    out.push_str(&sql[copied..]);
    Ok((out, kept))
}

/// The number of an `@P<n>` placeholder.
fn placeholder_number(token: &Token<'_>) -> Option<usize> {
    if token.kind != Kind::Variable {
        return None;
    }
    let digits = token
        .text
        .strip_prefix("@P")
        .or_else(|| token.text.strip_prefix("@p"))?;
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn words(sql: &str) -> Vec<(Kind, &str)> {
        tokenize(sql)
            .unwrap()
            .into_iter()
            .map(|token| (token.kind, token.text))
            .collect()
    }

    #[test]
    fn tokens_skip_comments_and_keep_literals_whole() {
        assert_eq!(
            words("SELECT [a]]b], \"c\", N'it''s', 'x' -- DELETE\n/* a /* nested */ DROP */ @p1"),
            vec![
                (Kind::Word, "SELECT"),
                (Kind::Quoted, "[a]]b]"),
                (Kind::Symbol, ","),
                (Kind::Quoted, "\"c\""),
                (Kind::Symbol, ","),
                (Kind::String, "N'it''s'"),
                (Kind::Symbol, ","),
                (Kind::String, "'x'"),
                (Kind::Variable, "@p1"),
            ]
        );
        assert_eq!(
            words("$1.50 + 0xFF + .5 + $action + #tmp"),
            vec![
                (Kind::Number, "$1.50"),
                (Kind::Symbol, "+"),
                (Kind::Number, "0xFF"),
                (Kind::Symbol, "+"),
                (Kind::Number, ".5"),
                (Kind::Symbol, "+"),
                (Kind::Word, "$action"),
                (Kind::Symbol, "+"),
                (Kind::Word, "#tmp"),
            ]
        );
        for broken in [
            "select 'x",
            "select [x",
            "select \"x",
            "select /* /* */ 1",
            "select N'",
        ] {
            assert!(tokenize(broken).is_err(), "{broken}");
        }
    }

    #[test]
    fn reads_pass() {
        for sql in [
            "SELECT 1",
            "select * from dbo.albums where title = 'INSERT INTO x' -- DELETE",
            "SELECT [update], \"drop\" FROM [shop].[albums] WITH (NOLOCK);",
            "WITH c AS (SELECT 1 AS n) SELECT n FROM c",
            "(SELECT 1) UNION ALL (SELECT 2)",
            "SELECT * FROM t ORDER BY id OFFSET 10 ROWS FETCH NEXT 5 ROWS ONLY",
            "SELECT TOP (5) * FROM other_db.dbo.t",
            "SELECT geography::Point(1, 2, 4326)",
            "SELECT CASE WHEN a IS NULL THEN 1 ELSE 2 END FROM t",
            "SELECT * FROM t WHERE id = @P1 FOR JSON PATH",
            "SELECT @@TRANCOUNT; ;",
            "SELECT 1 /* COMMIT */",
            "EXEC sp_help N'dbo.albums'",
            "exec sys.sp_columns @table_name = 'albums', @table_owner = 'dbo'",
            "EXECUTE master.sys.sp_who2",
            "EXEC [sys].[sp_tables]",
        ] {
            assert_eq!(
                check_read_only(sql, false),
                Ok(ReadOnlyStatement::Read),
                "{sql}"
            );
        }
    }

    #[test]
    fn writes_and_escapes_are_refused() {
        for sql in [
            "",
            "-- nothing",
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "MERGE t USING s ON 1 = 1 WHEN MATCHED THEN DELETE;",
            "SELECT * INTO new_table FROM t",
            "SELECT 1; DELETE FROM t",
            "SELECT 1 DELETE FROM t",
            "SELECT 1 COMMIT",
            "WITH c AS (SELECT 1 AS n) DELETE FROM t",
            "TRUNCATE TABLE t",
            "DROP TABLE t",
            "CREATE TABLE t (a int)",
            "ALTER DATABASE CURRENT SET READ_ONLY",
            "BACKUP DATABASE shop TO DISK = '/tmp/x.bak'",
            "RESTORE DATABASE shop FROM DISK = '/tmp/x.bak'",
            "DBCC CHECKDB",
            "KILL 52",
            "SHUTDOWN",
            "RECONFIGURE",
            "CHECKPOINT",
            "BULK INSERT t FROM '/etc/passwd'",
            "SELECT * FROM OPENROWSET(BULK '/etc/passwd', SINGLE_CLOB) AS x",
            "SELECT * FROM OPENQUERY(remote, 'select 1')",
            "SELECT * FROM OPENDATASOURCE('SQLNCLI', 'Data Source=x').db.dbo.t",
            "SELECT * FROM remote.db.dbo.t",
            "SELECT * FROM [remote].[db]..[t]",
            "SELECT NEXT VALUE FOR dbo.order_numbers",
            "EXEC xp_cmdshell 'whoami'",
            "EXEC sp_executesql N'DELETE FROM t'",
            "EXEC sp_configure 'xp_cmdshell', 1",
            "EXEC msdb.dbo.sp_send_dbmail @body = 'x'",
            "EXEC sp_OACreate 'x'",
            "EXEC dbo.my_proc",
            "EXEC sp_help @objname = @x",
            "EXEC sp_help 'a' WITH RECOMPILE",
            "EXEC other.sys.sp_help",
            "EXEC dbo.sp_who",
            "EXEC [dbo].[sp_help] 'albums'",
            "EXEC shop.sp_columns 'albums'",
            "EXECUTE master.dbo.sp_who2",
            "EXEC master..sp_who",
            "EXEC ('DELETE FROM t')",
            "WAITFOR DELAY '00:00:10'",
            "BEGIN TRAN",
            "COMMIT",
            "SAVE TRANSACTION x",
            "BEGIN DISTRIBUTED TRANSACTION",
            "USE master",
            "SET NOCOUNT ON",
            "DECLARE @t TABLE (a int)",
            "GRANT SELECT ON t TO public",
            "DENY SELECT ON t TO public",
            "REVOKE SELECT ON t FROM public",
            "SELECT 1 RECEIVE * FROM q",
            "SELECT 1 /* unterminated",
            "SELECT 'unterminated",
            "UPDATE STATISTICS t",
            "SELECT 1 GO",
            "SET SHOWPLAN_XML ON",
            // The server ends a `--` comment at a carriage return.
            "SELECT 1 --x\rDELETE FROM t",
            "SELECT 1 --x\rCOMMIT",
            // T-SQL needs no `;`: a label or a FETCH starts another statement.
            "SELECT 1 lbl: SELECT 2",
            "SELECT 1 FETCH NEXT FROM c",
        ] {
            assert!(check_read_only(sql, false).is_err(), "{sql}");
        }
    }

    #[test]
    fn numbers_running_into_words_are_refused() {
        // The server ends a number where its syntax ends and starts a new
        // token there, so each of these hides a keyword from a check that
        // reads the whole run as one number.
        let literals = [
            "1", "1.", "1.5", ".5", "1e1", "1e+1", "1e-1", "1E5", "1e", "1.e1", "1.5e", "$1",
            "$1.5", "$.5", "$1.", "$-1", "$+1", "$-.5", "0x", "0x1", "0xAB",
        ];
        for literal in literals {
            for keyword in ["DELETE", "COMMIT", "EXEC('x')", "INSERT", "UPDATE", "AS x"] {
                for sql in [
                    format!("SELECT {literal}{keyword}"),
                    format!("SELECT TOP (1) {literal}{keyword} FROM t"),
                    format!("SELECT a FROM t WHERE a = {literal}{keyword}"),
                    format!("EXEC sp_who {literal}{keyword}"),
                ] {
                    assert!(check_read_only(&sql, false).is_err(), "{sql}");
                }
            }
            for sql in [
                format!("SELECT {literal}"),
                format!("SELECT {literal} AS x"),
                format!("SELECT a FROM t WHERE a = {literal} ORDER BY a"),
            ] {
                assert_eq!(
                    check_read_only(&sql, false),
                    Ok(ReadOnlyStatement::Read),
                    "{sql}"
                );
            }
        }
        assert_eq!(
            check_read_only("SELECT 1.5e-3, 0xFF, $1.50, $action FROM t", false),
            Ok(ReadOnlyStatement::Read)
        );
        assert_eq!(
            check_read_only(
                "SELECT a FROM t ORDER BY a OFFSET 10 ROWS FETCH NEXT 5 ROWS ONLY",
                false
            ),
            Ok(ReadOnlyStatement::Read)
        );
        assert_eq!(
            check_read_only("SELECT geography::Point(1, 2, 4326) -- note\r\n", false),
            Ok(ReadOnlyStatement::Read)
        );
    }

    #[test]
    fn showplan_runs_only_in_transactions() {
        assert_eq!(
            check_read_only("set showplan_xml on", true),
            Ok(ReadOnlyStatement::ShowplanOn)
        );
        assert_eq!(
            check_read_only("SET SHOWPLAN_XML OFF;", true),
            Ok(ReadOnlyStatement::ShowplanOff)
        );
        assert_eq!(
            check_read_only("set statistics xml on", true),
            Ok(ReadOnlyStatement::StatisticsOn)
        );
        assert_eq!(
            check_read_only("SET STATISTICS XML OFF;", true),
            Ok(ReadOnlyStatement::StatisticsOff)
        );
        assert!(check_read_only("SET STATISTICS XML ON", false).is_err());
        assert!(check_read_only("SET STATISTICS IO ON", true).is_err());
        assert!(check_read_only("SET STATISTICS XML ON; SELECT 1", true).is_err());
        assert!(check_read_only("SET SHOWPLAN_XML ON; SELECT 1", true).is_err());
    }

    #[test]
    fn session_modes_follow_no_execute_options() {
        assert_eq!(no_execute_changes("UPDATE t SET a = 1"), Ok(vec![]));
        assert_eq!(
            no_execute_changes("set showplan_xml on"),
            Ok(vec![("SHOWPLAN_XML", true)])
        );
        assert_eq!(
            no_execute_changes("SET NOEXEC, PARSEONLY OFF"),
            Ok(vec![("NOEXEC", false), ("PARSEONLY", false)])
        );
        // SET options made in dynamic SQL revert when its batch ends.
        assert_eq!(no_execute_changes("EXEC ('SET NOEXEC ON')"), Ok(vec![]));
        assert_eq!(no_execute_changes("SELECT NOEXEC FROM t"), Err(()));
        assert_eq!(no_execute_changes("SET FMTONLY @x"), Err(()));
        assert_eq!(no_execute_changes("SELECT 'NOEXEC"), Err(()));

        let mut modes = SessionModes::default();
        assert!(modes.is_normal());
        modes.apply("SET SHOWPLAN_XML ON", true);
        assert!(!modes.is_normal());
        modes.apply("SET SHOWPLAN_XML OFF", true);
        assert!(modes.is_normal());
        modes.apply("SET NOEXEC ON", false);
        assert!(
            !modes.is_normal(),
            "a failed change leaves the mode unknown"
        );
    }

    #[test]
    fn placeholders_are_inlined_and_renumbered() {
        let (sql, kept) =
            inline_placeholders("SELECT @P1, '@P2', [@P2], @P2, @P3 -- @P1", 3, |index| {
                (index == 1).then(|| "SYSUTCDATETIME()".to_string())
            })
            .unwrap();
        assert_eq!(
            sql,
            "SELECT @P1, '@P2', [@P2], SYSUTCDATETIME(), @P2 -- @P1"
        );
        assert_eq!(kept, vec![0, 2]);
        assert!(inline_placeholders("SELECT @P4", 1, |_| Some("x".into())).is_err());
        assert_eq!(
            inline_placeholders("SELECT @P1", 1, |_| None).unwrap(),
            ("SELECT @P1".to_string(), vec![0])
        );
    }
}
