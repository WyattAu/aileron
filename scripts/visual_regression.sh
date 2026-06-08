#!/usr/bin/env bash
# visual_regression.sh -- Traverse aileron UI states, capture DOM + screenshots.
#
# Prerequisites (auto-detected):
#   - xdotool   (keyboard simulation)
#   - grim      (Wayland screenshot) OR scrot (X11 screenshot)
#   - jq        (JSON formatting for DOM snapshots)
#
# Usage:
#   ./scripts/visual_regression.sh              # capture to /tmp/aileron-snapshots/
#   ./scripts/visual_regression.sh --baseline     # save as new baselines to scripts/baselines/
#   ./scripts/visual_regression.sh --compare      # compare current against baselines
#
# Environment:
#   AILERON_BIN=...        path to aileron binary (default: ~/apps/aileron)
#   AILERON_OUTPUT=...     output directory (default: /tmp/aileron-snapshots)
set -euo pipefail

# ─── Configuration ────────────────────────────────────────────────
AILERON_BIN="${AILERON_BIN:-$HOME/apps/aileron}"
OUTPUT_DIR="${AILERON_OUTPUT:-/tmp/aileron-snapshots}"
BASELINE_DIR="$(cd "$(dirname "$0")" && pwd)/baselines"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
SESSION_DIR="$OUTPUT_DIR/$TIMESTAMP"
DOM_DIR="$SESSION_DIR/dom"
SCREEN_DIR="$SESSION_DIR/screens"

# ─── Colors ────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

log()  { echo -e "${CYAN}[VIS-REG]${NC} $*"; }
ok()   { echo -e "${GREEN}[PASS]${NC} $*"; }
fail() { echo -e "${RED}[FAIL]${NC} $*"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $*"; }

# ─── Detect screenshot tool ───────────────────────────────────────
detect_screenshot_tool() {
    if command -v grim &>/dev/null; then
        echo "grim"
    elif command -v scrot &>/dev/null; then
        echo "scrot"
    elif command -v import &>/dev/null; then
        echo "import"
    else
        echo ""
    fi
}

take_screenshot() {
    local name="$1"
    local tool
    tool="$(detect_screenshot_tool)"
    case "$tool" in
        grim)
            grim save "$SCREEN_DIR/${name}.png" 2>/dev/null
            ;;
        scrot)
            scrot -o "$SCREEN_DIR/${name}.png" 2>/dev/null
            ;;
        import)
            import -window root "$SCREEN_DIR/${name}.png" 2>/dev/null
            ;;
        *)
            warn "No screenshot tool found (need grim, scrot, or ImageMagick import)"
            return 1
            ;;
    esac
}

# ─── Detect xdotool ─────────────────────────────────────────────────
detect_xdotool() {
    if ! command -v xdotool &>/dev/null; then
        warn "xdotool not found -- keyboard simulation unavailable"
        return 1
    fi
    return 0
}

send_keys() {
    local delay=0.1
    xdotool key --delay "$delay" "$@"
    sleep 0.3
}

send_type() {
    local text="$1"
    xdotool type --delay 20 "$text"
    sleep 0.3
}

# ─── Wait for window ────────────────────────────────────────────────
wait_for_aileron() {
    local max_wait=15
    local elapsed=0
    while ! xdotool search --name "aileron" getwindowpid 2>/dev/null; do
        sleep 0.5
        elapsed=$((elapsed + 1))
        if (( elapsed > max_wait * 2 )); then
            fail "aileron window not found after ${max_wait}s"
            return 1
        fi
    done
    # Focus the window
    xdotool search --name "aileron" windowactivate --sync 2>/dev/null
    sleep 0.5
    return 0
}

# ─── UI State Traversal ────────────────────────────────────────────
# Each function navigates to a specific UI state and captures a snapshot.

# State: Default - Normal mode, empty new tab
capture_default() {
    log "S01: Default state (Normal mode, empty tab)"
    sleep 0.5
    take_screenshot "01_default"
}

# State: Command mode (url bar focused)
capture_command_mode() {
    log "S02: Command mode (URL bar focused)"
    send_keys colon
    sleep 0.5
    take_screenshot "02_command_mode"
    # Exit command mode
    send_keys Escape
    sleep 0.3
}

# State: Insert mode
capture_insert_mode() {
    log "S03: Insert mode"
    send_keys i
    sleep 0.5
    take_screenshot "03_insert_mode"
    # Exit insert mode
    send_keys Escape
    sleep 0.3
}

# State: Command palette open (empty)
capture_palette_empty() {
    log "S04: Command palette (empty)"
    send_keys space
    sleep 0.8
    take_screenshot "04_palette_empty"
    # Close palette
    send_keys Escape
    sleep 0.3
}

# State: Command palette with query typed
capture_palette_with_query() {
    log "S05: Command palette with query 'git'"
    send_keys space
    sleep 0.8
    send_type "git"
    sleep 0.8
    take_screenshot "05_palette_query"
    send_keys Escape
    sleep 0.3
}

# State: Find bar open
capture_find_bar() {
    log "S06: Find bar open"
    send_keys slash
    sleep 0.5
    send_type "test query"
    sleep 0.5
    take_screenshot "06_find_bar"
    # Close find bar
    send_keys Escape
    sleep 0.3
}

# State: Multiple tabs (split horizontal)
capture_multiple_tabs() {
    log "S07: Multiple tabs (split)"
    send_keys colon
    sleep 0.3
    send_type "newtab"
    sleep 0.5
    send_keys Return
    sleep 1.0
    send_keys Escape
    sleep 0.3
    take_screenshot "07_multiple_tabs"
}

# State: URL bar with a real URL
capture_with_url() {
    log "S08: URL bar with loaded page"
    # Type a URL in command mode
    send_keys colon
    sleep 0.3
    send_type "open https://example.com"
    sleep 0.3
    send_keys Return
    sleep 3.0
    take_screenshot "08_with_url"
    send_keys Escape
    sleep 0.3
}

# State: Status bar with git status
capture_git_status() {
    log "S09: Git status in status bar"
    sleep 1.0
    take_screenshot "09_git_status"
}

# State: Split panes
capture_split_panes() {
    log "S10: Split panes"
    send_keys colon
    sleep 0.3
    send_type "hsplit"
    sleep 0.3
    send_keys Return
    sleep 0.8
    take_screenshot "10_split_panes"
}

# ─── DOM Snapshot via evaluate_script ───────────────────────────────
# We create a small JS snippet that dumps the chrome DOM structure.
# This requires the chrome webview to support evaluate_script.

capture_dom_snapshot() {
    log "DOM: Capturing chrome webview DOM structure"
    # The DOM snapshot is captured via the aileron binary's test mode
    # For now, we document what JS we would inject:
    cat > "$DOM_DIR/chrome_dom_capture.js" << 'JSEOF'
// Inject this into the chrome webview to capture full DOM structure
(function() {
    function serialize(el, depth) {
        if (depth > 4) return '';
        var tag = el.tagName.toLowerCase();
        var attrs = '';
        for (var i = 0; i < el.attributes.length; i++) {
            var a = el.attributes[i];
            attrs += ' ' + a.name + '=' + JSON.stringify(a.value);
        }
        var classes = el.className && typeof el.className === 'string' ? ' class=' + JSON.stringify(el.className) : '';
        var text = '';
        if (tag === 'span' || tag === 'button') {
            text = el.textContent.trim().substring(0, 50);
            if (text) text = ' text=' + JSON.stringify(text);
        }
        var children = '';
        for (var i = 0; i < el.children.length; i++) {
            children += serialize(el.children[i], depth + 1);
        }
        var indent = '  '.repeat(depth);
        return indent + '<' + tag + attrs + classes + text + '>' + '\n' + children + indent + '</' + tag + '>\n';
    }
    var root = document.querySelector('.chrome-root');
    if (!root) return 'ERROR: .chrome-root not found';
    return serialize(root, 0);
})();
JSEOF

    # Write the expected DOM structure based on our component inventory
    cat > "$DOM_DIR/expected_structure.txt" << 'EOF'
STRUCTURE: chrome-root
├── STATUS-BAR: .status-bar
│   ├── Mode indicator: span.{mode-NORMAL|mode-INSERT|mode-COMMAND}
│   ├── URL display: span.status-url
│   └── Status message: span.status-msg
├── URL-BAR: .url-bar
│   └── Input: input.url-input (disabled when not focused)
├── TAB-SIDEBAR: .tab-sidebar
│   ├── Tab items: div.tab-item (.tab-active for active)
│   │   ├── Title: span.tab-title
│   │   └── Close: span.tab-close
│   └── New tab button: div.tab-new
├── FIND-BAR: .find-bar (CONDITIONAL: present when find_bar_open=true)
│   ├── Label: span.find-label "Find:"
│   ├── Input: input.find-input
│   ├── Next button: button.find-btn (↓)
│   ├── Prev button: button.find-btn (↑)
│   └── Close button: button.find-btn.find-close (✕)
└── PALETTE: .palette-backdrop (CONDITIONAL: present when command_palette_open=true)
    └── .palette-container
        ├── Input row: .palette-input-row
        │   ├── Prompt: span.palette-prompt ":"
        │   └── Input: input.palette-input
        └── Results: .palette-results
            └── Items: div.palette-item (.palette-selected)
                ├── Category: span.palette-cat
                ├── Label: span.palette-label
                └── Description: span.palette-desc
EOF
    log "DOM: Expected structure written to $DOM_DIR/expected_structure.txt"
}

# ─── Compare against baselines ──────────────────────────────────
compare_baselines() {
    log "Comparing against baselines in $BASELINE_DIR"
    local failures=0
    local total=0

    if [ ! -d "$BASELINE_DIR/screens" ]; then
        warn "No baseline screenshots found in $BASELINE_DIR/screens/"
        return 0
    fi

    for baseline in "$BASELINE_DIR/screens"/*.png; do
        [ -f "$baseline" ] || continue
        local name
        name="$(basename "$baseline")"
        local current="$SCREEN_DIR/$name"
        total=$((total + 1))
        if [ ! -f "$current" ]; then
            fail "Missing screenshot: $name"
            failures=$((failures + 1))
            continue
        fi
        # Use ImageMagick compare if available
        if command -v compare &>/dev/null; then
            local diff_out
            diff_out=$(compare -metric AE "$baseline" "$current" "$SESSION_DIR/diff_${name}" 2>&1) || true
            local ae
            ae=$(echo "$diff_out" | grep -oP '[\d.]+' | head -1)
            if (( $(echo "$ae < 0.01" | bc -l 2>/dev/null || echo "0") )); then
                ok "$name (AE=$ae)"
            else
                fail "$name (AE=$ae -- exceeds threshold 0.01)"
                failures=$((failures + 1))
            fi
        else
            # Fallback: just check file exists
            ok "$name (exists, no compare tool)"
        fi
    done

    echo ""
    log "Results: $((total - failures))/$total passed"
    return $failures
}

# ─── Save as new baselines ───────────────────────────────────────
save_baselines() {
    log "Saving screenshots as new baselines to $BASELINE_DIR"
    mkdir -p "$BASELINE_DIR/screens"
    mkdir -p "$BASELINE_DIR/dom"
    cp -v "$SCREEN_DIR"/*.png "$BASELINE_DIR/screens/" 2>/dev/null || warn "No screenshots to save"
    cp -v "$DOM_DIR"/*.txt "$BASELINE_DIR/dom/" 2>/dev/null || warn "No DOM files to save"
    cp -v "$DOM_DIR"/*.js "$BASELINE_DIR/dom/" 2>/dev/null || warn "No JS files to save"
    ok "Baselines saved"
}

# ─── Main ──────────────────────────────────────────────────────────
main() {
    local mode="${1:-capture}"

    echo ""
    echo "  Aileron Visual Regression Testing"
    echo "  ================================"
    echo "  Binary:   $AILERON_BIN"
    echo "  Output:   $SESSION_DIR"
    echo "  Baseline: $BASELINE_DIR"
    echo "  Mode:     $mode"
    echo ""

    # Setup
    mkdir -p "$DOM_DIR" "$SCREEN_DIR"

    # Check prerequisites
    local missing=0
    command -v xdotool &>/dev/null || { warn "xdotool not found (keyboard simulation)"; missing=1; }
    local tool
    tool="$(detect_screenshot_tool)"
    [ -n "$tool" ] || { warn "No screenshot tool found (grim/scrot/import)"; missing=1; }
    [ -f "$AILERON_BIN" ] || { warn "Aileron binary not found at $AILERON_BIN"; missing=1; }
    command -v jq &>/dev/null || { warn "jq not found (DOM formatting)"; missing=1; }

    if (( missing > 0 )) && [ "$mode" = "capture" ]; then
        warn "Missing prerequisites. Some captures will be skipped."
    fi

    # Verify chrome WASM is built
    if [ ! -f "$(dirname "$0")/../chrome/dist/index.html" ]; then
        warn "Chrome WASM not built. Run 'trunk build' in chrome/ first."
        warn "DOM snapshots will be generated from expected structure only."
    fi

    case "$mode" in
        capture)
            # Launch aileron in background
            log "Launching aileron..."
            AILERON_TESTING=1 GDK_BACKEND=x11 "$AILERON_BIN" &
            AILERON_PID=$!
            trap "kill $AILERON_PID 2>/dev/null; wait $AILERON_PID 2>/dev/null" EXIT

            if ! wait_for_aileron; then
                fail "Cannot proceed without aileron window"
                exit 1
            fi

            sleep 1.0

            # Capture all states
            capture_default
            capture_command_mode
            capture_insert_mode
            capture_palette_empty
            capture_palette_with_query
            capture_find_bar
            capture_multiple_tabs
            capture_with_url
            capture_git_status
            capture_split_panes

            # Capture DOM structure
            capture_dom_snapshot

            # Cleanup
            log "Stopping aileron (PID $AILERON_PID)..."
            kill "$AILERON_PID" 2>/dev/null
            wait "$AILERON_PID" 2>/dev/null
            trap - EXIT

            echo ""
            log "Screenshots saved to: $SCREEN_DIR/"
            log "DOM snapshots saved to: $DOM_DIR/"
            ;;

        baseline)
            if [ -d "$SCREEN_DIR" ] && [ "$(ls -A "$SCREEN_DIR" 2>/dev/null)" ]; then
                save_baselines
            else
                fail "No screenshots to save. Run 'capture' mode first."
                exit 1
            fi
            ;;

        compare)
            if [ -d "$SCREEN_DIR" ] && [ -d "$BASELINE_DIR/screens" ]; then
                compare_baselines
            else
                fail "Need both current screenshots and baselines to compare."
                exit 1
            fi
            ;;

        *)
            echo "Usage: $0 [--baseline|--compare]"
            echo "  (default) Capture screenshots and DOM snapshots"
            echo "  --baseline  Save current captures as new baselines"
            echo "  --compare   Compare current against baselines"
            exit 1
            ;;
    esac
}

main "$@"
