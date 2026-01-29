# atomCAD CLI REPL Mode Design

This document specifies the REPL (Read-Eval-Print Loop) mode for `atomcad-cli`.

## Overview

The CLI supports three usage patterns:
1. **Single commands** - `atomcad-cli query`, `atomcad-cli edit --code="..."`
2. **Standalone multi-line edit** - `atomcad-cli edit` (reads from stdin)
3. **REPL mode** - `atomcad-cli` (interactive session)

## Invocation Summary

| Invocation | Behavior |
|------------|----------|
| `atomcad-cli` | Enter REPL mode |
| `atomcad-cli query` | Query network, print, exit |
| `atomcad-cli edit --code="..."` | Send edit, exit |
| `atomcad-cli edit --code="..." --replace` | Send replace, exit |
| `atomcad-cli edit` | Read stdin until terminator, send edit, exit |
| `atomcad-cli edit --replace` | Read stdin until terminator, send replace, exit |

---

## REPL Mode

### Entry

```
$ atomcad-cli
atomCAD REPL (localhost:19847)
Type 'help' for commands.

>
```

If atomCAD is not running, print error and exit (same as current behavior).

### Modes

The REPL has two modes:

1. **Command mode** (default) - Input is interpreted as REPL commands
2. **Edit mode** - Input is accumulated as text format content

### Command Mode

**Prompt:** `> `

**Available commands:**

| Command | Aliases | Description |
|---------|---------|-------------|
| `query` | `q` | Print current network in text format |
| `edit` | | Enter edit mode (incremental) |
| `edit --replace` | `replace`, `r` | Enter edit mode (replace) |
| `help` | `?` | Show available commands |
| `quit` | `exit` | Exit REPL |

**Unknown commands:** Print error message, stay in command mode.

```
> foo
Unknown command: foo
Type 'help' for available commands.
>
```

### Edit Mode

**Entry:** Type `edit`, `edit --replace`, `replace`, or `r` in command mode.

**Prompt:** `edit> `

**Behavior:**
- All input is accumulated as text format content
- No commands are recognized (everything is content)
- Accumulation continues until terminator

**Termination:**
- **Empty line** - Send accumulated content
- **Single `.`** on its own line - Send accumulated content
- **Ctrl+C** - Cancel, discard content, return to command mode

**After termination:**
- Send accumulated content to atomCAD
- Print result (success message or error)
- Return to command mode

### Example Session

```
$ atomcad-cli
atomCAD REPL (localhost:19847)
Type 'help' for commands.

> query
# Network is empty

> edit
edit> sphere1 = sphere { radius: 5 }
edit> cuboid1 = cuboid { extent: (10, 10, 10) }
edit>
OK: Created sphere1, cuboid1

> query
sphere1 = sphere { center: (0, 0, 0), radius: 5 }
cuboid1 = cuboid { min_corner: (0, 0, 0), extent: (10, 10, 10) }

> edit
edit> union1 = union { shapes: [sphere1, cuboid1], visible: true }
edit> output union1
edit> .
OK: Created union1, set output

> replace
edit> sphere1 = sphere { radius: 10 }
edit> output sphere1
edit>
OK: Replaced network (1 node)

> q
sphere1 = sphere { center: (0, 0, 0), radius: 10, visible: true }

output sphere1

> quit
$
```

### Help Output

```
> help
atomCAD REPL Commands:

  query, q          Show current node network
  edit              Enter edit mode (incremental)
  edit --replace    Enter edit mode (replace entire network)
  replace, r        Same as 'edit --replace'
  help, ?           Show this help
  quit, exit        Exit REPL

Edit mode:
  Type text format commands, then:
  - Empty line to send
  - '.' on its own line to send
  - Ctrl+C to cancel

>
```

---

## Standalone Multi-line Edit

When `atomcad-cli edit` is invoked without `--code`, read from stdin.

### Behavior

1. Print prompt if stdin is a TTY: `Enter text format (empty line or '.' to send):`
2. Read lines until terminator:
   - Empty line
   - Single `.` on its own line
3. Send accumulated content to atomCAD
4. Print result
5. Exit

### Examples

**Interactive:**
```
$ atomcad-cli edit
Enter text format (empty line or '.' to send):
sphere1 = sphere { radius: 5 }
cuboid1 = cuboid { extent: (10, 10, 10) }

OK: Created sphere1, cuboid1
$
```

**With replace flag:**
```
$ atomcad-cli edit --replace
Enter text format (empty line or '.' to send):
sphere1 = sphere { radius: 20 }
.
OK: Replaced network (1 node)
$
```

**Heredoc (for scripts):**
```bash
atomcad-cli edit << 'EOF'
sphere1 = sphere { radius: 5 }
cuboid1 = cuboid { extent: (10, 10, 10) }
EOF
```
Note: Heredoc provides its own termination, so the empty line/`.` terminator is not required when input is not a TTY. However, for simplicity, we still require the terminator. Scripts should include an empty line or `.` at the end.

**Alternative for scripts - use --code:**
```bash
atomcad-cli edit --code="sphere1 = sphere { radius: 5 }"
```

**Pipe from file:**
```bash
cat design.txt | atomcad-cli edit
```
The file should end with an empty line or `.`.

---

## Error Handling

### atomCAD Not Running

```
$ atomcad-cli
Error: atomCAD is not running on localhost:19847
Please start atomCAD and try again.
$
```

### Edit Mode Errors

```
> edit
edit> invalid syntax here
edit>
Error: Parse error at line 1: expected '=' after identifier
>
```

The REPL returns to command mode after an error; accumulated content is discarded.

### Connection Lost During Session

```
> query
Error: Connection to atomCAD lost.
>
```

User can retry or quit. REPL does not auto-exit on connection errors.

---

## Implementation Notes

### State Machine

```
                    ┌─────────────┐
     startup ──────►│ COMMAND     │◄─────────┐
                    │ MODE        │          │
                    └──────┬──────┘          │
                           │                 │
            edit/replace   │                 │ send success
                           ▼                 │ send error
                    ┌─────────────┐          │ Ctrl+C
                    │ EDIT        ├──────────┘
                    │ MODE        │
                    └─────────────┘
```

### Input Reading

- Use `stdin.readLineSync()` or equivalent
- Detect TTY for prompt display decisions
- Handle Ctrl+C (SIGINT) gracefully in edit mode

### Accumulated Content

- Store as `List<String>` or `StringBuffer`
- Join with newlines when sending
- Clear after send or cancel

---

## Future Considerations

- **History:** Arrow keys to recall previous commands/edits (requires readline-like library)
- **Tab completion:** Complete node names, node types
- **Syntax highlighting:** Color output for text format
- **Watch mode:** Auto-refresh query output when network changes

---

## Changelog

- 2025-01-16: Initial design
