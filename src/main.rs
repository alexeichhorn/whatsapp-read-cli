use clap::{Parser, Subcommand, ValueEnum};
use rusqlite::types::{ToSql, Value, ValueRef};
use rusqlite::{Connection, OpenFlags, params_from_iter};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_GROUP: &str = "group.net.whatsapp.WhatsApp.shared";

#[derive(Parser)]
#[command(version, about = "Read-only CLI for local macOS WhatsApp data")]
struct Cli {
    #[arg(long, value_name = "DIR", global = true)]
    root: Option<PathBuf>,

    #[arg(long, value_enum, default_value_t = Format::Human, global = true)]
    format: Format,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Human,
    Json,
    Tsv,
}

#[derive(Subcommand)]
enum Command {
    /// Show local WhatsApp instance summary without message content.
    Info,
    /// List chats.
    Chats {
        #[arg(long)]
        unread: bool,
        #[arg(long, default_value_t = 50)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// List messages in one chat.
    Messages {
        /// Chat id from `chats`, or a name/JID substring that matches exactly one chat.
        #[arg(long)]
        chat: String,
        #[arg(long)]
        unread: bool,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// List contacts.
    Contacts {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// List calls.
    Calls {
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Search message text globally or within one chat.
    Search {
        text: String,
        #[arg(long)]
        chat: Option<String>,
        #[arg(long, default_value_t = 100)]
        limit: usize,
        #[arg(long, default_value_t = 0)]
        offset: usize,
    },
    /// Read-only database inspection escape hatches.
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },
}

#[derive(Subcommand)]
enum DebugCommand {
    /// Show discovered WhatsApp SQLite databases.
    Dbs,
    /// List tables in a database.
    Tables {
        #[arg(long, default_value = "chat")]
        db: String,
    },
    /// Print CREATE TABLE/INDEX SQL for one table or the whole database.
    Schema {
        #[arg(long, default_value = "chat")]
        db: String,
        table: Option<String>,
    },
    /// Run a read-only SELECT query.
    Query {
        #[arg(long, default_value = "chat")]
        db: String,
        sql: String,
    },
}

struct RowSet {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

struct ChatMatch {
    pk: i64,
    jid: String,
    name: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let root = cli.root.unwrap_or(default_root()?);
    let rows = match cli.command {
        Command::Info => info(&root)?,
        Command::Chats {
            unread,
            limit,
            offset,
        } => chats(&root, unread, limit, offset)?,
        Command::Messages {
            chat,
            unread,
            limit,
            offset,
        } => messages(&root, &chat, unread, limit, offset)?,
        Command::Contacts { limit, offset } => contacts(&root, limit, offset)?,
        Command::Calls { limit, offset } => calls(&root, limit, offset)?,
        Command::Search {
            text,
            chat,
            limit,
            offset,
        } => search(&root, &text, chat.as_deref(), limit, offset)?,
        Command::Debug { command } => debug(&root, command)?,
    };

    render(&rows, cli.format);
    Ok(())
}

fn default_root() -> Result<PathBuf, Box<dyn Error>> {
    let home = env::var("HOME")?;
    Ok(PathBuf::from(home)
        .join("Library")
        .join("Group Containers")
        .join(DEFAULT_GROUP))
}

fn resolve_db(root: &Path, db: &str) -> Result<PathBuf, Box<dyn Error>> {
    let path = match db {
        "chat" => root.join("ChatStorage.sqlite"),
        "contacts" => root.join("ContactsV2.sqlite"),
        "calls" => root.join("CallHistory.sqlite"),
        "local-storage" => root.join("LocalStorageDB/LocalStorageDatabase.sqlite"),
        "ext-chat" => root.join("ExtChatDB/ExtChatDatabase.sqlite"),
        "backed-up" => root.join("BackedUpStorageDB/BackedUpStorageDatabase.sqlite"),
        _ => {
            let path = PathBuf::from(db);
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        }
    };

    if path.exists() {
        Ok(path)
    } else {
        Err(format!("database not found: {}", path.display()).into())
    }
}

fn open_db(path: &Path) -> Result<Connection, Box<dyn Error>> {
    let uri = format!("file:{}?mode=ro&immutable=1", sqlite_uri_path(path));
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI;
    let conn = Connection::open_with_flags(uri, flags)?;
    conn.pragma_update(None, "query_only", "ON")?;
    Ok(conn)
}

fn info(root: &Path) -> Result<RowSet, Box<dyn Error>> {
    let mut paths = Vec::new();
    collect_dbs(root, &mut paths)?;

    let chat = open_db(&resolve_db(root, "chat")?)?;
    let contacts = open_db(&resolve_db(root, "contacts")?)?;
    let calls = open_db(&resolve_db(root, "calls")?)?;
    let rows = vec![
        vec!["root".to_string(), root.display().to_string()],
        vec!["read_only".to_string(), "true".to_string()],
        vec!["databases".to_string(), paths.len().to_string()],
        vec!["chats".to_string(), count(&chat, "ZWACHATSESSION")?.to_string()],
        vec![
            "unread_chats".to_string(),
            scalar_i64(
                &chat,
                "SELECT count(*) FROM ZWACHATSESSION WHERE ifnull(ZREMOVED, 0) = 0 AND ifnull(ZUNREADCOUNT, 0) > 0",
                &[],
            )?
            .to_string(),
        ],
        vec!["messages".to_string(), count(&chat, "ZWAMESSAGE")?.to_string()],
        vec![
            "contacts".to_string(),
            count(&contacts, "ZWAADDRESSBOOKCONTACT")?.to_string(),
        ],
        vec!["calls".to_string(), count(&calls, "ZWACDCALLEVENT")?.to_string()],
        vec![
            "latest_message_utc".to_string(),
            scalar_string(
                &chat,
                "SELECT ifnull(datetime(max(ZMESSAGEDATE) + 978307200, 'unixepoch'), '') FROM ZWAMESSAGE",
            )?,
        ],
    ];

    Ok(RowSet {
        headers: vec!["item".to_string(), "value".to_string()],
        rows,
    })
}

fn chats(root: &Path, unread: bool, limit: usize, offset: usize) -> Result<RowSet, Box<dyn Error>> {
    let conn = open_db(&resolve_db(root, "chat")?)?;
    let unread_filter = if unread {
        "AND ifnull(ZUNREADCOUNT, 0) > 0"
    } else {
        ""
    };
    let sql = format!(
        "SELECT
            Z_PK AS chat_id,
            ZPARTNERNAME AS name,
            ZCONTACTJID AS jid,
            ZUNREADCOUNT AS unread,
            ZMESSAGECOUNTER AS messages,
            datetime(ZLASTMESSAGEDATE + 978307200, 'unixepoch') AS last_message_utc,
            ZLASTMESSAGETEXT AS last_message
        FROM ZWACHATSESSION
        WHERE ifnull(ZREMOVED, 0) = 0 {unread_filter}
        ORDER BY ZLASTMESSAGEDATE DESC
        {page_clause}",
        page_clause = page_clause(limit, offset),
    );

    query_rows(&conn, &sql, &[])
}

fn messages(
    root: &Path,
    chat: &str,
    unread: bool,
    limit: usize,
    offset: usize,
) -> Result<RowSet, Box<dyn Error>> {
    let conn = open_db(&resolve_db(root, "chat")?)?;
    let chat_id = resolve_chat(&conn, chat)?;
    let mut filters = vec!["m.ZCHATSESSION = ?".to_string()];
    let params = vec![Value::Integer(chat_id)];
    let limit = if unread {
        let unread_count = chat_unread_count(&conn, chat_id)?;
        if unread_count == 0 {
            return Ok(message_headers());
        }
        filters.push("ifnull(m.ZISFROMME, 0) = 0".to_string());
        if limit == 0 {
            unread_count as usize
        } else {
            limit.min(unread_count as usize)
        }
    } else {
        limit
    };

    let sql = format!(
        "SELECT
            m.Z_PK AS message_id,
            datetime(m.ZMESSAGEDATE + 978307200, 'unixepoch') AS message_utc,
            CASE m.ZISFROMME WHEN 1 THEN 'me' ELSE ifnull(m.ZPUSHNAME, '') END AS sender,
            m.ZFROMJID AS from_jid,
            m.ZTOJID AS to_jid,
            m.ZMESSAGETYPE AS type,
            m.ZTEXT AS text
        FROM ZWAMESSAGE m
        WHERE {where_clause}
        ORDER BY m.ZMESSAGEDATE DESC
        {page_clause}",
        where_clause = filters.join(" AND "),
        page_clause = page_clause(limit, offset),
    );
    let params = to_params(&params);
    query_rows(&conn, &sql, &params)
}

fn message_headers() -> RowSet {
    RowSet {
        headers: vec![
            "message_id".to_string(),
            "message_utc".to_string(),
            "sender".to_string(),
            "from_jid".to_string(),
            "to_jid".to_string(),
            "type".to_string(),
            "text".to_string(),
        ],
        rows: Vec::new(),
    }
}

fn contacts(root: &Path, limit: usize, offset: usize) -> Result<RowSet, Box<dyn Error>> {
    let conn = open_db(&resolve_db(root, "contacts")?)?;
    let sql = format!(
        "SELECT
            Z_PK AS contact_id,
            ZFULLNAME AS full_name,
            ZWHATSAPPID AS whatsapp_id,
            ZPHONENUMBER AS phone,
            ZLOCALIZEDPHONENUMBER AS localized_phone,
            ZUSERNAME AS username,
            ZABOUTTEXT AS about
        FROM ZWAADDRESSBOOKCONTACT
        ORDER BY ZSORT ASC
        {page_clause}",
        page_clause = page_clause(limit, offset),
    );

    query_rows(&conn, &sql, &[])
}

fn calls(root: &Path, limit: usize, offset: usize) -> Result<RowSet, Box<dyn Error>> {
    let conn = open_db(&resolve_db(root, "calls")?)?;
    let sql = format!(
        "SELECT
            Z_PK AS call_id,
            datetime(ZDATE + 978307200, 'unixepoch') AS call_utc,
            ZDURATION AS duration_seconds,
            ZOUTCOME AS outcome,
            ZGROUPJIDSTRING AS group_jid,
            ZGROUPCALLCREATORUSERJIDSTRING AS creator_jid
        FROM ZWACDCALLEVENT
        ORDER BY ZDATE DESC
        {page_clause}",
        page_clause = page_clause(limit, offset),
    );

    query_rows(&conn, &sql, &[])
}

fn search(
    root: &Path,
    text: &str,
    chat: Option<&str>,
    limit: usize,
    offset: usize,
) -> Result<RowSet, Box<dyn Error>> {
    let conn = open_db(&resolve_db(root, "chat")?)?;
    let mut filters = vec!["m.ZTEXT LIKE ?".to_string()];
    let mut params = vec![Value::Text(format!("%{text}%"))];

    if let Some(chat) = chat {
        filters.push("m.ZCHATSESSION = ?".to_string());
        params.push(Value::Integer(resolve_chat(&conn, chat)?));
    }

    let sql = format!(
        "SELECT
            m.Z_PK AS message_id,
            m.ZCHATSESSION AS chat_id,
            c.ZPARTNERNAME AS chat_name,
            c.ZCONTACTJID AS chat_jid,
            datetime(m.ZMESSAGEDATE + 978307200, 'unixepoch') AS message_utc,
            CASE m.ZISFROMME WHEN 1 THEN 'me' ELSE ifnull(m.ZPUSHNAME, '') END AS sender,
            m.ZTEXT AS text
        FROM ZWAMESSAGE m
        LEFT JOIN ZWACHATSESSION c ON c.Z_PK = m.ZCHATSESSION
        WHERE {where_clause}
        ORDER BY m.ZMESSAGEDATE DESC
        {page_clause}",
        where_clause = filters.join(" AND "),
        page_clause = page_clause(limit, offset),
    );
    let params = to_params(&params);
    query_rows(&conn, &sql, &params)
}

fn debug(root: &Path, command: DebugCommand) -> Result<RowSet, Box<dyn Error>> {
    match command {
        DebugCommand::Dbs => list_dbs(root),
        DebugCommand::Tables { db } => {
            let conn = open_db(&resolve_db(root, &db)?)?;
            query_rows(
                &conn,
                "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name",
                &[],
            )
        }
        DebugCommand::Schema { db, table } => {
            let conn = open_db(&resolve_db(root, &db)?)?;
            if let Some(table) = table {
                ensure_identifier(&table)?;
                query_rows(
                    &conn,
                    "SELECT sql FROM sqlite_master WHERE tbl_name = ?1 AND sql IS NOT NULL ORDER BY type, name",
                    &[&table],
                )
            } else {
                query_rows(
                    &conn,
                    "SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY type, name",
                    &[],
                )
            }
        }
        DebugCommand::Query { db, sql } => {
            let sql = readonly_select(&sql)?;
            let conn = open_db(&resolve_db(root, &db)?)?;
            query_rows(&conn, sql, &[])
        }
    }
}

fn list_dbs(root: &Path) -> Result<RowSet, Box<dyn Error>> {
    let mut paths = Vec::new();
    collect_dbs(root, &mut paths)?;
    paths.sort();

    Ok(RowSet {
        headers: vec!["name".to_string(), "path".to_string()],
        rows: paths
            .into_iter()
            .map(|path| vec![db_name(root, &path), path.display().to_string()])
            .collect(),
    })
}

fn collect_dbs(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), Box<dyn Error>> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_dbs(&path, paths)?;
        } else if is_db_file(&path) {
            paths.push(path);
        }
    }

    Ok(())
}

fn is_db_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "sqlite" | "sqlite3" | "db"
            )
        })
        .unwrap_or(false)
}

fn db_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn resolve_chat(conn: &Connection, selector: &str) -> Result<i64, Box<dyn Error>> {
    if let Ok(pk) = selector.parse::<i64>() {
        let exists = scalar_i64(
            conn,
            "SELECT count(*) FROM ZWACHATSESSION WHERE Z_PK = ?1 AND ifnull(ZREMOVED, 0) = 0",
            &[&pk],
        )?;
        if exists == 1 {
            return Ok(pk);
        }
    }

    let pattern = format!("%{selector}%");
    let mut stmt = conn.prepare(
        "SELECT Z_PK, ifnull(ZCONTACTJID, ''), ifnull(ZPARTNERNAME, '')
        FROM ZWACHATSESSION
        WHERE ifnull(ZREMOVED, 0) = 0
          AND (ZCONTACTJID LIKE ?1 OR ZPARTNERNAME LIKE ?1)
        ORDER BY ZLASTMESSAGEDATE DESC
        LIMIT 20",
    )?;
    let matches: Vec<ChatMatch> = stmt
        .query_map([pattern], |row| {
            Ok(ChatMatch {
                pk: row.get(0)?,
                jid: row.get(1)?,
                name: row.get(2)?,
            })
        })?
        .collect::<Result<_, _>>()?;

    match matches.as_slice() {
        [one] => Ok(one.pk),
        [] => Err(format!("no chat matches: {selector}").into()),
        many => {
            let mut message = format!("chat selector matched {} chats; use chat_id:\n", many.len());
            for chat in many {
                message.push_str(&format!("  {}  {}  {}\n", chat.pk, chat.name, chat.jid));
            }
            Err(message.into())
        }
    }
}

fn chat_unread_count(conn: &Connection, chat_id: i64) -> Result<i64, Box<dyn Error>> {
    scalar_i64(
        conn,
        "SELECT ifnull(ZUNREADCOUNT, 0) FROM ZWACHATSESSION WHERE Z_PK = ?1",
        &[&chat_id],
    )
}

fn count(conn: &Connection, table: &str) -> Result<i64, Box<dyn Error>> {
    ensure_identifier(table)?;
    scalar_i64(conn, &format!("SELECT count(*) FROM \"{table}\""), &[])
}

fn scalar_i64(conn: &Connection, sql: &str, params: &[&dyn ToSql]) -> Result<i64, Box<dyn Error>> {
    Ok(conn.query_row(sql, params_from_iter(params.iter()), |row| row.get(0))?)
}

fn scalar_string(conn: &Connection, sql: &str) -> Result<String, Box<dyn Error>> {
    Ok(conn.query_row(sql, [], |row| row.get(0))?)
}

fn to_params(values: &[Value]) -> Vec<&dyn ToSql> {
    values.iter().map(|value| value as &dyn ToSql).collect()
}

fn readonly_select(sql: &str) -> Result<&str, Box<dyn Error>> {
    let sql = sql.trim();
    let sql = sql.strip_suffix(';').unwrap_or(sql).trim();
    if sql.contains(';') {
        return Err("only one SELECT statement is allowed".into());
    }
    if !sql.to_ascii_lowercase().starts_with("select ") {
        return Err("debug query accepts SELECT statements only".into());
    }
    Ok(sql)
}

fn ensure_identifier(identifier: &str) -> Result<(), Box<dyn Error>> {
    let ok = !identifier.is_empty()
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if ok {
        Ok(())
    } else {
        Err(format!("unsafe identifier: {identifier}").into())
    }
}

fn page_clause(limit: usize, offset: usize) -> String {
    match (limit, offset) {
        (0, 0) => String::new(),
        (0, offset) => format!("LIMIT -1 OFFSET {offset}"),
        (limit, 0) => format!("LIMIT {limit}"),
        (limit, offset) => format!("LIMIT {limit} OFFSET {offset}"),
    }
}

fn query_rows(
    conn: &Connection,
    sql: &str,
    params: &[&dyn ToSql],
) -> Result<RowSet, Box<dyn Error>> {
    let mut stmt = conn.prepare(sql)?;
    let headers: Vec<String> = stmt
        .column_names()
        .iter()
        .map(|name| name.to_string())
        .collect();

    let mut rows = stmt.query(params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut cells = Vec::with_capacity(headers.len());
        for index in 0..headers.len() {
            cells.push(cell(row.get_ref(index)?));
        }
        out.push(cells);
    }

    Ok(RowSet { headers, rows: out })
}

fn render(rows: &RowSet, format: Format) {
    match format {
        Format::Human => render_human(rows),
        Format::Json => render_json(rows),
        Format::Tsv => render_tsv(rows),
    }
}

fn render_human(rows: &RowSet) {
    if rows.headers.is_empty() {
        return;
    }

    let display_rows: Vec<Vec<String>> = rows
        .rows
        .iter()
        .map(|row| row.iter().map(|value| truncate(value, 96)).collect())
        .collect();
    let mut widths: Vec<usize> = rows.headers.iter().map(|header| header.len()).collect();
    for row in &display_rows {
        for (index, value) in row.iter().enumerate() {
            widths[index] = widths[index].max(value.len());
        }
    }

    print_human_row(&rows.headers, &widths);
    println!(
        "{}",
        widths
            .iter()
            .map(|width| "-".repeat(*width))
            .collect::<Vec<_>>()
            .join("  ")
    );
    for row in &display_rows {
        print_human_row(row, &widths);
    }
    if rows.rows.is_empty() {
        println!("(no rows)");
    }
}

fn print_human_row(row: &[String], widths: &[usize]) {
    let cells: Vec<String> = row
        .iter()
        .enumerate()
        .map(|(index, value)| format!("{value:<width$}", width = widths[index]))
        .collect();
    println!("{}", cells.join("  "));
}

fn render_tsv(rows: &RowSet) {
    println!("{}", rows.headers.join("\t"));
    for row in &rows.rows {
        println!("{}", row.join("\t"));
    }
}

fn render_json(rows: &RowSet) {
    println!("[");
    for (row_index, row) in rows.rows.iter().enumerate() {
        println!("  {{");
        for (index, header) in rows.headers.iter().enumerate() {
            let comma = if index + 1 == rows.headers.len() {
                ""
            } else {
                ","
            };
            let value = row.get(index).map(String::as_str).unwrap_or("");
            println!(
                "    \"{}\": \"{}\"{}",
                json_escape(header),
                json_escape(value),
                comma
            );
        }
        let comma = if row_index + 1 == rows.rows.len() {
            ""
        } else {
            ","
        };
        println!("  }}{comma}");
    }
    println!("]");
}

fn cell(value: ValueRef<'_>) -> String {
    let cell = match value {
        ValueRef::Null => String::new(),
        ValueRef::Integer(value) => value.to_string(),
        ValueRef::Real(value) => value.to_string(),
        ValueRef::Text(value) => String::from_utf8_lossy(value).to_string(),
        ValueRef::Blob(value) => format!("<{} bytes>", value.len()),
    };

    cell.replace('\t', " ").replace(['\r', '\n'], "\\n")
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}...", &value[..max.saturating_sub(3)])
    }
}

fn json_escape(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            ch if ch.is_control() => out.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => out.push(ch),
        }
    }
    out
}

fn sqlite_uri_path(path: &Path) -> String {
    let path = path.to_string_lossy();
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::{json_escape, page_clause, readonly_select};

    #[test]
    fn raw_query_only_accepts_single_select() {
        assert!(readonly_select("SELECT 1").is_ok());
        assert!(readonly_select("SELECT 1;").is_ok());
        assert!(readonly_select("DELETE FROM ZWAMESSAGE").is_err());
        assert!(readonly_select("SELECT 1; DELETE FROM ZWAMESSAGE").is_err());
    }

    #[test]
    fn json_strings_are_escaped() {
        assert_eq!(json_escape("a\"b\\c"), "a\\\"b\\\\c");
    }

    #[test]
    fn page_clause_supports_all_rows_after_offset() {
        assert_eq!(page_clause(0, 0), "");
        assert_eq!(page_clause(50, 0), "LIMIT 50");
        assert_eq!(page_clause(50, 100), "LIMIT 50 OFFSET 100");
        assert_eq!(page_clause(0, 100), "LIMIT -1 OFFSET 100");
    }
}
