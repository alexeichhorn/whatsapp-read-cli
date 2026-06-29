# whatsapp-read-cli

Read-only Rust CLI for browsing local macOS WhatsApp data. It never talks to WhatsApp servers.

The default WhatsApp data root is auto-detected at:

```text
~/Library/Group Containers/group.net.whatsapp.WhatsApp.shared
```

All SQLite databases are opened with `mode=ro&immutable=1` and `PRAGMA query_only = ON`. Debug SQL only accepts a single `SELECT` statement.

## Install

```sh
cargo install --path . --locked
```

This builds an optimized release binary and installs it into Cargo's bin directory, usually:

```sh
~/.cargo/bin/whatsapp-read-cli
```

Then run it from anywhere:

```sh
whatsapp-read-cli info
```

If your shell cannot find it, add Cargo's bin directory to your `PATH`:

```sh
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

To build a release binary without installing:

```sh
cargo build --release
```

## Agent Skill

This repo includes an agent-facing skill for using this CLI safely:

```text
.agents/skills/whatsapp-read-cli
```

From a checkout of this repo, install it into Codex with:

```sh
mkdir -p "${CODEX_HOME:-$HOME/.codex}/skills"
cp -R .agents/skills/whatsapp-read-cli "${CODEX_HOME:-$HOME/.codex}/skills/"
```

## Global Options

```sh
whatsapp-read-cli --root "/path/to/group.net.whatsapp.WhatsApp.shared" info
whatsapp-read-cli --format json chats
whatsapp-read-cli --format tsv contacts
```

`--root` is optional and only needed when auto-detection is wrong.

`--format` supports:

| format | use |
| --- | --- |
| `human` | default readable table |
| `json` | scripting |
| `tsv` | shell pipelines/spreadsheets |

## Commands

### `info`

Show setup and count information without message text.

```sh
whatsapp-read-cli info
```

Includes the detected root path, database count, chat/contact/call/message counts, unread chat count, read-only status, and latest message timestamp.

### `chats`

List chats.

```sh
whatsapp-read-cli chats
whatsapp-read-cli chats --limit 200
whatsapp-read-cli chats --limit 50 --offset 50
whatsapp-read-cli chats --unread
```

Use `--limit 0` to print all rows. Use `--offset` to skip rows, for example `--limit 50 --offset 50` for the second page.

### `messages`

List messages for one chat. `--chat` is required.

```sh
whatsapp-read-cli messages --chat 123
whatsapp-read-cli messages --chat "Alice"
whatsapp-read-cli messages --chat "49123456789" --limit 500
whatsapp-read-cli messages --chat 123 --limit 50 --offset 50
whatsapp-read-cli messages --chat 123 --unread
```

`--chat` accepts:

| selector | behavior |
| --- | --- |
| chat id | exact id from `chats` |
| name/JID substring | allowed only when it matches exactly one chat |

If a substring matches multiple chats, the command prints candidate chat ids and fails.

`--unread` uses WhatsApp's unread count for that chat and returns the newest incoming unread tail. Use `chats --unread` first to find chats marked unread.

### `contacts`

List contacts.

```sh
whatsapp-read-cli contacts
whatsapp-read-cli contacts --limit 100 --offset 100
whatsapp-read-cli contacts --limit 0
```

### `calls`

List call history.

```sh
whatsapp-read-cli calls
whatsapp-read-cli calls --limit 100 --offset 100
whatsapp-read-cli calls --limit 0
```

### `search`

Search message text globally, or within one chat.

```sh
whatsapp-read-cli search "invoice"
whatsapp-read-cli search "invoice" --chat 123
whatsapp-read-cli search "invoice" --limit 50 --offset 50
whatsapp-read-cli search "invoice" --format json
```

Search is global by default because it has an explicit search term. Use `--chat` to scope it.

## Debug Commands

Debug commands are for schema inspection and troubleshooting. Normal usage should not need them.

### `debug dbs`

List SQLite databases found under the WhatsApp root.

```sh
whatsapp-read-cli debug dbs
```

### `debug tables`

List tables in a database.

```sh
whatsapp-read-cli debug tables --db chat
whatsapp-read-cli debug tables --db contacts
```

Built-in database names:

| name | path |
| --- | --- |
| `chat` | `ChatStorage.sqlite` |
| `contacts` | `ContactsV2.sqlite` |
| `calls` | `CallHistory.sqlite` |
| `local-storage` | `LocalStorageDB/LocalStorageDatabase.sqlite` |
| `ext-chat` | `ExtChatDB/ExtChatDatabase.sqlite` |
| `backed-up` | `BackedUpStorageDB/BackedUpStorageDatabase.sqlite` |

You can also pass a relative path under the root or an absolute SQLite database path.

### `debug schema`

Print SQLite schema SQL for a database, or for one table.

```sh
whatsapp-read-cli debug schema --db chat
whatsapp-read-cli debug schema --db chat ZWAMESSAGE
```

### `debug query`

Run a single read-only `SELECT` query.

```sh
whatsapp-read-cli debug query --db chat "SELECT Z_PK, ZTEXT FROM ZWAMESSAGE LIMIT 10"
```

Statements containing another semicolon or not starting with `SELECT` are rejected.

## Read-Only Guarantees

- no WhatsApp network calls
- SQLite opened read-only
- SQLite opened with `immutable=1`
- `PRAGMA query_only = ON`
- raw SQL exists only under `debug query`
- raw SQL only accepts one `SELECT`
- debug table/schema identifiers reject unsafe characters
