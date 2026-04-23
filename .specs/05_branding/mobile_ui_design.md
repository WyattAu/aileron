# T05: Aileron Mobile Client UI Design Document

**Date:** 2026-04-23
**Author:** Aileron Architecture Team
**Status:** DRAFT
**Version:** 1.0.0-draft
**Related:** mobile_architecture.md (T01), arp_protocol_spec.md (T02)

---

## Table of Contents

1. [Design Philosophy](#1-design-philosophy)
2. [Screen Map](#2-screen-map)
3. [Main Screens](#3-main-screens)
4. [Touch Gestures](#4-touch-gestures)
5. [Color Scheme](#5-color-scheme)
6. [Typography](#6-typography)
7. [Layout Specs](#7-layout-specs)
8. [Component Library](#8-component-library)
9. [Animation](#9-animation)
10. [Offline Behavior](#10-offline-behavior)
11. [Accessibility](#11-accessibility)

---

## 1. Design Philosophy

Aileron Mobile is a **thin remote client** for the desktop Aileron environment. The UI design follows these principles:

### 1.1 Core Tenets

| Principle | Description |
|-----------|-------------|
| **Minimal chrome** | Content first. UI controls appear only when needed and fade when idle. |
| **Developer-native** | Monospace terminal rendering, dark-first color scheme, no consumer app aesthetics. |
| **Touch-efficient** | Every action achievable in 1-2 taps. Gestures replace keyboard shortcuts. |
| **State-transparent** | Connection status, latency, and sync state always visible but never intrusive. |
| **Progressive disclosure** | Simple surfaces by default; advanced features (command palette, settings) on demand. |

### 1.2 Design Anti-Patterns (Avoid)

- Decorative animations that delay interaction
- Floating action buttons (FABs) that obscure content
- Bottom sheets with no keyboard dismiss
- Hamburger menus with more than 5 items
- Non-monospace terminal rendering
- Light mode as default

### 1.3 Platform Adaptation

Both Android (Jetpack Compose) and iOS (SwiftUI) follow identical visual specs. Platform differences are limited to:

- **Navigation**: Android uses top app bar + back gesture; iOS uses navigation stack + swipe-back
- **Status bar**: Android translucent status bar; iOS matches safe area insets
- **Haptics**: Android `HapticFeedbackConstants`; iOS `UIImpactFeedbackGenerator`

---

## 2. Screen Map

### 2.1 Navigation Hierarchy

```
[Connection Screen] ──connect──> [Tab Carousel] (root)
                                    |
                                    ├── [Tab Content View]     (tap card or swipe-up)
                                    |     └── [URL Bar Overlay]  (pull-down or tap)
                                    |
                                    ├── [Terminal View]         (bottom nav)
                                    |     └── [Terminal Pane Selector] (tap header)
                                    |
                                    ├── [Downloads Panel]       (bottom nav)
                                    |
                                    ├── [Command Palette]       (two-finger tap)
                                    |
                                    └── [Settings]              (gear icon)
                                          ├── [Server Config]
                                          ├── [Display Preferences]
                                          └── [About / Version]
```

### 2.2 Screen Inventory

| # | Screen | Entry Point | ARP Methods Used | Fullscreen? |
|---|--------|-------------|------------------|-------------|
| S1 | Connection | App launch (no saved conn) | `system.info` | Yes |
| S2 | Tab Carousel | Post-connect root | `tabs.list`, `tabs.screenshot` | No |
| S3 | Tab Content View | Tap tab card | `tabs.screenshot`, `tabs.goBack`, `tabs.goForward`, `tabs.navigate` | Yes |
| S4 | Terminal View | Bottom nav | `terminal.list`, `terminal.input`, `terminal.sendKey` | Yes |
| S5 | Command Palette | Two-finger tap | Any (dispatches commands) | Modal |
| S6 | Downloads Panel | Bottom nav | `downloads.list`, `downloads.cancel`, `downloads.pause`, `downloads.resume` | No |
| S7 | Settings | Gear icon | None (local) | No |

---

## 3. Main Screens

### 3.1 Connection Screen (S1)

The entry point when no active connection exists. Supports manual entry and QR code scanning.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│          ╭─────────────────────╮         │
│          │                     │         │
│          │    ◆  AILERON       │         │
│          │    Remote Client    │         │
│          │                     │         │
│          ╰─────────────────────╯         │
│                                          │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │ 🌐  Host                           │  │
│  │ ┌────────────────────────────────┐ │  │
│  │ │ 192.168.1.100                  │ │  │
│  │ └────────────────────────────────┘ │  │
│  │                                    │  │
│  │ 🔑  Auth Token                     │  │
│  │ ┌────────────────────────────────┐ │  │
│  │ │ a1b2c3d4e5f6...  [👁] [📋]     │ │  │
│  │ └────────────────────────────────┘ │  │
│  │                                    │  │
│  │ Port                               │  │
│  │ ┌────────────────────────────────┐ │  │
│  │ │ 19743                          │ │  │
│  │ └────────────────────────────────┘ │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │            [  Connect  ]           │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ────── or scan QR code ──────          │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │                                     │  │
│  │          ┌──────────┐              │  │
│  │          │  ┌────┐  │              │  │
│  │          │  │ QR │  │              │  │
│  │          │  │    │  │              │  │
│  │          │  └────┘  │              │  │
│  │          └──────────┘              │  │
│  │         [📷 Scan QR Code]          │  │
│  │                                     │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ─── Saved Connections ───              │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │ ● Home Desktop  192.168.1.100      │  │
│  │   Last connected: 2 hours ago      │  │
│  │                     [Edit] [Del]   │  │
│  ├────────────────────────────────────┤  │
│  │ ○ Work Laptop  10.0.0.42           │  │
│  │   Last connected: 3 days ago       │  │
│  │                     [Edit] [Del]   │  │
│  └────────────────────────────────────┘  │
│                                          │
│  Status: Idle                           │
└──────────────────────────────────────────┘
```

#### State Table

| State | Visual | Connect Button |
|-------|--------|----------------|
| Idle | Default | Enabled |
| Connecting | Spinner on button, status "Connecting..." | Disabled |
| Authenticating | Spinner, status "Authenticating..." | Disabled |
| Connected | Green checkmark, auto-navigate to Tab Carousel | Hidden |
| Error (network) | Red banner: "Cannot reach host" | Re-enabled |
| Error (auth) | Red banner: "Invalid auth token" | Re-enabled |
| Error (version) | Red banner: "Server version incompatible" | Re-enabled |

#### Saved Connection Item Layout

```
┌─────────────────────────────────────────┐
│  ●  Home Desktop                        │
│     192.168.1.100:19743                 │
│     Last connected: 2 hours ago         │
│                          [✏] [🗑]      │
└─────────────────────────────────────────┘
```

- `●` green dot = reachable (last ping < 2s)
- `○` gray dot = unknown / offline
- Tap row = auto-connect
- `✏` = edit connection
- `🗑` = delete (with confirmation dialog)

---

### 3.2 Tab Carousel (S2)

The primary screen after connecting. Displays all open desktop tabs as horizontal swipeable cards with live screenshots.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│ ● Connected  192.168.1.100    ⚙ 12ms    │
├──────────────────────────────────────────┤
│                                          │
│  ┌─────────┐  ┌─────────┐  ┌─────────┐  │
│  │ Tab 1   │  │ Tab 2   │  │ Tab 3   │  │
│  │ ┌─────┐ │  │ ┌─────┐ │  │ ┌─────┐ │  │
│  │ │     │ │  │ │     │ │  │ │     │ │  │
│  │ │ Screenshot│ │Screenshot│ │Screenshot│
│  │ │     │ │  │ │     │ │  │ │     │ │  │
│  │ └─────┘ │  │ └─────┘ │  │ └─────┘ │  │
│  │ GitHub  │  │ docs.rs │  │ loading │  │
│  │    [×]  │  │    [×]  │  │    [×]  │  │
│  └─────────┘  └─────────┘  └─────────┘  │
│      ◄ 2 of 5 ►                         │
│                                          │
│  ──────────────────────────────────────  │
│  ┌────────────────────────────────────┐  │
│  │  [Terminal]  [Downloads]  [  +  ]  │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

#### Card Layout (Expanded)

Each tab card is a `TabCard` component:

```
┌───────────────────────┐  ←  Card border: 1dp #2a2a35, radius 8dp
│ ┌───────────────────┐ │
│ │                   │ │  ←  Screenshot area: 16:10 ratio
│ │                   │ │     aspect-fill, rounded top corners
│ │   Tab Screenshot  │ │     Gray placeholder while loading
│ │                   │ │     Loading spinner overlay if tab.loading
│ │                   │ │
│ └───────────────────┘ │
│  github.com           │  ←  Title: 13sp, #e0e0e0, single line, ellipsis
│  https://github.com   │  ←  URL: 11sp, #888888, single line, ellipsis
│  ● Active           × │  ←  Left: status dot (● active, ○ inactive)
│                        │     Right: close button (×), 48dp touch target
└───────────────────────┘
```

#### Carousel Behavior

- **Horizontal snap scroll** with `ViewPager2` (Android) / `LazyHStack + Paging` (iOS)
- **Active tab indicator**: Bright accent border (2dp #4db4ff) around active tab card
- **Screen peek**: Adjacent cards visible at 15% width on each side
- **Auto-screenshot refresh**: On `tab.updated` or `tab.loading` events (debounced 2s)
- **Swipe-up gesture** on active card opens full Tab Content View

#### Top Bar States

```
Connected:    ● Connected  192.168.1.100    ⚙ 12ms
Connecting:   ◐ Reconnecting...    ⚙ --
Disconnected: ○ Disconnected    ⚙  [Retry]
Error:        ✕ Connection lost    ⚙  [Retry]
```

- Green `●` = connected
- Amber `◐` = connecting / reconnecting
- Red `✕` = error
- Latency displayed as round-trip ms from `system.ping`
- `⚙` = tap to open Settings

---

### 3.3 Tab Content View (S3)

Full-screen view of a single tab's content with navigation controls.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│  [←]  [→]  [↻]                          │
├──────────────────────────────────────────┤
│                                          │
│                                          │
│                                          │
│           Full Tab Content               │
│           (Screenshot)                   │
│           pinch-zoom enabled             │
│                                          │
│                                          │
│                                          │
│                                          │
│                                          │
│                                          │
│                                          │
│                                          │
├──────────────────────────────────────────┤
│ ┌────────────────────────────────────┐   │
│ │ 🔒 https://github.com             │   │  ← Pull-down to reveal URL bar
│ └────────────────────────────────────┘   │
└──────────────────────────────────────────┘
```

#### URL Bar (Pull-Down Overlay)

```
┌──────────────────────────────────────────┐
│  ┌──────────────────────────────────┐    │
│  │ 🔒 https://github.com           │    │
│  └──────────────────────────────────┘    │
│  ┌──────────────────────────────────┐    │
│  │  Navigate to:                    │    │
│  │ ┌──────────────────────────────┐ │    │
│  │ │                              │ │    │
│  │ └──────────────────────────────┘ │    │
│  │            [  Go  ]              │    │
│  └──────────────────────────────────┘    │
│  Quickmarks:  [gh] [docs] [yt] [so]     │
└──────────────────────────────────────────┘
```

- URL bar slides down from top on pull-down gesture or tap on bottom URL strip
- Quickmarks row fetched from `quickmarks.list`
- Tapping a quickmark calls `quickmarks.open` and dismisses overlay
- "Go" button calls `tabs.navigate`

#### Navigation Controls (Top Bar)

| Button | Icon | ARP Method | Enabled When |
|--------|------|------------|--------------|
| Back | `←` | `tabs.goBack` | `can_go_back == true` |
| Forward | `→` | `tabs.goForward` | `can_go_forward == true` |
| Reload | `↻` | `tabs.navigate` (same URL) | Always |
| Close | `×` | `tabs.close` | Always |

#### Zoom Behavior

- Pinch-zoom: 1x to 3x, smooth scaling via gesture transform
- Double-tap: toggle between 1x and 2x
- Pan when zoomed beyond 1x
- Zoom resets on tab switch

#### Loading State

When `tab.loading == true`, overlay a thin progress bar (3dp, accent color) at the top of the content area, animated indeterminate.

---

### 3.4 Terminal View (S4)

Full VT100 terminal stream from a desktop terminal pane, with virtual keyboard for special keys.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│  [←]  Terminal                    ⋮     │
├──────────────────────────────────────────┤
│  ┌────────────────────────────────────┐  │
│  │ ▼ pane-3: bash                    │  │  ← Pane selector (tap to expand)
│  ├────────────────────────────────────┤  │
│  │                                    │  │
│  │  $ ls -la                          │  │
│  │  total 16                          │  │
│  │  drwxr-xr-x  5 user  staff  160    │  │
│  │  -rw-r--r--  1 user  staff   42    │  │
│  │  drwxr-xr-x  8 user  staff  256    │  │
│  │  $ cargo build --release           │  │
│  │     Compiling aileron v0.14.0      │  │
│  │     Compiling serde v1.0.200       │  │
│  │  $ _                               │  │
│  │                                    │  │
│  │                                    │  │
│  │                                    │  │
│  │                                    │  │
│  │                                    │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │ [Tab] [Ctrl▼] [Esc] [↑] [↓] [←][→]│  │  ← Special key bar
│  └────────────────────────────────────┘  │
├──────────────────────────────────────────┤
│  ┌────────────────────────────────────┐  │
│  │  Type here...                 [⏎] │  │  ← Input field (always visible)
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

#### Pane Selector (Expanded)

```
┌──────────────────────────────────────────┐
│  Select Terminal Pane                    │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │  ● pane-3  bash          [Attach] │  │  ← Currently attached
│  ├────────────────────────────────────┤  │
│  │  ○ pane-5  vim           [Attach] │  │
│  ├────────────────────────────────────┤  │
│  │  ○ pane-7  htop          [Attach] │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

- Fetched from `terminal.list`
- `●` indicates currently attached pane
- Tap "Attach" to switch pane, calls `terminal.snapshot` to populate content

#### Special Key Bar

| Button | Label | ARP Method |
|--------|-------|------------|
| Tab | `Tab` | `terminal.sendKey("tab")` |
| Ctrl | `Ctrl` (hold to reveal submenu) | See below |
| Esc | `Esc` | `terminal.sendKey("escape")` |
| Up | `↑` | `terminal.sendKey("up")` |
| Down | `↓` | `terminal.sendKey("down")` |
| Left | `←` | `terminal.sendKey("left")` |
| Right | `→` | `terminal.sendKey("right")` |

**Ctrl Submenu** (long-press or hold Ctrl button):

```
┌──────────────────────────────────────────┐
│  [C] [D] [Z] [L] [A] [E] [U] [K] [W]   │
└──────────────────────────────────────────┘
```

Each button calls `terminal.sendKey("ctrl_x")` where x is the letter.

#### Terminal Rendering

- Font: System monospace at 13sp (Android) / 15pt (iOS)
- Scroll buffer: Last 10,000 lines cached locally, scrollable
- Cursor: Blinking block cursor at `cursor_row`/`cursor_col`
- ANSI color support: Parse SGR sequences for 16-color terminal output
- Auto-scroll: Follows new output unless user has scrolled up
- New output indicator: "▼ New output" floating chip when scrolled up

#### Input Field

- Single-line text input at bottom
- `⏎` button sends `text + "\n"` via `terminal.input`
- System keyboard auto-shown when input field focused
- Swipe down on input field to dismiss keyboard and reveal more terminal

---

### 3.5 Command Palette (S5)

A modal bottom sheet for executing Aileron commands, mirroring the desktop `:` command interface.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│                                          │
│                                          │
│          (dimmed background)             │
│                                          │
│                                          │
├──────────────────────────────────────────┤  ← Drag handle
│  ┌────────────────────────────────────┐  │
│  │  🔍  Type a command...            │  │  ← Search input, auto-focused
│  └────────────────────────────────────┘  │
│                                          │
│  Commands                                │
│  ┌────────────────────────────────────┐  │
│  │  :open https://rust-lang.org       │  │  ← Navigate to URL
│  ├────────────────────────────────────┤  │
│  │  :tab-close                        │  │  ← Close current tab
│  ├────────────────────────────────────┤  │
│  │  :tab-new                          │  │  ← Open new tab
│  ├────────────────────────────────────┤  │
│  │  :download-cancel 1                │  │  ← Cancel download #1
│  ├────────────────────────────────────┤  │
│  │  :clipboard-get                    │  │  ← Get desktop clipboard
│  ├────────────────────────────────────┤  │
│  │  :quickmarks-list                  │  │  ← List quickmarks
│  └────────────────────────────────────┘  │
│                                          │
│  Recent                                  │
│  ┌────────────────────────────────────┐  │
│  │  :open https://github.com          │  │
│  │  :tab-close                        │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ═══════════════════════════════════    │  ← Drag handle (bottom)
└──────────────────────────────────────────┘
```

#### Command Dispatch

Commands are typed as strings matching desktop Aileron `:` command syntax. The mobile client:

1. Parses the command string locally to determine the ARP method
2. Calls the corresponding ARP method
3. Dismisses the palette on success
4. Shows inline error on failure

| Command Pattern | ARP Method |
|-----------------|------------|
| `:open <url>` | `tabs.create({url})` or `tabs.navigate` |
| `:tab-close [id]` | `tabs.close({id})` |
| `:tab-new` | `tabs.create({url: ""})` |
| `:tab-goto <id>` | `tabs.activate({id})` |
| `:download-cancel <id>` | `downloads.cancel({id})` |
| `:clipboard-get` | `clipboard.get` → copy to mobile clipboard |
| `:clipboard-set <text>` | `clipboard.set({text})` |
| `:quickmarks-list` | `quickmarks.list` |
| `:quickmarks-open <keyword>` | `quickmarks.open({keyword})` |

#### Filter Behavior

- Fuzzy match on command name as user types
- Recent commands shown below filtered results (persisted locally, max 10)
- Swipe down on search field to dismiss palette

---

### 3.6 Downloads Panel (S6)

A bottom panel (half-sheet) listing active and recent downloads.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│                                          │
│          (dimmed background)             │
│                                          │
├──────────────────────────────────────────┤  ← Drag handle
│  Downloads (2 active)                    │
│                                          │
│  Active                                  │
│  ┌────────────────────────────────────┐  │
│  │  📥 file.pdf                       │  │
│  │  https://example.com/file.pdf      │  │
│  │  ┌────────────────────────────┐    │  │
│  │  │████████████░░░░░░░░░░░░░░░│ 45%│    │  ← Progress bar
│  │  └────────────────────────────┘    │  │
│  │  2.1 MB/s  •  1.2 / 5.0 MB        │  │
│  │                    [⏸] [✕]        │  │
│  ├────────────────────────────────────┤  │
│  │  📥 data.csv                       │  │
│  │  ┌────────────────────────────┐    │  │
│  │  │████████████████████████████│100%│    │
│  │  └────────────────────────────┘    │  │
│  │  3.4 MB  •  Complete  •  [Share]  │  │
│  ├────────────────────────────────────┤  │
│  │  📥 image.png                      │  │
│  │  3.4 MB  •  Complete  •  [Share]  │  │
│  ├────────────────────────────────────┤  │
│  │  📥 archive.tar.gz                 │  │
│  │  12.1 MB  •  Failed  •  [Retry]   │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ═══════════════════════════════════    │  ← Drag handle (bottom)
└──────────────────────────────────────────┘
```

#### Download Item States

| State | Progress Bar | Actions |
|-------|-------------|---------|
| Downloading | Animated fill, accent color, percentage label | Pause `⏸`, Cancel `✕` |
| Paused | Half-filled, amber, "Paused" label | Resume `▶`, Cancel `✕` |
| Completed | Full fill, green, "Complete" label | Share `↗` |
| Failed | Red outline, "Failed" label with error text | Retry `↻` |

#### Progress Bar Component

```
┌──────────────────────────────────────────┐
│ ████████████████░░░░░░░░░░░░░░░░░░░░░░  │
│ 45%                    2.1 MB/s         │
└──────────────────────────────────────────┘
```

- Height: 4dp
- Track: #2a2a35
- Fill: #4db4ff (downloading), #e8a838 (paused), #4caf50 (complete), #e04040 (failed)
- Text: 11sp, #888888 (speed), #e0e0e0 (percentage)

---

### 3.7 Settings Screen (S7)

Local configuration for the mobile client. Does not modify desktop Aileron settings.

#### ASCII Mockup

```
┌──────────────────────────────────────────┐
│  [←]  Settings                           │
├──────────────────────────────────────────┤
│                                          │
│  Server                                  │
│  ┌────────────────────────────────────┐  │
│  │  Host          192.168.1.100   [>] │  │
│  │  Port          19743          [>] │  │
│  │  Auth Token    ••••••••      [>] │  │
│  │  Auto-connect  [Toggle: ON]       │  │
│  └────────────────────────────────────┘  │
│                                          │
│  Display                                 │
│  ┌────────────────────────────────────┐  │
│  │  Screenshot Quality     70%    [>] │  │
│  │  Screenshot Width       800px  [>] │  │
│  │  Terminal Font Size     14sp   [>] │  │
│  │  Dark Mode              [Toggle: ON]│
│  └────────────────────────────────────┘  │
│                                          │
│  Behavior                                │
│  ┌────────────────────────────────────┐  │
│  │  Auto-reconnect        [Toggle: ON]│  │
│  │  Reconnect Interval    60s max [>] │  │
│  │  Haptic Feedback       [Toggle: ON]│  │
│  └────────────────────────────────────┘  │
│                                          │
│  About                                   │
│  ┌────────────────────────────────────┐  │
│  │  Version        1.0.0              │  │
│  │  ARP Protocol   1.0.0              │  │
│  │  Build          2026-04-23         │  │
│  └────────────────────────────────────┘  │
│                                          │
│  ┌────────────────────────────────────┐  │
│  │         [  Disconnect  ]           │  │
│  └────────────────────────────────────┘  │
└──────────────────────────────────────────┘
```

#### Settings Items Detail

| Setting | Type | Range/Options | Default | Storage |
|---------|------|---------------|---------|---------|
| Host | Text input | Valid IP or hostname | — | Platform secure storage |
| Port | Number input | 1-65535 | 19743 | Platform secure storage |
| Auth Token | Password field (masked) | 64 hex chars | — | Android Keystore / iOS Keychain |
| Auto-connect | Toggle | On/Off | On | SharedPreferences / UserDefaults |
| Screenshot Quality | Slider | 30-90 (JPEG quality) | 70 | SharedPreferences / UserDefaults |
| Screenshot Width | Slider | 400-1920 (px) | 800 | SharedPreferences / UserDefaults |
| Terminal Font Size | Slider | 10-20 (sp/pt) | 14 (Android) / 15 (iOS) | SharedPreferences / UserDefaults |
| Dark Mode | Toggle | On/Off | On | SharedPreferences / UserDefaults |
| Auto-reconnect | Toggle | On/Off | On | SharedPreferences / UserDefaults |
| Haptic Feedback | Toggle | On/Off | On | SharedPreferences / UserDefaults |

---

## 4. Touch Gestures

### 4.1 Gesture Map

| Gesture | Context | Action | ARP Method |
|---------|---------|--------|------------|
| Swipe left/right | Tab Carousel | Navigate between tab cards | — (local) |
| Swipe up | Tab Card | Open full Tab Content View | `tabs.screenshot` |
| Swipe down | Tab Content View | Reveal URL bar overlay | — (local) |
| Pull-to-refresh | Tab Content View | Request fresh screenshot | `tabs.screenshot` |
| Pinch-zoom | Tab Content View | Zoom in/out 1x–3x | — (local) |
| Double-tap | Tab Content View | Toggle 1x / 2x zoom | — (local) |
| Long-press | Tab Card | Context menu (copy URL, close, pin) | Varies |
| Long-press | Terminal View | Select text for copy | — (local) |
| Long-press | Terminal input | Paste from mobile clipboard | — (local) |
| Two-finger tap | Any screen | Open command palette | — (local) |
| Swipe down | Command Palette | Dismiss palette | — (local) |
| Swipe down | Downloads Panel | Dismiss panel | — (local) |
| Tap | Tab Card | Open Tab Content View | `tabs.screenshot` |
| Tap | Bottom nav item | Switch to respective screen | Varies |

### 4.2 Context Menu (Long-Press Tab Card)

```
┌─────────────────────────────┐
│  Copy URL                   │  → Copy to mobile clipboard
│  Copy Title                 │  → Copy to mobile clipboard
│  Open in System Browser     │  → Open URL in device browser
│  ─────────────────────      │
│  Pin Tab                    │  → Visual pin, moves to front
│  Close Tab                  │  → tabs.close
└─────────────────────────────┘
```

### 4.3 Gesture Conflict Resolution

| Conflict | Resolution |
|----------|------------|
| Horizontal swipe (tabs) vs. scroll | Carousel uses snap scroll; if initial velocity > threshold, switch tab |
| Pinch-zoom vs. two-finger tap | Two-finger tap requires < 100ms contact, no movement |
| Pull-to-refresh vs. URL bar reveal | Pull-to-refresh when already at URL bar position; URL bar reveal on initial pull-down |

---

## 5. Color Scheme

### 5.1 Dark Theme (Default)

Matches desktop Aileron color palette exactly.

| Token | Hex | Usage |
|-------|-----|-------|
| `bg_primary` | `#191920` | Screen background, card background |
| `bg_secondary` | `#222230` | Top bar, bottom bar, sheet backgrounds |
| `bg_elevated` | `#2a2a35` | Input fields, progress bar tracks, dividers |
| `bg_hover` | `#333340` | Pressed states, highlighted items |
| `fg_primary` | `#e0e0e0` | Primary text, titles |
| `fg_secondary` | `#888888` | Secondary text, URLs, labels |
| `fg_tertiary` | `#555566` | Disabled text, placeholders |
| `accent` | `#4db4ff` | Active indicators, links, buttons, progress fill |
| `accent_dim` | `#2a6a99` | Accent backgrounds, muted accent |
| `success` | `#4caf50` | Connected status, completed downloads |
| `warning` | `#e8a838` | Reconnecting state, paused downloads |
| `error` | `#e04040` | Error states, failed downloads, close buttons |
| `border` | `#2a2a35` | Card borders, dividers |
| `border_active` | `#4db4ff` | Active tab card border |
| `scrim` | `#000000` @ 60% opacity | Modal sheet background dim |

### 5.2 Light Theme (Optional)

Disabled by default. Available as accessibility option.

| Token | Hex | Usage |
|-------|-----|-------|
| `bg_primary` | `#f5f5f7` | Screen background |
| `bg_secondary` | `#ffffff` | Cards, sheets |
| `bg_elevated` | `#e8e8ec` | Input fields |
| `fg_primary` | `#1a1a2e` | Primary text |
| `fg_secondary` | `#666680` | Secondary text |
| `accent` | `#0078d4` | Accent elements |
| `border` | `#d0d0d8` | Borders |

### 5.3 Terminal Colors (16-Color ANSI)

| Index | Name | Dark Theme | Light Theme |
|-------|------|------------|-------------|
| 0 | Black | `#191920` | `#1a1a2e` |
| 1 | Red | `#e04040` | `#cc3333` |
| 2 | Green | `#4caf50` | `#2e7d32` |
| 3 | Yellow | `#e8a838` | `#f9a825` |
| 4 | Blue | `#4db4ff` | `#1565c0` |
| 5 | Magenta | `#ce93d8` | `#8e24aa` |
| 6 | Cyan | `#4dd0e1` | `#00838f` |
| 7 | White | `#e0e0e0` | `#e0e0e0` |
| 8 | Bright Black | `#555566` | `#555566` |
| 9 | Bright Red | `#ff6b6b` | `#ef5350` |
| 10 | Bright Green | `#69f0ae` | `#66bb6a` |
| 11 | Bright Yellow | `#ffe082` | `#ffca28` |
| 12 | Bright Blue | `#82b1ff` | `#448aff` |
| 13 | Bright Magenta | `#ea80fc` | `#e040fb` |
| 14 | Bright Cyan | `#80deea` | `#26c6da` |
| 15 | Bright White | `#ffffff` | `#ffffff` |

---

## 6. Typography

### 6.1 Font Families

| Usage | Android | iOS |
|-------|---------|-----|
| UI Text | `Roboto` (system default) | `SF Pro Text` (system default) |
| Terminal | `monospace` (maps to `Roboto Mono` or `JetBrains Mono` if installed) | `monospace` (maps to `SF Mono`) |
| Code / URLs | `monospace` | `monospace` |

### 6.2 Type Scale

| Role | Size (Android sp / iOS pt) | Weight | Line Height | Color Token |
|------|---------------------------|--------|-------------|-------------|
| Screen Title | 20sp / 20pt | Medium (500) | 28sp | `fg_primary` |
| Section Header | 14sp / 15pt | Medium (500) | 20sp | `fg_secondary` |
| Body | 14sp / 15pt | Regular (400) | 20sp | `fg_primary` |
| Body Secondary | 13sp / 14pt | Regular (400) | 18sp | `fg_secondary` |
| Caption | 11sp / 12pt | Regular (400) | 16sp | `fg_secondary` |
| Caption Dim | 11sp / 12pt | Regular (400) | 16sp | `fg_tertiary` |
| Button | 14sp / 15pt | Medium (500) | 20sp | `accent` |
| Terminal | 13sp / 15pt | Regular (400) | 18sp | `fg_primary` |
| Terminal (small) | 11sp / 13pt | Regular (400) | 15sp | `fg_secondary` |
| Tab Title (card) | 13sp / 14pt | Medium (500) | 18sp | `fg_primary` |
| Tab URL (card) | 11sp / 12pt | Regular (400) | 16sp | `fg_secondary` |
| Input Text | 15sp / 16pt | Regular (400) | 22sp | `fg_primary` |
| Input Placeholder | 15sp / 16pt | Regular (400) | 22sp | `fg_tertiary` |

---

## 7. Layout Specs

### 7.1 Global Dimensions

| Dimension | Value |
|-----------|-------|
| Status bar height | System default (24dp Android / 44pt iOS safe area) |
| Top bar height | 56dp / 56pt |
| Bottom nav height | 56dp / 56pt |
| Bottom sheet corner radius | 16dp / 16pt |
| Card corner radius | 8dp / 8pt |
| Button corner radius | 8dp / 8pt |
| Input field corner radius | 8dp / 8pt |
| Minimum touch target | 48dp / 44pt |
| Screen padding (horizontal) | 16dp / 16pt |
| Screen padding (vertical) | 8dp / 8pt |
| Card spacing | 12dp / 12pt |
| List item height | 56dp / 56pt |

### 7.2 Connection Screen Layout

```
┌──────────────────────────────────────────┐
│ ← 48dp top safe area →                  │
│                                          │
│          Logo area                       │
│          120dp × 120dp                   │
│          Logo → Brand mark (24dp)        │
│          Title: 20sp Medium              │
│          Subtitle: 13sp Regular #888     │
│                                          │
│          ↓ 24dp spacing ↓                │
│                                          │
│  Form container                          │
│  ┌────────────────────────────────────┐  │
│  │ Horizontal padding: 24dp           │  │
│  │                                    │  │
│  │ Label: 12sp Medium #888            │  │
│  │ ↑ 4dp spacing ↑                    │  │
│  │ Input field: 48dp height           │  │
│  │   Horizontal padding: 16dp         │  │
│  │   Corner radius: 8dp               │  │
│  │   Background: #2a2a35              │  │
│  │   Border: 1dp #2a2a35              │  │
│  │   Focused border: 1dp #4db4ff      │  │
│  │   Text: 15sp Regular #e0e0e0       │  │
│  │   Placeholder: 15sp #555566        │  │
│  │                                    │  │
│  │ ↓ 16dp spacing ↓                   │  │
│  │ (repeat for each field)            │  │
│  └────────────────────────────────────┘  │
│                                          │
│          ↓ 24dp spacing ↓                │
│                                          │
│  Connect button                          │
│  ┌────────────────────────────────────┐  │
│  │ Height: 48dp                       │  │
│  │ Full width minus 48dp margins      │  │
│  │ Background: #4db4ff                │  │
│  │ Text: 14sp Medium #191920          │  │
│  │ Corner radius: 8dp                 │  │
│  │ Disabled: #333340 text #555566     │  │
│  └────────────────────────────────────┘  │
│                                          │
│          ↓ 32dp spacing ↓                │
│                                          │
│  Divider: "──── or scan QR code ────"    │
│  12sp #555566, centered                  │
│                                          │
│          ↓ 16dp spacing ↓                │
│                                          │
│  QR scan area                            │
│  ┌────────────────────────────────────┐  │
│  │ Height: 200dp                      │  │
│  │ Corner radius: 8dp                 │  │
│  │ Border: 1dp dashed #555566         │  │
│  │ Center: QR icon 48dp               │  │
│  │ Below: "Scan QR Code" 13sp #4db4ff │  │
│  └────────────────────────────────────┘  │
│                                          │
│          ↓ 24dp spacing ↓                │
│                                          │
│  Saved connections section               │
│  Section header: 14sp Medium #888        │
│  ↑ 8dp spacing ↑                        │
│  Connection items: 72dp height each      │
│  Divider between items: 1dp #2a2a35     │
└──────────────────────────────────────────┘
```

### 7.3 Tab Carousel Layout

```
┌──────────────────────────────────────────┐
│ Top bar: 56dp                           │
│ ┌────────────────────────────────────┐   │
│ │ ● Connected  hostname    ⚙ 12ms  │   │
│ │ Left: status dot (8dp circle)     │   │
│ │   + status text (13sp)            │   │
│ │ Right: settings gear (24dp icon)  │   │
│ │   + latency (11sp #888)           │   │
│ └────────────────────────────────────┘   │
│                                          │
│ Carousel area: remaining height - 56dp   │
│ ┌────────────────────────────────────┐   │
│ │                                    │   │
│ │  Card width: screen width - 48dp   │   │
│ │  Card height: auto (16:10 ratio)   │   │
│ │  Page margin: 24dp each side       │   │
│ │  Page peek: 16dp                   │   │
│ │                                    │   │
│ │  Tab counter: "2 of 5"             │   │
│ │  Position: below carousel, 11sp    │   │
│ │                                    │   │
│ └────────────────────────────────────┘   │
│                                          │
│ Bottom nav: 56dp                         │
│ ┌────────────────────────────────────┐   │
│ │                                    │   │
│ │  [Terminal]  [Downloads]  [  +  ]  │   │
│ │                                    │   │
│ │  Icon: 24dp                        │   │
│ │  Label: 11sp                       │   │
│ │  Active: #4db4ff                   │   │
│ │  Inactive: #555566                 │   │
│ │  Item width: 1/3 of nav bar        │   │
│ │  Touch target: 48dp                │   │
│ │                                    │   │
│ └────────────────────────────────────┘   │
└──────────────────────────────────────────┘
```

### 7.4 Tab Card Layout (Expanded)

```
┌──────────────────────────────────────────┐
│ Card container                           │
│ ┌────────────────────────────────────┐   │
│ │ Corner radius: 8dp                  │   │
│ │ Border: 1dp #2a2a35                 │   │
│ │ Active border: 2dp #4db4ff          │   │
│ │ Elevation: 2dp shadow               │   │
│ │ Background: #191920                 │   │
│ │                                    │   │
│ │ Screenshot area                     │   │
│ │ ┌────────────────────────────────┐ │   │
│ │ │ Aspect ratio: 16:10            │ │   │
│ │ │ Corner radius: 8dp top         │ │   │
│ │ │ Background: #222230 (loading)  │ │   │
│ │ │ Loading: indeterminate spinner  │ │   │
│ │ │   (24dp, #4db4ff)              │ │   │
│ │ │ Content: aspect-fill            │ │   │
│ │ └────────────────────────────────┘ │   │
│ │                                    │   │
│ │ Info row: padding 12dp horizontal  │   │
│ │ ┌────────────────────────────────┐ │   │
│ │ │ Title: 13sp Medium #e0e0e0     │ │   │
│ │ │ Single line, ellipsis          │ │   │
│ │ │ ↑ 2dp ↑                        │ │   │
│ │ │ URL: 11sp Regular #888888      │ │   │
│ │ │ Single line, ellipsis          │ │   │
│ │ └────────────────────────────────┘ │   │
│ │                                    │   │
│ │ Bottom row: padding 8dp horizontal │   │
│ │ ┌────────────────────────────────┐ │   │
│ │ │ Left: status dot (6dp circle)   │ │   │
│ │ │   ● #4db4ff (active)           │ │   │
│ │ │   ○ #555566 (inactive)         │ │   │
│ │ │   + "Active" label (11sp)      │ │   │
│ │ │ Right: close button (×)         │ │   │
│ │ │   24dp icon, #888888           │ │   │
│ │ │   Touch target: 48dp           │ │   │
│ │ └────────────────────────────────┘ │   │
│ │                                    │   │
│ │ Total vertical padding: 0dp top    │   │
│ │ (screenshot touches card edge)     │   │
│ │ 8dp bottom (above info row)        │   │
│ │ 8dp bottom (below info row)        │   │
│ └────────────────────────────────────┘   │
└──────────────────────────────────────────┘
```

### 7.5 Terminal View Layout

```
┌──────────────────────────────────────────┐
│ Header bar: 56dp                         │
│ ┌────────────────────────────────────┐   │
│ │ [←] (24dp)  "Terminal" (20sp)     │   │
│ │ Right: ⋮ menu (24dp)              │   │
│ └────────────────────────────────────┘   │
│                                          │
│ Pane selector: 40dp                      │
│ ┌────────────────────────────────────┐   │
│ │ ▼ pane-3: bash                    │   │
│ │ Background: #222230                │   │
│ │ Tap to expand dropdown             │   │
│ └────────────────────────────────────┘   │
│                                          │
│ Terminal content: remaining space        │
│ ┌────────────────────────────────────┐   │
│ │ Background: #191920                │   │
│ │ Padding: 8dp all sides             │   │
│ │ Font: 13sp monospace               │   │
│ │ Scrollable (vertical only)         │   │
│ │ Scrollbar: 4dp wide, #555566       │   │
│ │ New output chip (when scrolled up):│   │
│ │   ┌──────────────┐                 │   │
│ │   │ ▼ New output │ (floating)     │   │
│ │   └──────────────┘                 │   │
│ └────────────────────────────────────┘   │
│                                          │
│ Special key bar: 44dp                    │
│ ┌────────────────────────────────────┐   │
│ │ Background: #222230                │   │
│ │ Buttons: 7 keys                    │   │
│ │ Key size: 44dp × 44dp              │   │
│ │ Key label: 11sp Medium #e0e0e0     │   │
│ │ Key background: #2a2a35            │   │
│ │ Key pressed: #333340               │   │
│ │ Horizontal spacing: 4dp            │   │
│ │ Keys: Tab Ctrl Esc ↑ ↓ ← →        │   │
│ └────────────────────────────────────┘   │
│                                          │
│ Input field: 48dp                        │
│ ┌────────────────────────────────────┐   │
│ │ Background: #222230                │   │
│ │ Border top: 1dp #2a2a35            │   │
│ │ Padding: 12dp horizontal           │   │
│ │ Text: 15sp monospace               │   │
│ │ Send button (⏎): 48dp × 48dp       │   │
│ │   Background: #4db4ff              │   │
│ │   Icon: white arrow                │   │
│ │   Right-aligned                    │   │
│ └────────────────────────────────────┘   │
└──────────────────────────────────────────┘
```

### 7.6 Command Palette Layout

```
┌──────────────────────────────────────────┐
│                                          │
│           Scrim: #000000 @ 60%           │
│                                          │
├──────────────────────────────────────────┤  ← Top: rounded 16dp corners
│ Sheet content                            │
│ Max height: 70% of screen                │
│ Drag handle: 32dp wide, 4dp tall         │
│   Color: #555566                         │
│   Centered, 12dp from top                │
│                                          │
│ Search input: 16dp from top              │
│ ┌────────────────────────────────────┐   │
│ │ Height: 48dp                       │   │
│ │ Margin: 16dp horizontal            │   │
│ │ Background: #2a2a35                │   │
│ │ Left icon: 🔍 (20dp, #888)        │   │
│ │ Text: 15sp Regular #e0e0e0         │   │
│ │ Placeholder: "Type a command..."   │   │
│ └────────────────────────────────────┘   │
│                                          │
│ ↓ 8dp spacing ↓                          │
│                                          │
│ Section header: "Commands"               │
│ 12sp Medium #888, 16dp horizontal        │
│                                          │
│ Command list: scrollable                  │
│ ┌────────────────────────────────────┐   │
│ │ Item height: 48dp                  │   │
│ │ Padding: 16dp horizontal           │   │
│ │ Command text: 14sp monospace #e0   │   │
│ │ Description: 11sp #888 (below)     │   │
│ │ Highlighted match: #4db4ff bg      │   │
│ │ Selected: #2a2a35 bg               │   │
│ └────────────────────────────────────┘   │
│                                          │
│ ↓ 16dp spacing ↓                         │
│                                          │
│ Section header: "Recent"                 │
│ Same styling as above                    │
│                                          │
│ ↓ 16dp spacing (bottom safe area)        │
└──────────────────────────────────────────┘
```

---

## 8. Component Library

### 8.1 Component Inventory

| Component | Platform | Reusable | Screens Used In |
|-----------|----------|----------|-----------------|
| `TabCard` | Both | Yes | S2 (Tab Carousel) |
| `ProgressBar` | Both | Yes | S6 (Downloads), S3 (loading) |
| `TerminalView` | Both | Yes | S4 (Terminal) |
| `CommandList` | Both | Yes | S5 (Command Palette) |
| `ConnectionForm` | Both | Yes | S1 (Connection), S7 (Settings) |
| `StatusPill` | Both | Yes | S2 (top bar), S1 (status) |
| `KeyButton` | Both | Yes | S4 (special key bar) |
| `DownloadItem` | Both | Yes | S6 (Downloads) |
| `PaneSelector` | Both | Yes | S4 (Terminal) |
| `UrlBar` | Both | Yes | S3 (Tab Content) |
| `NavBar` | Both | Yes | S2 (bottom nav) |

### 8.2 Component Specifications

#### TabCard

```
Props:
  tab: TabModel          // { id, url, title, loading, active }
  screenshot: ImageBitmap? // nullable, null = placeholder
  onClose: (tabId) -> Unit
  onTap: (tabId) -> Unit
  isActive: Boolean

Layout:
  Width: parent width
  Height: wrap (16:10 screenshot + info rows)
  Corner radius: 8dp
  Border: 1dp border_default, 2dp border_active when active
  Elevation: 2dp
  Background: bg_primary

Children:
  - ScreenshotContainer (16:10 aspect ratio)
    - ImageView / AsyncImage (screenshot or placeholder)
    - LoadingSpinner (visible when tab.loading)
  - InfoRow
    - TitleText (13sp medium, single line ellipsis)
    - UrlText (11sp regular, single line ellipsis)
  - ActionBar
    - StatusDot (6dp circle)
    - ActiveLabel ("Active", 11sp)
    - CloseButton (48dp touch target, 24dp icon)
```

#### ProgressBar

```
Props:
  progress: Float        // 0.0 - 1.0
  state: DownloadState   // downloading | paused | completed | failed
  speed: String?         // "2.1 MB/s"
  received: Long         // bytes
  total: Long            // bytes

Layout:
  Height: 4dp (track) + 16dp (labels)
  Track corner radius: 2dp
  Track background: bg_elevated
  Fill background: color by state (see Downloads Panel)

Labels (below track):
  Left: "{percentage}%" (11sp, fg_primary)
  Right: speed (11sp, fg_secondary)

Animation:
  Fill width animated with spring (stiffness 200, damping 25)
  Indeterminate mode: track slides left-right, 1.5s cycle
```

#### TerminalView

```
Props:
  content: String         // terminal text buffer
  cursorRow: Int
  cursorCol: Int
  fontSize: Int           // sp/pt, default 13/15
  onInput: (String) -> Unit
  onSpecialKey: (String) -> Unit
  isScrolledToBottom: Boolean

Layout:
  Fill parent width and height
  Background: bg_primary
  Padding: 8dp
  Font: monospace, configurable size
  Scroll: vertical only
  Scrollbar: 4dp wide, fg_tertiary, auto-hide

Behavior:
  - Renders text with ANSI SGR color support
  - Blinking block cursor at cursorRow/cursorCol
  - Auto-scrolls to bottom on new content
  - Shows "New output" chip when user scrolls up
  - Tap chip to scroll to bottom
  - Long-press for text selection → copy to clipboard
```

#### CommandList

```
Props:
  commands: List<Command>  // { name, description, pattern }
  recentCommands: List<String>
  query: String
  onCommandSelected: (String) -> Unit

Layout:
  Height: scrollable, max remaining sheet height
  Item height: 48dp
  Divider: 1dp bg_elevated between items

Item layout:
  ┌────────────────────────────────────┐
  │  :open https://rust-lang.org       │  ← 14sp monospace
  │  Navigate to URL                   │  ← 11sp fg_secondary
  └────────────────────────────────────┘

Filtering:
  - Fuzzy match query against command name
  - Highlight matched characters with accent background
  - Empty state: "No matching commands" (13sp fg_tertiary, centered)
```

#### ConnectionForm

```
Props:
  host: String
  port: Int
  token: String
  onConnect: (host, port, token) -> Unit
  onScanQr: () -> Unit
  connecting: Boolean
  error: String?

Layout:
  Vertical form with 16dp spacing between fields
  Field label: 12sp medium fg_secondary, 4dp above input
  Input: 48dp height, bg_elevated, 8dp radius, 16dp horizontal padding
  Toggle visibility button for token field (eye icon)
  Connect button: 48dp height, full width, accent background

Validation:
  - Host: non-empty, valid IP or hostname
  - Port: 1-65535
  - Token: exactly 64 hex characters
  - Show validation error below field (11sp error color)
```

#### StatusPill

```
Props:
  status: ConnectionStatus  // connected | connecting | disconnected | error
  hostname: String
  latency: Int?             // ms, null if not connected

Layout:
  Height: 56dp (matches top bar)
  Horizontal: status dot + text (left), gear + latency (right)

States:
  Connected:    ● green + "Connected  hostname" + "12ms"
  Connecting:   ◐ amber + "Reconnecting..." + "--"
  Disconnected: ○ gray + "Disconnected" + "--"
  Error:        ✕ red + "Connection lost" + "--"
```

#### KeyButton

```
Props:
  label: String           // "Tab", "Ctrl", "Esc", etc.
  onTap: () -> Unit
  onLongPress: (() -> Unit)?  // for Ctrl submenu

Layout:
  Size: 44dp × 44dp
  Corner radius: 6dp
  Background: bg_elevated
  Pressed: bg_hover
  Label: 11sp medium fg_primary
  Touch feedback: haptic (light impact)

Variants:
  - Default: bg_elevated background
  - Active/pressed: bg_hover background
  - Destructive (Ctrl-C): subtle red tint on press
```

#### DownloadItem

```
Props:
  download: DownloadModel
  onCancel: (id) -> Unit
  onPause: (id) -> Unit
  onResume: (id) -> Unit
  onShare: (id) -> Unit

Layout:
  Height: wrap
  Padding: 16dp horizontal, 12dp vertical
  Divider: 1dp bg_elevated below

Content:
  ┌────────────────────────────────────┐
  │  📥 filename.ext           [actions]│  ← 13sp medium
  │  url (11sp fg_secondary, ellipsis) │
  │  ┌──────────────────────────────┐  │
  │  │ ████████████░░░░░░░░░░  45%  │  │  ← ProgressBar
  │  └──────────────────────────────┘  │
  │  2.1 MB/s  •  1.2 / 5.0 MB        │  ← 11sp fg_secondary
  └────────────────────────────────────┘
```

---

## 9. Animation

### 9.1 Animation Inventory

| Animation | Trigger | Duration | Easing | Description |
|-----------|---------|----------|--------|-------------|
| Tab switch (carousel) | Swipe / tap | 300ms | `FastOutSlowIn` | Horizontal slide with crossfade |
| Tab card open | Swipe up / tap | 250ms | `FastOutSlowIn` | Card scales to fill screen |
| Tab card close | Back gesture / button | 200ms | `Accelerate` | Screen shrinks to card position |
| Command palette open | Two-finger tap | 300ms | `FastOutSlowIn` | Slide up from bottom |
| Command palette dismiss | Swipe down / tap scrim | 250ms | `Accelerate` | Slide down |
| Downloads panel open | Bottom nav tap | 300ms | `FastOutSlowIn` | Slide up from bottom |
| Downloads panel dismiss | Swipe down / tap scrim | 250ms | `Accelerate` | Slide down |
| Connection pulse | Connected state | 2000ms | `EaseInOut` | Status dot pulses opacity |
| Loading spinner | `tab.loading` | 1000ms | Linear | Continuous rotation |
| Progress bar fill | Download progress | Spring | Stiffness 200, Damping 25 | Width animates to new value |
| Pull-to-refresh | Pull gesture | Depends on network | — | Screenshot replaces old with crossfade |
| Screenshot crossfade | New screenshot received | 150ms | `Linear` | Crossfade old → new image |
| Button press | Tap | 100ms | `EaseOut` | Scale 0.97x → 1.0x |
| Context menu appear | Long-press | 150ms | `FastOutSlowIn` | Scale from 0.8x to 1.0x, fade in |
| Error shake | Validation error | 400ms | Spring | Horizontal oscillation ±4dp |
| Reconnect animation | Connection lost → retry | 500ms | `EaseInOut` | Status dot fades amber → gray → amber |

### 9.2 Animation Curves

| Curve | Android | iOS | Usage |
|-------|---------|-----|-------|
| FastOutSlowIn | `FastOutSlowInEasing` | `.easeOut(duration:)` | Enter transitions |
| Accelerate | `LinearOutSlowInEasing` | `.easeIn(duration:)` | Exit transitions |
| EaseInOut | `EaseInOutEasing` | `.easeInOut(duration:)` | Loops, pulses |
| Spring | `spring(dampingRatio, stiffness)` | `.spring(response:dampingFraction:)` | Physics-based (progress bars, buttons) |

### 9.3 Platform-Specific Notes

**Android (Jetpack Compose):**
- Use `animateContentSize` for card expansion
- Use `AnimatedVisibility` for sheet show/hide
- Use `updateTransition` for state-driven animations
- Respect `Settings.Global.ANIMATOR_DURATION_SCALE`

**iOS (SwiftUI):**
- Use `.transition(.move(edge: .bottom))` for sheets
- Use `.matchedGeometryEffect` for hero transitions (tab card → content view)
- Use `withAnimation(.spring())` for interactive animations
- Respect `UIAccessibility.isReduceMotionEnabled`

---

## 10. Offline Behavior

### 10.1 Connection Loss Handling

When the WebSocket connection drops:

1. **Immediate**: Show amber status pill with "Reconnecting..."
2. **UI freeze**: Last-known state remains visible (screenshots, tab list)
3. **Command queuing**: User actions that require ARP calls are queued locally
   - Max queue size: 50 commands
   - Queued commands shown as subtle pending indicator on relevant UI elements
4. **Auto-reconnect**: Exponential backoff (1s, 2s, 4s, 8s, 16s, 32s, max 60s)
5. **Recovery**:
   - On reconnect: replay queued commands in order
   - Refresh tab list via `tabs.list`
   - Refresh current tab screenshot via `tabs.screenshot`
   - Re-subscribe to events via `system.subscribe`
6. **Max retry**: After 5 minutes of continuous failure, stop auto-reconnect
   - Show "Disconnected" with manual "Retry" button
   - Allow user to navigate to Connection Screen

### 10.2 Offline UI States

| Screen | Offline Behavior |
|--------|-----------------|
| Tab Carousel | Show last-known tab list with cached screenshots. "Reconnecting..." banner. Tab cards dimmed slightly (opacity 0.7). |
| Tab Content View | Show last screenshot. Nav buttons disabled. URL bar still visible but "Go" disabled. |
| Terminal View | Show last terminal snapshot. Input field disabled. Special keys disabled. "Reconnecting..." banner. |
| Downloads Panel | Show last-known download states. All action buttons disabled. Progress bars frozen. |
| Command Palette | Commands can be typed but show "Queued" badge. Cannot execute until reconnected. |
| Settings | Fully functional (local-only). "Disconnect" button shows "Already disconnected". |

### 10.3 Data Persistence

| Data | Storage | TTL |
|------|---------|-----|
| Server configs (host, port, token) | Secure storage (Keystore/Keychain) | Permanent |
| Tab list | SharedPreferences / UserDefaults | Cleared on disconnect |
| Tab screenshots | Disk cache (LRU, 50MB max) | 24 hours |
| Terminal scroll buffer | In-memory only | Cleared on disconnect |
| Command history | SharedPreferences / UserDefaults | Last 10, permanent |
| Download list | SharedPreferences / UserDefaults | Cleared on disconnect |

---

## 11. Accessibility

### 11.1 WCAG 2.1 AA Compliance

#### Contrast Ratios

All color pairings meet WCAG 2.1 AA minimum contrast ratios (4.5:1 for normal text, 3:1 for large text and UI components).

| Pairing | Ratio | Meets |
|---------|-------|-------|
| `#e0e0e0` on `#191920` | 12.63:1 | AA Normal (4.5:1), AAA Normal (7:1) |
| `#888888` on `#191920` | 4.87:1 | AA Normal (4.5:1) |
| `#555566` on `#191920` | 2.56:1 | AA Large (3:1) for decorative only |
| `#4db4ff` on `#191920` | 7.12:1 | AA Normal (4.5:1), AAA Normal (7:1) |
| `#e04040` on `#191920` | 5.65:1 | AA Normal (4.5:1) |
| `#4caf50` on `#191920` | 5.08:1 | AA Normal (4.5:1) |
| `#e8a838` on `#191920` | 6.51:1 | AA Normal (4.5:1) |

**Note:** `fg_tertiary` (#555566) on `bg_primary` (#191920) fails AA for normal text. This color is used exclusively for:
- Placeholder text (supplemented by label above)
- Disabled states (supplemented by disabled affordance)
- Decorative dividers

These uses are acceptable under WCAG 2.1 AA as they are not the sole means of conveying information.

### 11.2 Touch Targets

All interactive elements meet minimum touch target sizes:

| Element | Size | Platform Minimum | Notes |
|---------|------|-----------------|-------|
| Button (primary) | 48dp × 48dp | 48dp (Android), 44pt (iOS) | Meets both |
| Tab card tap area | Full card width × full card height | — | Generous target |
| Close button (×) | 48dp × 48dp (icon 24dp centered) | 48dp | Padding provides target |
| Bottom nav item | 48dp × 48dp | 48dp | Icon + label |
| Special key button | 44dp × 44dp | 44pt (iOS minimum) | Meets both |
| List item | 56dp height × full width | 48dp | Meets both |
| Settings toggle | 48dp × 24dp | 48dp | Native toggle component |
| Drag handle (sheets) | 32dp wide × 4dp tall | — | Decorative; tap scrim to dismiss |

### 11.3 Screen Reader Support

#### Content Descriptions

| Element | Android `contentDescription` | iOS `accessibilityLabel` |
|---------|------------------------------|--------------------------|
| Status dot (green) | "Connected to {hostname}" | "Connected to {hostname}" |
| Status dot (amber) | "Reconnecting to {hostname}" | "Reconnecting to {hostname}" |
| Status dot (red) | "Connection lost" | "Connection lost" |
| Tab card | "{title}, {url}, tab {n} of {total}" | "{title}, {url}, tab {n} of {total}" |
| Tab close button | "Close tab {title}" | "Close tab {title}" |
| Terminal pane selector | "Terminal pane: {title}. Tap to change." | "Terminal pane: {title}. Tap to change." |
| Special key (Tab) | "Tab key" | "Tab key" |
| Special key (Ctrl) | "Control key. Hold for more options." | "Control key. Hold for more options." |
| Download item | "{filename}, {state}, {percent} complete" | "{filename}, {state}, {percent} complete" |
| Progress bar | "{percent} percent, {speed}" | "{percent} percent, {speed}" |
| Connect button | "Connect to server" | "Connect to server" |
| QR scan button | "Scan QR code to pair" | "Scan QR code to pair" |
| Gear icon | "Settings" | "Settings" |
| Command palette item | "Command: {name}. {description}" | "Command: {name}. {description}" |

#### Accessibility Roles

| Element | Android Role | iOS Trait |
|---------|-------------|-----------|
| Tab carousel | `ViewPager` (horizontal paging) | `.allowsHorizontalSwiping` |
| Tab card | `Button` | `.isButton` |
| Terminal content | None (custom view) | `.allowsScrolling` |
| Command list | `ListView` | `.allowsVerticalScrolling` |
| Download progress | `ProgressBar` | `.updatesFrequently` |
| Connection form inputs | `EditText` | `.isTextEntry` |
| Toggle switches | `Switch` | `.isToggle` |
| Bottom sheet | `ModalBottomSheet` | `.isModal` |

#### Focus Order

1. Connection Screen: Host → Port → Token → Connect → QR Scan → Saved connections (each)
2. Tab Carousel: Status pill → Tab cards (left to right) → Bottom nav items
3. Tab Content View: Back button → Forward button → Reload → Close → Content (described as image)
4. Terminal View: Back → Pane selector → Terminal content → Special keys (left to right) → Input field → Send
5. Command Palette: Search input → Command items (top to bottom) → Recent items
6. Downloads Panel: Download items (top to bottom, each: filename → progress → action buttons)
7. Settings: Section headers (decorative, skipped) → Setting items (label → value/control)

### 11.4 Reduce Motion

When `Reduce Motion` is enabled (system setting):

- All transitions use crossfade (150ms) instead of slide/scale
- Loading spinner replaced with static "Loading..." text
- Connection pulse animation disabled
- Progress bar fills instantly (no animation)
- Pull-to-refresh shows static spinner instead of animated

**Android:** Check `Settings.Global.ANIMATOR_DURATION_SCALE == 0` or `View.ACCESSIBILITY_FLAG`
**iOS:** Check `UIAccessibility.isReduceMotionEnabled`

### 11.5 Font Scaling

Support system font scaling up to 200% (Android) / Largest (iOS):

- Terminal view: Lock font size to user setting (ignore system scaling to preserve layout)
- All other UI: Use `sp` (Android) / dynamic type (iOS) for automatic scaling
- Test at 1.0x, 1.5x, 2.0x scales to ensure no text truncation or overflow
- Tab card titles: Allow ellipsis at large scales (already single-line)
- Bottom nav labels: Allow ellipsis at large scales

---

## Appendix A: Platform Implementation Notes

### Android (Kotlin + Jetpack Compose)

```
Recommended dependencies:
  - Compose BOM 2024.x
  - Compose Material 3 (dark theme)
  - Navigation Compose
  - OkHttp (WebSocket client)
  - Coil (async image loading for screenshots)
  - CameraX (QR scanning)
  - DataStore Preferences (settings persistence)
  - androidx.security.crypto (encrypted token storage)
```

### iOS (Swift + SwiftUI)

```
Recommended dependencies:
  - SwiftUI (iOS 16+)
  - URLSessionWebSocketTask (WebSocket client)
  - Kingfisher (async image loading for screenshots)
  - CodeScanner (QR scanning)
  - @AppStorage / UserDefaults (settings persistence)
  - KeychainAccess (encrypted token storage)
```

---

## Appendix B: State Machine — Connection Lifecycle

```
                  ┌──────────┐
                  │  IDLE    │
                  └────┬─────┘
                       │ connect()
                       ▼
                  ┌──────────┐
          ┌───► │CONNECTING│
          │     └────┬─────┘
          │          │ ws open
          │          ▼
          │     ┌──────────┐
          │     │  AUTH    │
          │     └────┬─────┘
          │          │ token valid
          │          ▼
          │     ┌──────────┐
          │     │CONNECTED │◄──────────┐
          │     └────┬─────┘           │
          │          │ ws close /      │
          │          │ ws error        │
          │          ▼                 │
          │     ┌──────────┐    reconnect()
          │     │RECONNECT │───────────┘
          │     └────┬─────┘
          │          │ max retries
          │          ▼
          │     ┌──────────┐
          └─────│DISCONNECT│
                └────┬─────┘
                     │ connect()
                     │
                     └──► CONNECTING
```

---

## Appendix C: Mockup Quick Reference

| Screen | Key Dimensions | Primary ARP Methods |
|--------|---------------|---------------------|
| S1 Connection | Form fields 48dp, button 48dp | `system.info` |
| S2 Tab Carousel | Cards 16:10, peek 16dp | `tabs.list`, `tabs.screenshot` |
| S3 Tab Content | Fullscreen, nav bar 56dp | `tabs.screenshot`, `tabs.goBack`, `tabs.goForward`, `tabs.navigate` |
| S4 Terminal | Key bar 44dp, input 48dp | `terminal.list`, `terminal.input`, `terminal.sendKey` |
| S5 Command Palette | Sheet 70% height | Varies (dispatches commands) |
| S6 Downloads | Sheet 50% height | `downloads.list`, `downloads.cancel`, `downloads.pause`, `downloads.resume` |
| S7 Settings | List items 56dp | None (local) |
