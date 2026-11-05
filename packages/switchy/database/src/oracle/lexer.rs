//! A conservative Oracle SQL tokenizer, the read-only statement check
//! built on it, and placeholder handling.
//!
//! The tokenizer reads exactly what it needs to find the words of a
//! statement: string literals (`'...'`, `N'...'`, and the alternative
//! quoting `q'[...]'` / `nq'{...}'`), quoted identifiers (`"..."`),
//! comments (`-- ...` and `/* ... */`, which don't nest in Oracle; optimizer
//! hints are comments), bind variables (`:1`, `:name`) and numbers.
//! Anything it can't read the way Oracle does is an error, and the
//! read-only check refuses what it can't tokenize:
//!
//! * an unterminated string, quoted identifier or comment;
//! * a line comment holding a bare carriage return, and whitespace other
//!   than space, tab, carriage return, line feed and form feed;
//! * a number running straight into a word (`1x`, which Oracle reads as the
//!   number `1` followed by `x`), except the `f`/`d` suffixes of
//!   `BINARY_FLOAT`/`BINARY_DOUBLE` literals (`1.5f`);
//! * two quoted identifiers back to back (`"a""b"`: Oracle has no escaped
//!   double quote).
//!
//! # The read-only check
//!
//! Oracle runs one statement per call, so nothing can follow a query
//! unseen; what a read-only connection has to keep out is a statement that
//! isn't a query, and a query that does more than read. [`check_read_only`]
//! only lets through statements that are reads by construction:
//!
//! * one statement (a `;` may only end it), starting with `SELECT`, `WITH`
//!   or `(`;
//! * containing none of the [`DENIED_WORDS`] outside literals, comments and
//!   quoted identifiers: every keyword that starts or makes a statement
//!   other than a query (`INSERT`, `INTO`, `MERGE`, `CREATE`, `CALL`,
//!   `BEGIN`, `DECLARE`, `COMMIT`, `SET`, `EXPLAIN`, `LOCK`, ...), a
//!   PL/SQL function declared in a `WITH` clause (`FUNCTION`, `PROCEDURE`),
//!   `FOR UPDATE` (`UPDATE`), and sequence values (`NEXTVAL`, which advances
//!   the sequence outside any transaction);
//! * no names starting with the prefixes of Oracle's side-effect packages
//!   ([`DENIED_PREFIXES`]: `DBMS_`, `UTL_`, ...), no database links (`@`);
//! * every call written with parentheses (`name(...)`) is an Oracle
//!   built-in SQL function ([`BUILTIN_FUNCTIONS`]), unqualified (Oracle
//!   resolves an unqualified built-in name called with parentheses to the
//!   built-in even when a schema has a function of the same name; built-ins
//!   with syntax of their own that a user function could take over, such as
//!   `COLLECT` or the XML aggregates, are left out, and `EXTRACT` is allowed
//!   only as `EXTRACT(field FROM value)`), or a reserved word that takes
//!   parentheses ([`RESERVED_PAREN_WORDS`]: `IN (`, `NUMBER(10, 2)`; a
//!   reserved word can't name a function), or an unreserved keyword where
//!   the grammar makes it one ([`CONTEXT_PAREN_KEYWORDS`]: `OVER (` after
//!   a `)`, `PIVOT (` after a table, `CAST(x AS TIMESTAMP(6))`, ...;
//!   anywhere else Oracle calls a function of that name), or the `(+)`
//!   outer-join marker, or a `WITH` clause's column list. A user
//!   function may be autonomous (`PRAGMA AUTONOMOUS_TRANSACTION`) and
//!   commit writes of its own, so calls to anything else are refused.
//!
//! What the check can't see is code already in the database that a query
//! runs without naming it as a call: a function called without
//! parentheses (Oracle lets a function without arguments be called as
//! `name` or `schema.name`, which reads like a column), a view's functions,
//! an object type's `MAP`/`ORDER` method, policy functions. The backend's
//! other measures (a read-only session, a read-only transaction) stop most
//! of what such code could do; see the [backend documentation](super).

/// A token's kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    /// A bare word: a keyword or an unquoted identifier.
    Word,
    /// A quoted identifier: `"name"`.
    Quoted,
    /// A bind variable: `:1`, `:name`.
    Bind,
    /// A string literal (any of its forms).
    String,
    /// A number literal.
    Number,
    /// Any other character (`;`, `.`, `(`, `,`, operators, `@`, ...).
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
    pub(super) fn is_word(&self, word: &str) -> bool {
        self.kind == Kind::Word && self.text.eq_ignore_ascii_case(word)
    }

    /// Whether this is the symbol `symbol`.
    pub(super) fn is_symbol(&self, symbol: char) -> bool {
        self.kind == Kind::Symbol && self.text.starts_with(symbol)
    }

    /// The upper-case text of a bare word.
    fn upper(&self) -> String {
        self.text.to_ascii_uppercase()
    }
}

/// Characters that may continue a bare word.
fn word_char(ch: char) -> bool {
    ch.is_alphanumeric() || matches!(ch, '_' | '$' | '#')
}

/// Whitespace as Oracle reads it.
const fn oracle_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\r' | '\n' | '\x0C')
}

/// The closing delimiter of an alternative-quoting literal opened with
/// `open`, or `None` if `open` can't open one.
const fn q_close(open: char) -> Option<char> {
    match open {
        '[' => Some(']'),
        '{' => Some('}'),
        '<' => Some('>'),
        '(' => Some(')'),
        ' ' | '\t' | '\r' | '\n' | '\'' => None,
        other => Some(other),
    }
}

/// Split `sql` into tokens, skipping whitespace and comments.
///
/// # Errors
///
/// * Text the tokenizer can't read the way Oracle does (see the [module
///   documentation](self))
#[allow(clippy::too_many_lines)]
pub(super) fn tokenize(sql: &str) -> Result<Vec<Token<'_>>, String> {
    let bytes = sql.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while let Some(ch) = sql[at..].chars().next() {
        let start = at;
        if oracle_space(ch) {
            at += 1;
            continue;
        }
        if ch.is_whitespace() || ch.is_control() {
            return Err(format!(
                "the statement contains the character U+{:04X}, which Oracle may not read as \
                 whitespace",
                u32::from(ch)
            ));
        }
        let next = bytes.get(at + ch.len_utf8()).copied();
        // Line comment, up to the line feed.
        if ch == '-' && next == Some(b'-') {
            let end = sql[at..].find('\n').map_or(sql.len(), |offset| at + offset);
            if sql[at..end].trim_end_matches('\r').contains('\r') {
                return Err("a line comment contains a carriage return".to_string());
            }
            at = end;
            continue;
        }
        // Block comments (and optimizer hints) don't nest.
        if ch == '/' && next == Some(b'*') {
            let Some(offset) = sql[at + 2..].find("*/") else {
                return Err("a /* comment is not closed".to_string());
            };
            at += 2 + offset + 2;
            continue;
        }
        let rest = &sql[at..];
        let lower = rest
            .chars()
            .take(3)
            .collect::<String>()
            .to_ascii_lowercase();
        let (kind, end) = if ch == '\'' {
            (Kind::String, quoted_end(sql, at)?)
        } else if lower.starts_with("nq'") {
            (Kind::String, q_quoted_end(sql, at + 2)?)
        } else if lower.starts_with("q'") {
            (Kind::String, q_quoted_end(sql, at + 1)?)
        } else if lower.starts_with("n'") {
            (Kind::String, quoted_end(sql, at + 1)?)
        } else if ch == '"' {
            let Some(offset) = sql[at + 1..].find('"') else {
                return Err("a \"quoted\" identifier is not closed".to_string());
            };
            if offset == 0 {
                return Err("an empty \"\" identifier".to_string());
            }
            let end = at + 1 + offset + 1;
            if bytes.get(end) == Some(&b'"') {
                return Err("two quoted identifiers run together".to_string());
            }
            (Kind::Quoted, end)
        } else if ch == ':' && next.is_some_and(|b| b.is_ascii_alphanumeric() || b == b'_') {
            (Kind::Bind, scan(sql, at + 1, word_char))
        } else if ch.is_ascii_digit() || (ch == '.' && next.is_some_and(|b| b.is_ascii_digit())) {
            (Kind::Number, number_end(sql, at)?)
        } else if ch.is_alphabetic() {
            (Kind::Word, scan(sql, at + ch.len_utf8(), word_char))
        } else {
            (Kind::Symbol, at + ch.len_utf8())
        };
        tokens.push(Token {
            kind,
            text: &sql[start..end],
            start,
        });
        at = end;
    }
    Ok(tokens)
}

/// The end of the run of `continues` characters starting at `from`.
fn scan(sql: &str, from: usize, continues: fn(char) -> bool) -> usize {
    sql[from..]
        .char_indices()
        .find(|&(_, c)| !continues(c))
        .map_or(sql.len(), |(offset, _)| from + offset)
}

/// The end of the `'...'` literal whose quote is at `start` (`''` is an
/// escaped quote).
fn quoted_end(sql: &str, start: usize) -> Result<usize, String> {
    let body = start + 1;
    let mut chars = sql[body..].char_indices().peekable();
    while let Some((offset, c)) = chars.next() {
        if c == '\'' {
            if chars.peek().is_some_and(|&(_, next)| next == '\'') {
                chars.next();
                continue;
            }
            return Ok(body + offset + 1);
        }
    }
    Err("a string literal is not closed".to_string())
}

/// The end of the alternative-quoting literal whose quote (after `q`) is
/// at `quote`: `q'[...]'` ends at the first `]'`.
fn q_quoted_end(sql: &str, quote: usize) -> Result<usize, String> {
    let open_at = quote + 1;
    let Some(open) = sql[open_at..].chars().next() else {
        return Err("a q'...' literal is not closed".to_string());
    };
    let close =
        q_close(open).ok_or_else(|| format!("a q'...' literal can't be delimited by {open:?}"))?;
    let body = open_at + open.len_utf8();
    let mut terminator = String::with_capacity(2);
    terminator.push(close);
    terminator.push('\'');
    sql[body..]
        .find(&terminator)
        .map(|offset| body + offset + terminator.len())
        .ok_or_else(|| "a q'...' literal is not closed".to_string())
}

/// The end of the number at `start` (`1`, `1.`, `.5`, `1.5e-3`, `2f`,
/// `1.5D`), read as Oracle reads it.
///
/// # Errors
///
/// * The number runs straight into a word: Oracle would start a new token
///   there, so it is refused rather than guessed at
fn number_end(sql: &str, start: usize) -> Result<usize, String> {
    let bytes = sql.as_bytes();
    let digits = |mut at: usize| {
        while bytes.get(at).is_some_and(u8::is_ascii_digit) {
            at += 1;
        }
        at
    };
    let mut end = digits(start);
    if bytes.get(end) == Some(&b'.') && bytes.get(end + 1) != Some(&b'.') {
        end = digits(end + 1);
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let mut exponent = end + 1;
        if matches!(bytes.get(exponent), Some(b'+' | b'-')) {
            exponent += 1;
        }
        if bytes.get(exponent).is_some_and(u8::is_ascii_digit) {
            end = digits(exponent);
        }
    }
    if matches!(bytes.get(end), Some(b'f' | b'F' | b'd' | b'D'))
        && !sql[end + 1..].chars().next().is_some_and(word_char)
    {
        end += 1;
    }
    if sql[end..].chars().next().is_some_and(word_char) {
        return Err(format!(
            "the number {:?} runs straight into a word",
            &sql[start..end]
        ));
    }
    Ok(end)
}

/// Words refused anywhere in a statement on a read-only connection: they
/// start (or are part of) statements that write, run code, change the
/// session, control transactions, lock rows, or advance sequences.
pub const DENIED_WORDS: &[&str] = &[
    // Data and schema changes.
    "INSERT",
    "UPDATE",
    "DELETE",
    "MERGE",
    "UPSERT",
    "INTO",
    "CREATE",
    "ALTER",
    "DROP",
    "TRUNCATE",
    "RENAME",
    "PURGE",
    "FLASHBACK",
    "COMMENT",
    "ANALYZE",
    "ASSOCIATE",
    "DISASSOCIATE",
    "AUDIT",
    "NOAUDIT",
    "GRANT",
    "REVOKE",
    "ADMINISTER",
    "LOCK",
    // Code: PL/SQL blocks, calls, dynamic SQL, WITH-clause functions.
    "BEGIN",
    "DECLARE",
    "CALL",
    "EXEC",
    "EXECUTE",
    "IMMEDIATE",
    "FUNCTION",
    "PROCEDURE",
    "PACKAGE",
    "TRIGGER",
    "PRAGMA",
    "LIBRARY",
    "JAVA",
    // Transactions and session state.
    "COMMIT",
    "ROLLBACK",
    "SAVEPOINT",
    "SET",
    "EXPLAIN",
    "SHUTDOWN",
    "STARTUP",
    // Sequences advance outside any transaction.
    "NEXTVAL",
    "CURRVAL",
];

/// Prefixes of the names of Oracle's supplied packages that change data,
/// the session or the world outside the database.
///
/// (`DBMS_SCHEDULER`, `DBMS_PIPE`, `UTL_HTTP`, `UTL_FILE`, ...) Any name
/// starting with one is refused, so neither `pkg.fn(...)` nor a
/// parenthesis-free `pkg.fn` call reaches them.
pub const DENIED_PREFIXES: &[&str] = &[
    "DBMS_", "UTL_", "OWA_", "HTP", "HTF", "APEX_", "CTX_", "ORDSYS", "SDO_", "XDB", "DBURI",
    "HTTPURI",
];

/// Oracle's reserved words (`V$RESERVED_WORDS.RESERVED = 'Y'`) that a `(`
/// may follow (`IN (`, `EXISTS (`, `NUMBER(10, 2)`, `TABLE(...)`).
///
/// A reserved word can't name a function unquoted, so `word(` never calls
/// one (checked live: a function named `"NUMBER"` or `"IN"` is never run
/// by `number(1)` or `in(1)`).
pub const RESERVED_PAREN_WORDS: &[&str] = &[
    "ALL",
    "AND",
    "ANY",
    "AS",
    "BETWEEN",
    "BY",
    "CHAR",
    "DECIMAL",
    "DISTINCT",
    "ELSE",
    "EXCEPT",
    "EXISTS",
    "FLOAT",
    "FOR",
    "FROM",
    "GROUP",
    "HAVING",
    "IN",
    "INTERSECT",
    "IS",
    "LIKE",
    "MINUS",
    "NOT",
    "NUMBER",
    "OF",
    "ON",
    "OR",
    "PRIOR",
    "RAW",
    "SELECT",
    "TABLE",
    "THEN",
    "TO",
    "UNION",
    "VALUES",
    "VARCHAR",
    "VARCHAR2",
    "WHERE",
    "WITH",
];

/// Oracle keywords that aren't reserved, which a `(` follows in some
/// clauses (`OVER (`, `PIVOT (`, `CAST(x AS TIMESTAMP(6))`).
///
/// Unreserved, any of them can name a user function, and `word(` calls it
/// wherever an expression may start (checked live for every one). So each
/// is accepted only where the grammar makes it a keyword
/// ([`keyword_before_paren`]), and is a call (refused) anywhere else.
pub const CONTEXT_PAREN_KEYWORDS: &[&str] = &[
    "APPLY",
    "CHARACTER",
    "COLUMNS",
    "CURSOR",
    "DAY",
    "DEC",
    "JOIN",
    "KEEP",
    "LATERAL",
    "MATCH_RECOGNIZE",
    "MEASURES",
    "MONTH",
    "MULTISET",
    "NCHAR",
    "NUMERIC",
    "NVARCHAR2",
    "OVER",
    "PARTITION",
    "PATTERN",
    "PIVOT",
    "RULES",
    "SAMPLE",
    "SECOND",
    "SEED",
    "SOME",
    "SUBPARTITION",
    "TIMESTAMP",
    "UNPIVOT",
    "UNTIL",
    "UROWID",
    "USING",
    "VECTOR",
    "WHEN",
    "YEAR",
];

/// Whether the token at `at` ends an operand: a `)`, a literal, a bind
/// variable, a quoted name, a name qualified with `.`, or a table name
/// right after `FROM` or `JOIN`. Two operands can't be adjacent, so a
/// `word(` straight after one is never a call.
fn ends_operand(tokens: &[Token<'_>], at: usize) -> bool {
    let token = &tokens[at];
    match token.kind {
        Kind::Quoted | Kind::String | Kind::Number | Kind::Bind => true,
        Kind::Symbol => token.is_symbol(')'),
        Kind::Word => at.checked_sub(1).is_some_and(|before| {
            let before = &tokens[before];
            before.is_symbol('.') || before.is_word("FROM") || before.is_word("JOIN")
        }),
    }
}

/// The word before the innermost `(` still open at `at`, if any.
fn enclosing_call<'a>(tokens: &[Token<'a>], at: usize) -> Option<Token<'a>> {
    let mut depth = 0_usize;
    for index in (0..at).rev() {
        let token = &tokens[index];
        if token.is_symbol(')') {
            depth += 1;
        } else if token.is_symbol('(') {
            if depth == 0 {
                return index
                    .checked_sub(1)
                    .map(|before| tokens[before])
                    .filter(|before| before.kind == Kind::Word);
            }
            depth -= 1;
        }
    }
    None
}

/// Whether the unreserved keyword at `index`, which a `(` follows, is a
/// keyword there rather than a call ([`CONTEXT_PAREN_KEYWORDS`]).
///
/// The rules, each the only place the keyword takes a `(` that the check
/// lets through:
///
/// * any of them before a query (`JOIN (SELECT`, `LATERAL (SELECT`,
///   `CURSOR(SELECT`, `MULTISET(SELECT`, `SOME (SELECT`): a call's
///   argument can't be a bare query (ORA-00936);
/// * `OVER`, `KEEP`, `SEED`, `MEASURES`, `RULES`, `UNTIL` after a `)`
///   (`COUNT(*) OVER (`, `SAMPLE (5) SEED (1)`, the `MODEL` clause);
/// * `PIVOT`, `UNPIVOT`, `MATCH_RECOGNIZE`, `SAMPLE`, `PARTITION`,
///   `SUBPARTITION`, `USING`, `JOIN` after a table (a `)`, a quoted or
///   qualified name, or a name right after `FROM` / `JOIN`), and `JOIN`
///   after a join type (`LEFT JOIN (`);
/// * `APPLY` after `CROSS` or `OUTER`;
/// * `PATTERN` after `ROW`, `MATCH`, `MATCHES` or a `)` (in
///   `MATCH_RECOGNIZE`);
/// * `WHEN` after `CASE` or an operand (`CASE WHEN (`, `THEN 1 WHEN (`);
/// * `COLUMNS` after a string, a `)`, `ERROR` or `EMPTY` (`JSON_TABLE`);
/// * `DAY`, `MONTH`, `YEAR`, `SECOND` after `INTERVAL`, `TO` or a string
///   (interval types and literals);
/// * type names (`TIMESTAMP`, `NCHAR`, `NVARCHAR2`, ...) after `AS` or
///   `RETURNING` inside a built-in's parentheses (`CAST(x AS NCHAR(3))`,
///   `JSON_VALUE(... RETURNING TIMESTAMP(6))`), and `TIMESTAMP` after `AS
///   OF` (a flashback query).
fn keyword_before_paren(tokens: &[Token<'_>], index: usize) -> bool {
    let word = tokens[index].upper();
    let before = index.checked_sub(1).map(|at| &tokens[at]);
    let before_is = |words: &[&str]| before.is_some_and(|t| words.iter().any(|w| t.is_word(w)));
    let after_paren = index
        .checked_sub(1)
        .is_some_and(|at| tokens[at].is_symbol(')'));
    let after_operand = index
        .checked_sub(1)
        .is_some_and(|at| ends_operand(tokens, at));
    let query_follows = tokens
        .get(index + 2)
        .is_some_and(|t| t.is_word("SELECT") || t.is_word("WITH"));
    if query_follows {
        return true;
    }
    let type_position = || {
        before_is(&["AS", "RETURNING"])
            && enclosing_call(tokens, index)
                .is_some_and(|call| BUILTIN_FUNCTIONS.contains(&call.upper().as_str()))
    };
    match word.as_str() {
        "OVER" | "KEEP" | "SEED" | "MEASURES" | "RULES" | "UNTIL" => after_paren,
        "PIVOT" | "UNPIVOT" | "MATCH_RECOGNIZE" | "SAMPLE" | "PARTITION" | "SUBPARTITION"
        | "USING" => after_operand,
        "JOIN" => {
            after_operand
                || before_is(&[
                    "INNER", "LEFT", "RIGHT", "FULL", "OUTER", "CROSS", "NATURAL",
                ])
        }
        "APPLY" => before_is(&["CROSS", "OUTER"]),
        "PATTERN" => after_paren || before_is(&["ROW", "MATCH", "MATCHES"]),
        "WHEN" => after_operand || before_is(&["CASE"]),
        "COLUMNS" => {
            after_paren
                || before.is_some_and(|t| t.kind == Kind::String)
                || before_is(&["ERROR", "EMPTY"])
        }
        "DAY" | "MONTH" | "YEAR" | "SECOND" => {
            before_is(&["INTERVAL", "TO"]) || before.is_some_and(|t| t.kind == Kind::String)
        }
        "TIMESTAMP" => {
            type_position()
                || (before_is(&["OF"])
                    && index
                        .checked_sub(2)
                        .is_some_and(|at| tokens[at].is_word("AS")))
        }
        "CHARACTER" | "DEC" | "NCHAR" | "NUMERIC" | "NVARCHAR2" | "UROWID" | "VECTOR" => {
            type_position()
        }
        _ => false,
    }
}

/// Oracle built-in SQL functions a read-only query may call.
///
/// They compute values and read nothing outside the query. Functions that
/// parse documents (and could fetch external resources: `XMLTYPE`,
/// `XMLPARSE`, `XMLQUERY`), read server files (`BFILENAME`) or call out are
/// left out.
pub const BUILTIN_FUNCTIONS: &[&str] = &[
    "ABS",
    "ACOS",
    "ADD_MONTHS",
    "ANY_VALUE",
    "APPROX_COUNT",
    "APPROX_COUNT_DISTINCT",
    "APPROX_MEDIAN",
    "APPROX_PERCENTILE",
    "APPROX_RANK",
    "APPROX_SUM",
    "ASCII",
    "ASCIISTR",
    "ASIN",
    "ATAN",
    "ATAN2",
    "AVG",
    "BIN_TO_NUM",
    "BITAND",
    "BIT_AND_AGG",
    "BIT_OR_AGG",
    "BIT_XOR_AGG",
    "CARDINALITY",
    "CAST",
    "CEIL",
    "CHARTOROWID",
    "CHECKSUM",
    "CHR",
    "COALESCE",
    "COLLATION",
    "COMPOSE",
    "CONCAT",
    "CONVERT",
    "CORR",
    "CORR_K",
    "CORR_S",
    "COS",
    "COSH",
    "COUNT",
    "COVAR_POP",
    "COVAR_SAMP",
    "CUME_DIST",
    "CURRENT_TIMESTAMP",
    "DECODE",
    "DECOMPOSE",
    "DENSE_RANK",
    "DUMP",
    "EMPTY_BLOB",
    "EMPTY_CLOB",
    "EXP",
    "EXTRACT",
    "FIRST_VALUE",
    "FLOOR",
    "FROM_TZ",
    "GREATEST",
    "GROUPING",
    "GROUPING_ID",
    "GROUP_ID",
    "HEXTORAW",
    "INITCAP",
    "INSTR",
    "INSTR2",
    "INSTR4",
    "INSTRB",
    "INSTRC",
    "JSON",
    "JSON_ARRAY",
    "JSON_ARRAYAGG",
    "JSON_EQUAL",
    "JSON_EXISTS",
    "JSON_MERGEPATCH",
    "JSON_OBJECT",
    "JSON_OBJECTAGG",
    "JSON_QUERY",
    "JSON_SCALAR",
    "JSON_SERIALIZE",
    "JSON_TABLE",
    "JSON_TRANSFORM",
    "JSON_VALUE",
    "KURTOSIS_POP",
    "KURTOSIS_SAMP",
    "LAG",
    "LAST_DAY",
    "LAST_VALUE",
    "LEAD",
    "LEAST",
    "LENGTH",
    "LENGTH2",
    "LENGTH4",
    "LENGTHB",
    "LENGTHC",
    "LISTAGG",
    "LN",
    "LNNVL",
    "LOCALTIMESTAMP",
    "LOG",
    "LOWER",
    "LPAD",
    "LTRIM",
    "MAX",
    "MEDIAN",
    "MIN",
    "MOD",
    "MONTHS_BETWEEN",
    "NANVL",
    "NCHR",
    "NEW_TIME",
    "NEXT_DAY",
    "NLSSORT",
    "NLS_CHARSET_DECL_LEN",
    "NLS_CHARSET_ID",
    "NLS_CHARSET_NAME",
    "NLS_INITCAP",
    "NLS_LOWER",
    "NLS_UPPER",
    "NTH_VALUE",
    "NTILE",
    "NULLIF",
    "NUMTODSINTERVAL",
    "NUMTOYMINTERVAL",
    "NVL",
    "NVL2",
    "ORA_HASH",
    "PERCENTILE_CONT",
    "PERCENTILE_DISC",
    "PERCENT_RANK",
    "POWER",
    "RANK",
    "RATIO_TO_REPORT",
    "RAWTOHEX",
    "RAWTONHEX",
    "REGEXP_COUNT",
    "REGEXP_INSTR",
    "REGEXP_LIKE",
    "REGEXP_REPLACE",
    "REGEXP_SUBSTR",
    "REGR_AVGX",
    "REGR_AVGY",
    "REGR_COUNT",
    "REGR_INTERCEPT",
    "REGR_R2",
    "REGR_SLOPE",
    "REGR_SXX",
    "REGR_SXY",
    "REGR_SYY",
    "REMAINDER",
    "REPLACE",
    "ROUND",
    "ROUND_TIES_TO_EVEN",
    "ROWIDTOCHAR",
    "ROWIDTONCHAR",
    "ROW_NUMBER",
    "RPAD",
    "RTRIM",
    "SIGN",
    "SIN",
    "SINH",
    "SKEWNESS_POP",
    "SKEWNESS_SAMP",
    "SOUNDEX",
    "SQRT",
    "STANDARD_HASH",
    "STDDEV",
    "STDDEV_POP",
    "STDDEV_SAMP",
    "SUBSTR",
    "SUBSTR2",
    "SUBSTR4",
    "SUBSTRB",
    "SUBSTRC",
    "SUM",
    "SYSTIMESTAMP",
    "SYS_CONNECT_BY_PATH",
    "SYS_CONTEXT",
    "SYS_EXTRACT_UTC",
    "SYS_GUID",
    "TAN",
    "TANH",
    "TIME_BUCKET",
    "TO_BINARY_DOUBLE",
    "TO_BINARY_FLOAT",
    "TO_BLOB",
    "TO_BOOLEAN",
    "TO_CHAR",
    "TO_CLOB",
    "TO_DATE",
    "TO_DSINTERVAL",
    "TO_MULTI_BYTE",
    "TO_NCHAR",
    "TO_NCLOB",
    "TO_NUMBER",
    "TO_SINGLE_BYTE",
    "TO_TIMESTAMP",
    "TO_TIMESTAMP_TZ",
    "TO_YMINTERVAL",
    "TRANSLATE",
    "TRIM",
    "TRUNC",
    "TZ_OFFSET",
    "UNISTR",
    "UPPER",
    "USERENV",
    "VALIDATE_CONVERSION",
    "VARIANCE",
    "VAR_POP",
    "VAR_SAMP",
    "VSIZE",
    "WIDTH_BUCKET",
    "XMLELEMENT",
    "XMLFOREST",
];

/// The fields `EXTRACT(field FROM value)` takes.
const DATETIME_FIELDS: &[&str] = &[
    "YEAR",
    "MONTH",
    "DAY",
    "HOUR",
    "MINUTE",
    "SECOND",
    "TIMEZONE_HOUR",
    "TIMEZONE_MINUTE",
    "TIMEZONE_REGION",
    "TIMEZONE_ABBR",
];

/// Strip trailing `;`s (and nothing else) from `tokens`.
fn without_terminators<'a, 'b>(tokens: &'b [Token<'a>]) -> &'b [Token<'a>] {
    let mut end = tokens.len();
    while end > 0 && tokens[end - 1].is_symbol(';') {
        end -= 1;
    }
    &tokens[..end]
}

/// Check that `sql` is a statement a read-only connection may run (see the
/// [module documentation](self)).
///
/// # Errors
///
/// * The reason the statement is refused
pub(super) fn check_read_only(sql: &str) -> Result<(), String> {
    let tokens =
        tokenize(sql).map_err(|error| format!("the statement can't be checked: {error}"))?;
    let tokens = without_terminators(&tokens);
    let Some(first) = tokens.first() else {
        return Err("the statement is empty".to_string());
    };
    if tokens.iter().any(|token| token.is_symbol(';')) {
        return Err("only one statement can run at a time".to_string());
    }
    if !(first.is_word("SELECT") || first.is_word("WITH") || first.is_symbol('(')) {
        return Err(format!(
            "only queries (SELECT or WITH) can run on a read-only connection, not {}",
            first.text.to_ascii_uppercase()
        ));
    }
    let column_lists = cte_column_lists(tokens);
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            Kind::Word => {
                let upper = token.upper();
                if DENIED_WORDS.contains(&upper.as_str()) {
                    return Err(format!(
                        "{upper} isn't allowed on a read-only connection (it writes, runs code, \
                         changes the session or advances a sequence)"
                    ));
                }
                if let Some(prefix) = DENIED_PREFIXES
                    .iter()
                    .find(|prefix| upper.starts_with(**prefix))
                {
                    return Err(format!(
                        "{upper} isn't allowed on a read-only connection (names starting with \
                         {prefix} are Oracle packages that may change data, the session or the \
                         world outside the database)"
                    ));
                }
            }
            Kind::Symbol if token.is_symbol('@') => {
                return Err(
                    "database links (@) aren't allowed on a read-only connection: the remote \
                     database's side effects can't be checked"
                        .to_string(),
                );
            }
            _ => {}
        }
        if tokens
            .get(index + 1)
            .is_some_and(|next| next.is_symbol('('))
        {
            check_call(tokens, index, &column_lists)?;
        }
    }
    Ok(())
}

/// Check the token at `index`, which a `(` follows: a call must be an
/// allowed built-in function.
fn check_call(tokens: &[Token<'_>], index: usize, column_lists: &[usize]) -> Result<(), String> {
    let token = &tokens[index];
    // The `(+)` outer-join marker.
    if tokens.get(index + 2).is_some_and(|t| t.is_symbol('+'))
        && tokens.get(index + 3).is_some_and(|t| t.is_symbol(')'))
    {
        return Ok(());
    }
    if column_lists.contains(&(index + 1)) {
        return Ok(());
    }
    let qualified = index > 0 && tokens[index - 1].is_symbol('.');
    match token.kind {
        Kind::Symbol | Kind::String | Kind::Number => Ok(()),
        Kind::Word if !qualified => {
            let upper = token.upper();
            // `EXTRACT(field FROM value)`: called any other way, the name
            // can reach a user function of the same name.
            if upper == "EXTRACT" {
                let field = tokens.get(index + 2).map(Token::upper).unwrap_or_default();
                return if DATETIME_FIELDS.contains(&field.as_str())
                    && tokens.get(index + 3).is_some_and(|t| t.is_word("FROM"))
                {
                    Ok(())
                } else {
                    Err(
                        "EXTRACT is allowed on a read-only connection only as EXTRACT(field \
                         FROM value)"
                            .to_string(),
                    )
                };
            }
            if RESERVED_PAREN_WORDS.contains(&upper.as_str())
                || BUILTIN_FUNCTIONS.contains(&upper.as_str())
            {
                Ok(())
            } else if CONTEXT_PAREN_KEYWORDS.contains(&upper.as_str()) {
                if keyword_before_paren(tokens, index) {
                    Ok(())
                } else {
                    Err(format!(
                        "{upper}(...) isn't allowed on a read-only connection here: {upper} \
                         isn't reserved, so outside the clauses where Oracle reads it as a \
                         keyword it calls any function of that name, which may write in a \
                         transaction of its own"
                    ))
                }
            } else {
                Err(format!(
                    "{upper}(...) isn't allowed on a read-only connection: only Oracle's \
                     built-in SQL functions can be called, since any other function may write \
                     in a transaction of its own"
                ))
            }
        }
        _ => Err(format!(
            "{}{}(...) isn't allowed on a read-only connection: only Oracle's built-in SQL \
             functions can be called, unqualified",
            if qualified { "." } else { "" },
            token.text
        )),
    }
}

/// The positions of the `(`s that open a `WITH` clause's column lists:
/// `WITH name (a, b) AS (...)`.
fn cte_column_lists(tokens: &[Token<'_>]) -> Vec<usize> {
    let mut lists = Vec::new();
    if !tokens.first().is_some_and(|token| token.is_word("WITH")) {
        return lists;
    }
    let mut at = 1;
    loop {
        // name
        if !tokens
            .get(at)
            .is_some_and(|t| matches!(t.kind, Kind::Word | Kind::Quoted))
        {
            return lists;
        }
        at += 1;
        // (column, ...)
        if tokens.get(at).is_some_and(|t| t.is_symbol('(')) {
            let open = at;
            at += 1;
            loop {
                if !tokens
                    .get(at)
                    .is_some_and(|t| matches!(t.kind, Kind::Word | Kind::Quoted))
                {
                    return lists;
                }
                at += 1;
                match tokens.get(at) {
                    Some(t) if t.is_symbol(',') => at += 1,
                    Some(t) if t.is_symbol(')') => {
                        at += 1;
                        break;
                    }
                    _ => return lists,
                }
            }
            if !tokens.get(at).is_some_and(|t| t.is_word("AS")) {
                return lists;
            }
            lists.push(open);
        }
        // AS [NOT] [MATERIALIZED] ( query )
        if !tokens.get(at).is_some_and(|t| t.is_word("AS")) {
            return lists;
        }
        at += 1;
        if !tokens.get(at).is_some_and(|t| t.is_symbol('(')) {
            return lists;
        }
        let mut depth = 0_usize;
        while let Some(token) = tokens.get(at) {
            if token.is_symbol('(') {
                depth += 1;
            } else if token.is_symbol(')') {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            at += 1;
        }
        at += 1;
        // SEARCH / CYCLE clauses hold SET, which the check refuses anyway.
        if tokens.get(at).is_some_and(|t| t.is_symbol(',')) {
            at += 1;
        } else {
            return lists;
        }
    }
}

/// How the statement's bind variables map to parameters.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Bound {
    /// The statement, each placeholder renumbered `:1`, `:2`, ... in the
    /// order it appears, and placeholders whose value is written into the
    /// statement replaced by it.
    pub sql: String,
    /// For each placeholder left, in order, the index of its parameter.
    pub params: Vec<usize>,
}

/// Map the `:1`, `:2`, ... placeholders of `sql` (outside literals and
/// comments; a number may repeat) onto `count` parameters. `inline` gives
/// the SQL to write instead of binding parameter `index`, if any (the
/// "now" values). A placeholder named `output` is an output variable the
/// caller binds after the parameters; it is left as it is.
///
/// Without parameters the statement is left untouched: `:NEW` / `:OLD` in a
/// trigger body, say, are not placeholders.
///
/// # Errors
///
/// * The statement can't be tokenized, names a placeholder other than by
///   number, names a parameter that doesn't exist, or leaves one unused
pub(super) fn bind_placeholders(
    sql: &str,
    count: usize,
    output: Option<&str>,
    inline: impl Fn(usize) -> Option<String>,
) -> Result<Bound, String> {
    if count == 0 {
        return Ok(Bound {
            sql: sql.to_string(),
            params: Vec::new(),
        });
    }
    let tokens = tokenize(sql)?;
    let mut out = String::with_capacity(sql.len());
    let mut params = Vec::new();
    let mut used = vec![false; count];
    let mut copied = 0;
    for token in tokens.iter().filter(|token| token.kind == Kind::Bind) {
        let name = &token.text[1..];
        if output.is_some_and(|output| name.eq_ignore_ascii_case(output)) {
            continue;
        }
        let number: usize = name.parse().map_err(|_| {
            format!(
                "the placeholder {} isn't numbered; use :1, :2, ... with parameters",
                token.text
            )
        })?;
        let index = number
            .checked_sub(1)
            .filter(|&index| index < count)
            .ok_or_else(|| {
                format!(
                    "the placeholder {} has no parameter ({count} given)",
                    token.text
                )
            })?;
        used[index] = true;
        out.push_str(&sql[copied..token.start]);
        if let Some(expression) = inline(index) {
            out.push_str(&expression);
        } else {
            params.push(index);
            out.push(':');
            out.push_str(&params.len().to_string());
        }
        copied = token.start + token.text.len();
    }
    out.push_str(&sql[copied..]);
    if let Some(unused) = used.iter().position(|used| !used) {
        return Err(format!(
            "parameter {} isn't used by the statement (placeholders are :1, :2, ...)",
            unused + 1
        ));
    }
    Ok(Bound { sql: out, params })
}

/// Words after `CREATE [OR REPLACE] [EDITIONABLE | NONEDITIONABLE |
/// EDITIONING]` that make a PL/SQL unit, whose text ends with `END;`.
const PLSQL_UNITS: &[&str] = &[
    "PROCEDURE",
    "FUNCTION",
    "PACKAGE",
    "TRIGGER",
    "TYPE",
    "LIBRARY",
    "JAVA",
];

/// Whether `sql` is PL/SQL (an anonymous block, or the source of a stored
/// unit), whose final `;` belongs to it.
pub(super) fn is_plsql(tokens: &[Token<'_>]) -> bool {
    let Some(first) = tokens.first() else {
        return false;
    };
    if first.is_word("BEGIN") || first.is_word("DECLARE") {
        return true;
    }
    if !first.is_word("CREATE") {
        return false;
    }
    tokens[1..]
        .iter()
        .take(5)
        .find(|token| {
            !(token.is_word("OR")
                || token.is_word("REPLACE")
                || token.is_word("EDITIONABLE")
                || token.is_word("NONEDITIONABLE")
                || token.is_word("EDITIONING")
                || token.is_word("AND")
                || token.is_word("COMPILE")
                || token.is_word("RESOLVE")
                || token.is_word("NOFORCE")
                || token.is_word("SOURCE")
                || token.is_word("FORCE"))
        })
        .is_some_and(|token| {
            PLSQL_UNITS
                .iter()
                .any(|unit| token.text.eq_ignore_ascii_case(unit))
        })
}

/// `sql` without the `;` that ends a SQL statement (which Oracle refuses
/// when sent through a client library). PL/SQL keeps its final `;`, and
/// text that can't be tokenized is left as it is (the server decides).
pub(super) fn strip_terminator(sql: &str) -> &str {
    let Ok(tokens) = tokenize(sql) else {
        return sql;
    };
    if is_plsql(&tokens) {
        return sql;
    }
    match tokens.last() {
        Some(last) if last.is_symbol(';') => {
            let before = &tokens[..tokens.len() - 1];
            before
                .last()
                .map_or("", |token| &sql[..token.start + token.text.len()])
        }
        _ => sql,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    fn kinds(sql: &str) -> Vec<(Kind, &str)> {
        tokenize(sql)
            .unwrap()
            .into_iter()
            .map(|token| (token.kind, token.text))
            .collect()
    }

    #[test]
    fn literals_comments_and_names_tokenize_as_oracle_reads_them() {
        assert_eq!(
            kinds("SELECT q'[it's ] x]', nq'{a}', N'b''c', \"Mixed\" FROM dual -- c\n/* d */"),
            vec![
                (Kind::Word, "SELECT"),
                (Kind::String, "q'[it's ] x]'"),
                (Kind::Symbol, ","),
                (Kind::String, "nq'{a}'"),
                (Kind::Symbol, ","),
                (Kind::String, "N'b''c'"),
                (Kind::Symbol, ","),
                (Kind::Quoted, "\"Mixed\""),
                (Kind::Word, "FROM"),
                (Kind::Word, "dual"),
            ]
        );
        assert_eq!(
            kinds("q'!a'b!' Q'|x|' v$session :1 :name 1.5e-3 2f .5D"),
            vec![
                (Kind::String, "q'!a'b!'"),
                (Kind::String, "Q'|x|'"),
                (Kind::Word, "v$session"),
                (Kind::Bind, ":1"),
                (Kind::Bind, ":name"),
                (Kind::Number, "1.5e-3"),
                (Kind::Number, "2f"),
                (Kind::Number, ".5D"),
            ]
        );
        // Comments don't nest: the first */ ends this one.
        assert_eq!(
            kinds("/* a /* b */ x */"),
            vec![(Kind::Word, "x"), (Kind::Symbol, "*"), (Kind::Symbol, "/")]
        );
    }

    #[test]
    fn text_oracle_could_read_differently_is_refused() {
        for sql in [
            "SELECT 'a FROM dual",
            "SELECT q'[a' FROM dual",
            "SELECT q' a ' FROM dual",
            "SELECT \"a FROM dual",
            "SELECT \"a\"\"b\" FROM dual",
            "SELECT \"\" FROM dual",
            "SELECT 1 /* x",
            "SELECT 1x FROM dual",
            "SELECT 1fx FROM dual",
            "SELECT 1 -- a\rDELETE FROM t",
            "SELECT 1\u{a0}FROM dual",
            "SELECT 1\u{0}FROM dual",
        ] {
            assert!(tokenize(sql).is_err(), "{sql:?}");
            assert!(check_read_only(sql).is_err(), "{sql:?}");
        }
        // CRLF line endings are fine.
        assert!(check_read_only("SELECT 1 -- a\r\nFROM dual").is_ok());
    }

    #[test]
    fn reads_pass_the_read_only_check() {
        for sql in [
            "SELECT * FROM shop.albums",
            "select count(*) from albums;",
            "(SELECT 1 FROM dual) UNION ALL (SELECT 2 FROM dual)",
            "WITH t (a, b) AS (SELECT 1, 2 FROM dual), u AS (SELECT a FROM t) SELECT * FROM u",
            "SELECT a.id, b.name FROM a, b WHERE a.id = b.id(+)",
            "SELECT LEVEL, SYS_CONNECT_BY_PATH(name, '/') FROM t CONNECT BY PRIOR id = parent_id START WITH parent_id IS NULL",
            "SELECT * FROM t WHERE ROWNUM <= 10",
            "SELECT * FROM t ORDER BY id OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY",
            "SELECT q'[it's; DELETE]' FROM dual",
            "SELECT CAST(price AS NUMBER(10, 2)), TO_CHAR(created, 'YYYY') FROM t",
            "SELECT LISTAGG(name, ',') WITHIN GROUP (ORDER BY name) FROM t",
            "SELECT ROW_NUMBER() OVER (PARTITION BY a ORDER BY b) FROM t",
            "SELECT * FROM t AS OF TIMESTAMP (SYSTIMESTAMP - INTERVAL '1' HOUR)",
            "SELECT jt.* FROM t, JSON_TABLE(t.doc, '$' COLUMNS (a NUMBER PATH '$.a')) jt",
            "SELECT * FROM t WHERE id IN (:1, :2)",
            "SELECT \"type\", \"comment\" FROM t",
            "SELECT SYS_CONTEXT('USERENV', 'CURRENT_SCHEMA') FROM dual",
            "SELECT EXTRACT(YEAR FROM SYSDATE) FROM dual",
            "SELECT * FROM v$session",
            // Unreserved keywords where the grammar makes them keywords.
            "SELECT * FROM (SELECT a, b FROM t) PIVOT (SUM(b) FOR a IN (1 AS one))",
            "SELECT * FROM t PIVOT (COUNT(*) FOR a IN ('X' AS x))",
            "SELECT * FROM shop.t UNPIVOT (v FOR k IN (a, b))",
            "SELECT * FROM a JOIN (SELECT 1 x FROM dual) b ON 1 = 1",
            "SELECT * FROM a LEFT JOIN (b JOIN c ON b.x = c.x) ON a.x = b.x",
            "SELECT * FROM a JOIN b ON a.id = b.id JOIN (SELECT 1 x FROM dual) c ON 1 = 1",
            "SELECT * FROM a CROSS JOIN LATERAL (SELECT 1 x FROM dual) b",
            "SELECT * FROM a CROSS APPLY (SELECT 1 x FROM dual) b",
            "SELECT * FROM a JOIN b USING (id)",
            "SELECT * FROM t SAMPLE (5) SEED (1)",
            "SELECT * FROM t PARTITION (p1)",
            "SELECT MAX(a) KEEP (DENSE_RANK FIRST ORDER BY b) FROM t",
            "SELECT CAST(SYSDATE AS TIMESTAMP(6)), CAST('a' AS NCHAR(3)) FROM dual",
            "SELECT CAST(x AS INTERVAL DAY(2) TO SECOND(6)), INTERVAL '5' DAY(3) FROM t",
            "SELECT JSON_VALUE(doc, '$.a' RETURNING TIMESTAMP(6)) FROM t",
            "SELECT CASE WHEN (a = 1) THEN 'x' WHEN (a = 2) THEN 'y' END FROM t",
            "SELECT CAST(MULTISET(SELECT 1 FROM dual) AS sys.odcinumberlist) FROM dual",
            "SELECT 1 FROM dual WHERE 1 = SOME (SELECT 1 FROM dual)",
            "SELECT * FROM t MATCH_RECOGNIZE (ORDER BY a ONE ROW PER MATCH PATTERN (x y*) DEFINE y AS y.a > 1)",
        ] {
            assert_eq!(check_read_only(sql), Ok(()), "{sql}");
        }
    }

    #[test]
    fn unreserved_keywords_called_as_functions_are_refused() {
        for word in CONTEXT_PAREN_KEYWORDS {
            for sql in [
                format!("SELECT {word}(1) FROM dual"),
                format!("SELECT 1 FROM dual WHERE {word}(1) = 1"),
                format!("SELECT 1 + {word}(1) FROM dual"),
                format!("SELECT a, {word}(1) FROM t"),
                format!("SELECT * FROM t ORDER BY a NULLS FIRST, {word}(1)"),
            ] {
                assert!(check_read_only(&sql).is_err(), "{sql}");
            }
            // Right after CASE, Oracle reads WHEN as the keyword (`CASE
            // when(1) WHEN 1` is ORA-02000, missing THEN), never a call.
            if *word != "WHEN" {
                let sql = format!("SELECT CASE {word}(1) WHEN 1 THEN 2 END FROM t");
                assert!(check_read_only(&sql).is_err(), "{sql}");
            }
        }
        for sql in [
            "SELECT day(1), first(2), cursor(3) FROM dual",
            "SELECT seed(1) FROM dual",
            "SELECT first(1) FROM dual",
            "SELECT next(1) FROM dual",
            "SELECT CASE (1) WHEN 1 THEN 2 END FROM dual",
            "SELECT * FROM t WHERE a = 1 AND pivot(1) = 1",
            "SELECT COUNT(*) OVER (ORDER BY a ROWS pattern(1) PRECEDING) FROM t",
            "SELECT * FROM t WHERE a = b AND x MEMBER OF timestamp(1)",
            "SELECT CAST(nchar(1) AS NUMBER) FROM dual",
            "SELECT * FROM t MATCH_RECOGNIZE (PATTERN (a) DEFINE a AS nchar(1) = 1)",
            "SELECT * FROM t WHERE a = SOME (1, 2)",
        ] {
            assert!(check_read_only(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn writes_code_and_side_effects_are_refused() {
        for sql in [
            "INSERT INTO t VALUES (1)",
            "UPDATE t SET a = 1",
            "DELETE FROM t",
            "MERGE INTO t USING s ON (1 = 1) WHEN MATCHED THEN UPDATE SET a = 1",
            "CREATE TABLE x (a NUMBER)",
            "ALTER SESSION SET READ_ONLY = FALSE",
            "BEGIN NULL; END;",
            "DECLARE x NUMBER; BEGIN NULL; END;",
            "CALL p()",
            "EXPLAIN PLAN FOR SELECT 1 FROM dual",
            "LOCK TABLE t IN EXCLUSIVE MODE",
            "COMMIT",
            "SET TRANSACTION READ WRITE",
            "SELECT * FROM t FOR UPDATE",
            "SELECT * FROM t FOR UPDATE NOWAIT",
            "SELECT s.NEXTVAL FROM dual",
            "SELECT my_function(1) FROM dual",
            "SELECT shop.my_function(1) FROM dual",
            "SELECT shop.pkg.fn(1) FROM dual",
            "SELECT \"my_function\"(1) FROM dual",
            "SELECT DBMS_RANDOM.VALUE FROM dual",
            "SELECT dbms_pipe.receive_message('x') FROM dual",
            "SELECT UTL_HTTP.REQUEST('http://example.com') FROM dual",
            "SELECT * FROM t@remote",
            "WITH FUNCTION f RETURN NUMBER IS BEGIN RETURN 1; END; SELECT f FROM dual",
            "SELECT 1 FROM dual; DELETE FROM t",
            "SELECT XMLTYPE('<a/>') FROM dual",
            "SELECT EXTRACT(1) FROM dual",
            "SELECT BFILENAME('DIR', 'f') FROM dual",
            "SELECT x.getclobval() FROM t x",
            "SELECT 1 INTO :x FROM dual",
            "",
            ";",
        ] {
            assert!(check_read_only(sql).is_err(), "{sql}");
        }
    }

    #[test]
    fn placeholders_are_renumbered_in_order() {
        let bound = bind_placeholders(
            "SELECT * FROM t WHERE a = :2 AND b = :1 AND c = ':1' AND d = :2",
            2,
            None,
            |_| None,
        )
        .unwrap();
        assert_eq!(
            bound.sql,
            "SELECT * FROM t WHERE a = :1 AND b = :2 AND c = ':1' AND d = :3"
        );
        assert_eq!(bound.params, vec![1, 0, 1]);
        let bound = bind_placeholders("SELECT :1, :2 FROM dual", 2, None, |index| {
            (index == 0).then(|| "SYSTIMESTAMP".to_string())
        })
        .unwrap();
        assert_eq!(bound.sql, "SELECT SYSTIMESTAMP, :1 FROM dual");
        assert_eq!(bound.params, vec![1]);
        assert!(bind_placeholders("SELECT :name FROM dual", 1, None, |_| None).is_err());
        assert!(bind_placeholders("SELECT :3 FROM dual", 1, None, |_| None).is_err());
        assert!(bind_placeholders("SELECT :1 FROM dual", 2, None, |_| None).is_err());
        // Without parameters, nothing is a placeholder.
        let trigger = "CREATE TRIGGER x BEFORE INSERT ON t FOR EACH ROW BEGIN :NEW.a := 1; END;";
        assert_eq!(
            bind_placeholders(trigger, 0, None, |_| None).unwrap().sql,
            trigger
        );
    }

    #[test]
    fn statements_lose_their_terminator_but_plsql_keeps_it() {
        assert_eq!(
            strip_terminator("SELECT 1 FROM dual ;  "),
            "SELECT 1 FROM dual"
        );
        assert_eq!(
            strip_terminator("SELECT ';' FROM dual; -- done"),
            "SELECT ';' FROM dual"
        );
        assert_eq!(strip_terminator("BEGIN NULL; END;"), "BEGIN NULL; END;");
        let unit = "CREATE OR REPLACE EDITIONABLE PROCEDURE p IS BEGIN NULL; END;";
        assert_eq!(strip_terminator(unit), unit);
        assert_eq!(
            strip_terminator("CREATE TABLE t (a NUMBER);"),
            "CREATE TABLE t (a NUMBER)"
        );
    }
}
