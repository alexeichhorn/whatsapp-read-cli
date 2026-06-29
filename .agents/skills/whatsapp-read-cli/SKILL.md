---
name: whatsapp-read-cli
description: "Use when an agent needs to answer questions by reading a user's local macOS WhatsApp data through the whatsapp-read-cli tool. Trigger for tasks like finding chats, unread chats, messages in a chat, contacts, calls, local WhatsApp search, or safely using the CLI without contacting WhatsApp servers."
---

# WhatsApp Read CLI

Use `whatsapp-read-cli` as a read-only local data source for WhatsApp on macOS. Treat it like a user-data tool: run the smallest command that answers the question, summarize results, and avoid dumping private content unless the user asks for raw output.

## Safety Rules

- Never contact WhatsApp servers.
- Never mark messages read, send messages, delete data, edit data, sync data, or imply that the CLI can modify WhatsApp state.
- Prefer normal commands over `debug`.
- Use `debug query` only when normal commands cannot answer the question.
- If using `debug query`, run only a single `SELECT`; never attempt write SQL.
- Do not paste large private message dumps into the final answer. Summarize and cite counts/snippets only as needed.

## Setup Check

If the binary may not be installed, first try:

```sh
whatsapp-read-cli info
```

If missing and the repo is available, install from the repo root:

```sh
cargo install --path . --locked
```

If auto-detection reads the wrong WhatsApp instance, use:

```sh
whatsapp-read-cli --root "/path/to/group.net.whatsapp.WhatsApp.shared" info
```

## Output Formats

Default output is human-readable. For parsing, use:

```sh
whatsapp-read-cli --format json ...
whatsapp-read-cli --format tsv ...
```

Prefer `json` when you need structured post-processing. Prefer small `--limit` values while exploring.

## Common Tasks

### Check Available Data

```sh
whatsapp-read-cli info
```

Use this to confirm the detected root, read-only mode, and counts for chats, unread chats, messages, contacts, and calls.

### Find Chats

```sh
whatsapp-read-cli chats --limit 50
whatsapp-read-cli chats --unread --limit 50
whatsapp-read-cli chats --limit 50 --offset 50
```

Use `--limit 0` only when the user explicitly needs all rows.

### Read Messages

`messages` always requires `--chat`.

```sh
whatsapp-read-cli messages --chat 123 --limit 50
whatsapp-read-cli messages --chat "Alice" --limit 50
whatsapp-read-cli messages --chat 123 --unread
```

`--chat` accepts a chat id from `chats`, or a name/JID substring only when it matches exactly one chat. If multiple chats match, use the printed candidates and rerun with the exact chat id.

### Search Messages

```sh
whatsapp-read-cli search "invoice" --limit 50
whatsapp-read-cli search "invoice" --chat 123 --limit 50
```

Search is global by default because the user gave an explicit search term. Add `--chat` when the user asks about a specific conversation.

### Contacts And Calls

```sh
whatsapp-read-cli contacts --limit 100
whatsapp-read-cli calls --limit 100
```

Use `--offset` for pagination.

## Debug Escape Hatch

Only use debug commands for troubleshooting or schema inspection:

```sh
whatsapp-read-cli debug dbs
whatsapp-read-cli debug tables --db chat
whatsapp-read-cli debug schema --db chat ZWAMESSAGE
whatsapp-read-cli debug query --db chat "SELECT Z_PK, ZTEXT FROM ZWAMESSAGE LIMIT 10"
```

Normal user questions should usually be answered with `info`, `chats`, `messages`, `contacts`, `calls`, or `search`.
