# GhostWire User Guide

A comprehensive guide to using the GhostWire encrypted chat client.

> **New here?** See the [Quick Start](../../QUICKSTART.md) for a 5-minute local setup, then come back here for the full reference.

---

## Table of Contents

1. [Getting Started](#1-getting-started)
2. [Interface Overview](#2-interface-overview)
3. [Keyboard Controls](#3-keyboard-controls)
4. [Slash Commands](#4-slash-commands)
5. [Messaging Features](#5-messaging-features)
6. [Channels & Direct Messages](#6-channels--direct-messages)
7. [Identity & Verification](#7-identity--verification)
8. [Configuration](#8-configuration)
9. [Focus Mode](#9-focus-mode)
10. [Fonts & Terminal Recommendations](#10-fonts--terminal-recommendations)
11. [Troubleshooting](#11-troubleshooting)

---

## 1. Getting Started

### Installation

| Platform             | Command                                                                                           |
| -------------------- | ------------------------------------------------------------------------------------------------- |
| macOS (Homebrew)     | `brew install jcyrus/tap/ghostwire`                                                               |
| Windows (Scoop)      | `scoop bucket add ghostwire https://github.com/jcyrus/scoop-ghostwire && scoop install ghostwire` |
| Linux / macOS (curl) | `curl -fsSL https://ghost.jcyrus.com/install.sh \| sh`                                            |
| Windows (PowerShell) | `irm https://ghost.jcyrus.com/install.ps1 \| iex`                                                 |

For Windows-specific details (PATH setup, troubleshooting), see [WINDOWS.md](WINDOWS.md).

### Launching

```bash
# Connect with a random username
ghostwire

# Choose your username
ghostwire alice

# Connect to a custom server
ghostwire alice ws://localhost:8080/ws
```

**Arguments:**

| Argument     | Required | Default                     | Description               |
| ------------ | -------- | --------------------------- | ------------------------- |
| `USERNAME`   | No       | `ghost_XXXXXXXX` (random)   | Your display name         |
| `SERVER_URL` | No       | `wss://ghost.jcyrus.com/ws` | WebSocket server endpoint |

### First Launch

When you connect, you'll see:

1. A welcome message: _"Welcome to GhostWire, \<username\>!"_
2. A _"Connected"_ system message
3. The three-panel interface ready for chatting

You start in **Normal mode** — press `i` or `Enter` to start typing.

<!-- screenshot: first-launch-overview -->

---

## 2. Interface Overview

GhostWire uses a three-panel terminal interface:

```
┌─ Channels ──┬─── Chat ──────────────────────┬─ Activity ──┐
│              │                               │ ⏱ 0h2m14s   │
│ # global     │ [09:18:10] ℹ Welcome to ...   │ ↑ 1.2KB     │
│              │ [09:18:10] ℹ Connected         │ 📨 3/5      │
│              │ [09:18:15] alice: Hello!        │             │
│              │                               ├─ Reference ─┤
│              │                               │ i/Enter Edit │
│──────────────│                               │ q/Esc   Quit │
│ Users (2)    │                               │ j/k ↑↓ Scrl │
│ ● alice      │                               │ ...          │
│ ● bob        │                               │              │
│              │                               │── Commands ──│
│              │                               │ /me <action> │
│              │                               │ /react <emj> │
│              ├───────────────────────────────│              │
│              │ [NORMAL | F10: Focus]          │              │
└──────────────┴───────────────────────────────┴──────────────┘
```

### Left Panel — Channels & Users

- **Channels** — lists all joined channels; the active channel is highlighted green; channels with unread messages appear yellow with a count
- **Users** — shows online (●), idle (◐), and offline (○) users with last-seen time

### Middle Panel — Chat & Input

- **Title bar** — shows the channel name, connection status (● CONNECTED / ○ DISCONNECTED), scroll position, and encrypted message count (🔒)
- **Messages** — chat messages with timestamps; system messages use colored icons (ℹ info, ⚠ warning, ✖ error)
- **Input box** — shows the current mode (NORMAL / EDIT / COMMAND) and accepts text input

### Right Panel — Activity & Reference

- **Activity** — compact summary of uptime, latency, bytes transferred, and message counts
- **Reference** — quick reference for keyboard shortcuts, slash commands, and markdown syntax

> **Tip:** Press `F10` to hide the right panel and expand the chat area. See [Focus Mode](#9-focus-mode).

---

## 3. Keyboard Controls

### Normal Mode

You start in Normal mode. It's for navigation and reading.

| Key           | Action                                                    |
| ------------- | --------------------------------------------------------- |
| `i` / `Enter` | Enter Edit mode (start typing)                            |
| `q` / `Esc`   | Quit GhostWire                                            |
| `j` / `↓`     | Scroll chat down (newer)                                  |
| `k` / `↑`     | Scroll chat up (older)                                    |
| `PageDown`    | Scroll down ~20 lines                                     |
| `PageUp`      | Scroll up ~20 lines                                       |
| `G`           | Jump to bottom (latest messages)                          |
| `g`           | Jump to top (oldest messages)                             |
| `h` / `←`     | Select previous channel                                   |
| `l` / `→`     | Select next channel                                       |
| `Tab`         | Activate the selected channel                             |
| `#`           | Jump to the #global channel                               |
| `J`           | Select next user (in user list)                           |
| `K`           | Select previous user                                      |
| `d`           | Open DM with selected user                                |
| `r`           | Quick react (opens command mode with `/react ` prefilled) |
| `F10`         | Toggle Focus mode (hide/show right panel)                 |

### Edit Mode

Press `i` or `Enter` from Normal mode to enter Edit mode.

| Key           | Action                         |
| ------------- | ------------------------------ |
| `Esc`         | Exit to Normal mode            |
| `Enter`       | Send message                   |
| `Backspace`   | Delete character before cursor |
| `←` / `→`     | Move cursor left/right         |
| Any character | Type into the input            |

### Command Mode

Type `/` as the first character in Edit mode to switch to Command mode. The input border turns cyan, and inline command hints appear.

| Key         | Action                                               |
| ----------- | ---------------------------------------------------- |
| `Esc`       | Exit to Normal mode                                  |
| `Enter`     | Execute the command                                  |
| `Backspace` | Delete; if input becomes empty, returns to Edit mode |

---

## 4. Slash Commands

Type these in Edit mode (they auto-switch to Command mode when `/` is the first character).

### `/me <action>`

Send an action message, displayed in italic magenta:

```
/me waves hello
```

Renders as: _\* alice waves hello_

### `/react <emoji>`

React to the most recent message with an emoji:

```
/react 👍
```

To react to a specific (older) message, include its ID:

```
/react abc123 🎉
```

Reactions appear as a strip below the message: `👍 2  🎉 1`

> **Tip:** Press `r` in Normal mode to quickly open `/react `.

### `/verify <username>`

Display the safety number for a peer, used for out-of-band identity verification:

```
/verify bob
```

### `/confirm <username>`

Mark a peer as verified after comparing safety numbers:

```
/confirm bob
```

A ✓ badge appears next to verified users in the user list.

### `/expire <seconds> <message>`

Send a self-destructing message (1–86400 seconds):

```
/expire 30 This message will disappear in 30 seconds
```

### `/groupkey <group> <user1,user2,...>`

Distribute a group encryption key to specified users:

```
/groupkey project-team alice,bob,carol
```

---

## 5. Messaging Features

### Markdown Formatting

GhostWire supports lightweight markdown in chat messages:

| Syntax                             | Renders as                                       |
| ---------------------------------- | ------------------------------------------------ |
| `**bold text**`                    | **bold text**                                    |
| `*italic text*` or `_italic text_` | _italic text_                                    |
| `` `inline code` ``                | `inline code` (green on dark background)         |
| `> quoted text`                    | │ _quoted text_ (gray, italic, with left border) |
| ` ``` ` (fenced block)             | Code block (green on dark background)            |

Fenced code blocks span multiple lines:

````
```
fn main() {
    println!("Hello, GhostWire!");
}
```
````

### Message Reactions

React to messages using `/react <emoji>` or the quick-react keybind `r`. Reactions from all users are aggregated and shown below the message:

```
[09:20:15] alice: Great news everyone!
                  👍 3  🎉 2  ❤️ 1
```

### Action Messages

Use `/me` to send third-person action messages:

```
/me joined the meeting
```

Displays as: _\* alice joined the meeting_ (italic, magenta)

### Encrypted Messages

Messages encrypted with end-to-end encryption show a 🔒 lock icon before the sender name. The chat title bar also shows the total encrypted message count.

### Self-Destructing Messages

Use `/expire <seconds> <message>` to send messages that auto-delete after the specified duration.

---

## 6. Channels & Direct Messages

### Channels

All users start in the **#global** channel. Channels appear in the left panel.

**Navigation:**

| Action                           | Keys                   |
| -------------------------------- | ---------------------- |
| Select previous/next channel     | `h` / `l` or `←` / `→` |
| Activate the highlighted channel | `Tab`                  |
| Jump to #global                  | `#`                    |

Unread channels appear **yellow** with an unread count (e.g., `# general (3)`).

### Direct Messages

To start a DM:

1. Use `J` / `K` to select a user in the user list
2. Press `d` to open (or create) a DM channel
3. The DM channel appears in your channel list

DM channels are displayed as `@ username` in the channel list.

For a detailed guide on the channel system, see [CHANNELS.md](../../CHANNELS.md).

---

## 7. Identity & Verification

### Automatic Key Exchange

When you connect, GhostWire automatically performs an X25519 key exchange with other online users. This establishes shared secrets for encrypting messages — no manual setup needed.

### Username Colors

Each user's name is displayed in a unique color derived from their public key. This provides a visual fingerprint — if a user's color changes unexpectedly, their identity may have changed.

### Verifying Peers

For high-security conversations, verify peers out-of-band:

1. Run `/verify <username>` to display their safety number
2. Compare the safety number through a separate channel (phone, in person, etc.)
3. If it matches, run `/confirm <username>` to mark them as verified
4. A ✓ badge appears next to their name

### Trust Model

GhostWire uses a **Trust On First Use (TOFU)** model by default. The first time you see a peer's key, it's accepted. Subsequent key changes trigger warnings.

For the full cryptographic design and threat model, see [SECURITY.md](SECURITY.md).

---

## 8. Configuration

GhostWire stores its configuration at:

```
~/.config/ghostwire/config.toml
```

If the file doesn't exist, GhostWire creates one with defaults on first launch.

### Full Configuration Reference

```toml
# Server to connect to (overridden by CLI argument)
default_server_url = "wss://ghost.jcyrus.com/ws"

# Send typing indicators to other users
send_typing_indicators = true

# How many days to keep log files
log_retention_days = 7

# Timestamp display format: "24h", "12h", "DateTime", or "Relative"
timestamp_format = "24h"

# Auto-reconnect behavior
[auto_reconnect]
enabled = true
max_attempts = 10          # 0 = unlimited retries
initial_backoff_secs = 1   # First retry delay
max_backoff_secs = 16      # Maximum retry delay (exponential backoff)
```

### Option Details

| Option                                | Type    | Default                     | Description                                                |
| ------------------------------------- | ------- | --------------------------- | ---------------------------------------------------------- |
| `default_server_url`                  | String  | `wss://ghost.jcyrus.com/ws` | WebSocket URL for the relay server                         |
| `send_typing_indicators`              | Boolean | `true`                      | Whether to broadcast typing status                         |
| `log_retention_days`                  | Integer | `7`                         | Days to keep rotated log files                             |
| `timestamp_format`                    | String  | `24h`                       | Timestamp display: `24h`, `12h`, `DateTime`, or `Relative` |
| `auto_reconnect.enabled`              | Boolean | `true`                      | Enable automatic reconnection                              |
| `auto_reconnect.max_attempts`         | Integer | `10`                        | Max reconnect attempts (0 = unlimited)                     |
| `auto_reconnect.initial_backoff_secs` | Integer | `1`                         | Initial delay between retries (seconds)                    |
| `auto_reconnect.max_backoff_secs`     | Integer | `16`                        | Maximum delay between retries (seconds)                    |

### Timestamp Formats

| Format     | Example               |
| ---------- | --------------------- |
| `24h`      | `14:30:05`            |
| `12h`      | `2:30:05 PM`          |
| `DateTime` | `2026-03-08 14:30:05` |
| `Relative` | `2m ago`              |

---

## 9. Focus Mode

Press `F10` to toggle **Focus Mode**, which hides the right sidebar (Activity + Reference panel) and expands the chat area to 80% of the terminal width.

- When the sidebar is visible, the mode indicator shows `F10: Focus`
- When the sidebar is hidden, it shows `F10: Telemetry`

This is useful for maximizing chat space on smaller terminals.

---

## 10. Fonts & Terminal Recommendations

GhostWire uses emoji throughout the UI — reactions (`👍 🎉 ❤️`), status icons, telemetry indicators, and more. To get proper rendering, your terminal and font need full Unicode and emoji support.

### Recommended Fonts

Use a font with built-in emoji glyphs or a [Nerd Font](https://www.nerdfonts.com/) patched variant. These render emoji at the correct width and avoid tofu (□) or broken spacing.

| Font                          | Notes                                                            |
| ----------------------------- | ---------------------------------------------------------------- |
| **JetBrains Mono NL**         | Excellent emoji coverage; popular with developers                |
| **Fira Code** (Nerd Font)     | Ligatures + full Unicode; use the `FiraCode Nerd Font` variant   |
| **Cascadia Code** (Nerd Font) | Ships with Windows Terminal; Nerd Font variant adds extra glyphs |
| **Hack Nerd Font**            | Clean monospace with strong Unicode coverage                     |
| **MesloLGS NF**               | Recommended by many terminal setups (e.g., Powerlevel10k)        |

> **Tip:** If emoji appear as boxes (□) or question marks (?), switch to a Nerd Font variant of your preferred font. Most can be installed from https://www.nerdfonts.com/font-downloads.

### Recommended Terminals

Not all terminal emulators render emoji equally well. The following are tested and known to work with GhostWire:

| Platform           | Terminal                                    | Notes                                                                                   |
| ------------------ | ------------------------------------------- | --------------------------------------------------------------------------------------- |
| **macOS**          | [iTerm2](https://iterm2.com/)               | Best-in-class emoji rendering; set font to a Nerd Font in Preferences → Profiles → Text |
| **macOS**          | [Ghostty](https://ghostty.org/)             | Fast GPU-accelerated terminal with native emoji support                                 |
| **macOS**          | [Kitty](https://sw.kovidgoyal.net/kitty/)   | Renders emoji natively using system fonts; very fast                                    |
| **macOS**          | Terminal.app                                | Works with recent macOS versions; emoji support is adequate                             |
| **Linux**          | [Kitty](https://sw.kovidgoyal.net/kitty/)   | Preferred on Linux; renders emoji without extra configuration                           |
| **Linux**          | [Alacritty](https://alacritty.org/)         | Fast GPU terminal; pair with a Nerd Font for emoji                                      |
| **Linux**          | [WezTerm](https://wezfurlong.org/wezterm/)  | Built-in font fallback handles emoji well out of the box                                |
| **Linux**          | [foot](https://codeberg.org/dnkl/foot)      | Lightweight Wayland terminal with good Unicode support                                  |
| **Windows**        | [Windows Terminal](https://aka.ms/terminal) | Ships with Cascadia Code; best option on Windows                                        |
| **Windows**        | [WezTerm](https://wezfurlong.org/wezterm/)  | Cross-platform; works well on Windows with Nerd Fonts                                   |
| **Cross-platform** | [Warp](https://www.warp.dev/)               | Modern terminal with native emoji rendering                                             |

### Quick Setup Checklist

1. **Install a Nerd Font** — download from [nerdfonts.com](https://www.nerdfonts.com/font-downloads) and install system-wide
2. **Set it in your terminal** — change the font in your terminal's preferences to the Nerd Font variant (e.g., `JetBrainsMono Nerd Font`)
3. **Verify** — run `echo "👍 🎉 ❤️ 🔒 ℹ ⚠ ✖"` in your terminal; all characters should render as distinct glyphs without overlap
4. **Launch GhostWire** — reactions and status icons should now display correctly

> **Note:** If you use `tmux` or `screen`, make sure your multiplexer also supports UTF-8. For tmux, add `set -g default-terminal "tmux-256color"` and `set -gq allow-passthrough on` to your `~/.tmux.conf`.

---

## 11. Troubleshooting

### Connection Issues

**"○ DISCONNECTED" in the title bar**

GhostWire will auto-reconnect using exponential backoff (configurable — see [Configuration](#8-configuration)). Common causes:

- Server is down or restarting
- Network connectivity lost
- Firewall blocking WebSocket connections (port 443 for `wss://`)
- Incorrect server URL

**Connection refused on localhost**

Make sure the local server is running:

```bash
cargo run --bin ghostwire-local
```

Then connect with:

```bash
ghostwire alice ws://localhost:8080/ws
```

### Log Files

GhostWire writes logs to:

```
~/.config/ghostwire/logs/
```

Set the `RUST_LOG` environment variable for verbose output:

```bash
RUST_LOG=debug ghostwire alice
```

### Security Audit Log

Security-relevant events (key exchanges, verifications, encryption errors) are logged to:

```
~/.config/ghostwire/security_audit.log
```

### Common Issues

| Problem                        | Solution                                                                                        |
| ------------------------------ | ----------------------------------------------------------------------------------------------- |
| `ghostwire: command not found` | Ensure the install directory is in your PATH. See [WINDOWS.md](WINDOWS.md) for Windows.         |
| Messages not encrypted (no 🔒) | Key exchange hasn't completed yet with that peer. Wait for them to come online.                 |
| Username shows wrong color     | The peer's public key changed (reconnection or new client). Use `/verify` to re-check identity. |
| Can't see the right panel      | Press `F10` to toggle it back on.                                                               |
| Input box not responding       | You may be in Normal mode. Press `i` or `Enter` to enter Edit mode.                             |

---

## Further Reading

- [Quick Start](../../QUICKSTART.md) — 5-minute local setup
- [Channels & DMs](../../CHANNELS.md) — Detailed channel system guide
- [Security Model](SECURITY.md) — Cryptographic design and threat analysis
- [Changelog](../../CHANGELOG.md) — Version history and release notes
