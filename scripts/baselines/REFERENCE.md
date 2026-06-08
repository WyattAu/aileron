# Visual Regression Baseline Reference
#
# Generated: $(date +%Y-%m-%d)
# Aileron v0.21.0 -- Leptos WASM Chrome Webview
#
# This file documents the expected DOM structure and visual states
# for the chrome webview. Used by scripts/visual_regression.sh for
# drift detection.

## CSS Classes Reference

### Root
- `.chrome-root` -- Full-window flex column, transparent background, pointer-events: none

### Status Bar (.status-bar)
- `.status-url` -- Current URL display, truncated with ellipsis
- `.status-msg` -- Status message text
- `.mode-NORMAL` -- Blue (#89b4fa), bold
- `.mode-INSERT` -- Green (#a6e3a1), bold
- `.mode-COMMAND` -- Yellow (#f9e2af), bold
- Mode text appends " FIND" when find_bar_open=true

### URL Bar (.url-bar)
- `.url-input` -- Text input, disabled when not focused
- Focus state: border-color #89b4fa
- Disabled state: opacity 0.7

### Tab Sidebar (.tab-sidebar)
- `.sidebar-right` -- When tab_sidebar_right=true
- `.tab-item` -- Per-pane container
- `.tab-active` -- Active pane (blue left border #89b4fa)
- `.tab-title` -- Pane title, ellipsis truncated
- `.tab-close` -- Close button "x"
- `.tab-new` -- New tab button "+"

### Find Bar (.find-bar) [CONDITIONAL]
- `.find-label` -- "Find:" label
- `.find-input` -- Search input, placeholder "Search in page..."
- `.find-btn` -- Next (↓), Prev (↑), Close buttons
- `.find-close` -- Close button (✕)
- Hidden when find_bar_open=false

### Command Palette (.palette-backdrop) [CONDITIONAL]
- `.palette-container` -- Centered 520px modal
- `.palette-input-row` -- Input container with ": " prompt
- `.palette-input` -- Search input, placeholder "Search commands..."
- `.palette-results` -- Scrollable results list (max 280px)
- `.palette-item` -- Per-result row
- `.palette-selected` -- Selected result (blue left border)
- `.palette-cat` -- Category label: [H], [B], [>], [T], [S], [key], [L]
- `.palette-label` -- Result label text
- `.palette-desc` -- Result description
- Hidden when command_palette_open=false

## Test States

### S01: Default (Normal mode, empty tab)
- Status bar: mode="NORMAL", url="aileron://newtab", no status message
- URL bar: disabled, value="aileron://newtab"
- Sidebar: 1 tab item (active)
- Find bar: hidden
- Palette: hidden

### S02: Command mode (url bar focused)
- Status bar: mode="COMMAND"
- URL bar: enabled, focused, blue border
- Sidebar: unchanged

### S03: Insert mode
- Status bar: mode="INSERT"
- URL bar: disabled

### S04: Command palette empty
- Status bar: unchanged
- Palette backdrop: visible, centered
- Palette input: empty, focused
- Palette results: empty (or recent commands)

### S05: Command palette with query
- Palette input: value="git"
- Palette results: filtered list of git-related items
- Each item shows [category] label + description

### S06: Find bar open
- Status bar: mode appended with " FIND"
- Find bar: visible at bottom
- Find input: value="test query"
- Three buttons: ↓ ↑ ✕

### S07: Multiple tabs
- Sidebar: 2+ tab items
- First tab has .tab-active

### S08: Loaded URL
- Status bar: url="https://example.com"
- URL bar: disabled, value="https://example.com"

### S09: Split panes
- Sidebar: 2+ tab items (one per split)

## Catppuccin Mocha Theme (Color Reference)

| Element | Color | Hex |
|---------|-------|-----|
| Base background | Crust | #1e1e2e |
| Surface 0 | Mantle | #181825 |
| Surface 1 | Base | #313244 |
| Surface 2 | Surface0 | #45475a |
| Text | Text | #cdd6f4 |
| Subtext 0 | Subtext0 | #a6adc8 |
| Subtext 1 | Subtext1 | #6c7086 |
| Blue | Blue | #89b4fa |
| Green | Green | #a6e3a1 |
| Yellow | Yellow | #f9e2af |
| Red | Red | #f38ba8 |
| Lavender | Lavender | #b4befe |
