//! Agent-Computer Interface core (YP-AGENT-ACI-001, Era-1 S8–S12).
//!
//! In-page walker + actuator programs. Design (see the YP):
//! - `browser_snapshot` injects [`walker_js`], which assigns epoch-stamped
//!   integer refs to interactive elements and caches them **in the page**
//!   (`window.__aileron_refs`) — the page owns its own address space.
//! - Actuation tools inject [`actuator_js`] programs that fail closed on
//!   epoch mismatch (`STALE_EPOCH`) or detached nodes (`DETACHED`), per the
//!   no-stale-actuation theorem sketch (ACT-ACI-001).
//! - Observation tiers: T1 = interactive+named only; T2 = all interactive.

/// Epoch value the walker stamps refs with (from `McpState.active_epoch`).
#[must_use]
pub fn walker_js(epoch: u64, tier: &str, max_refs: usize) -> String {
    // T2 keeps unnamed interactive elements; T1 requires a usable name.
    let name_filter = if tier.eq_ignore_ascii_case("T1") {
        "if (!nm) return;"
    } else {
        ""
    };
    format!(
        r#"(() => {{
  const EPOCH = {epoch};
  const MAXREFS = {max_refs};
  const INTERACTIVE = 'a,button,input,select,textarea,summary,option,[role],[contenteditable="true"],[onclick],[tabindex]';
  const ROLE_MAP = {{A:'link',BUTTON:'button',SELECT:'combobox',TEXTAREA:'textbox',SUMMARY:'button',OPTION:'option',IMG:'img',LABEL:'label',H1:'heading',H2:'heading',H3:'heading',H4:'heading',H5:'heading',H6:'heading'}};
  const INPUT_ROLE = {{text:'textbox',email:'textbox',password:'textbox',search:'textbox',tel:'textbox',url:'textbox',number:'spinbutton',checkbox:'checkbox',radio:'radio',range:'slider',file:'button',color:'textbox',date:'textbox'}};
  const visible = (el) => {{
    const s = getComputedStyle(el);
    if (s.display === 'none' || s.visibility === 'hidden' || s.pointerEvents === 'none') return false;
    const r = el.getBoundingClientRect();
    return r.width > 0 && r.height > 0;
  }};
  const nameOf = (el) => (el.getAttribute('aria-label') || (el.innerText || '').trim().split('\n')[0] || el.value || el.getAttribute('placeholder') || el.getAttribute('alt') || el.getAttribute('title') || el.getAttribute('name') || '').toString().trim().slice(0, 80);
  const roleOf = (el) => {{
    const r = el.getAttribute('role');
    if (r) return r;
    if (el.tagName === 'INPUT') return INPUT_ROLE[el.type] || 'textbox';
    return ROLE_MAP[el.tagName] || '';
  }};
  window.__aileron_refs = {{}};
  window.__aileron_epoch = EPOCH;
  const out = [];
  let k = 0;
  document.querySelectorAll(INTERACTIVE).forEach((el) => {{
    if (k >= MAXREFS) return;
    if (!el.isConnected || !visible(el)) return;
    const role = roleOf(el);
    if (!role) return;
    const nm = nameOf(el);
    {name_filter}
    window.__aileron_refs[k] = el;
    const st = [];
    if (el.tabIndex >= 0) st.push('focusable');
    if (el === document.activeElement) st.push('focused');
    if (el.checked === true) st.push('checked');
    if (el.required || el.getAttribute('aria-required') === 'true') st.push('required');
    if (el.disabled) st.push('disabled');
    const node = {{ r: k, e: EPOCH, role, name: nm, tag: el.tagName.toLowerCase() }};
    if (el.value && el.tagName !== 'OPTION') node.v = String(el.value).slice(0, 40);
    if (st.length) node.st = st;
    if (el.tagName === 'A' && el.href) node.href = String(el.href).slice(0, 120);
    if (role === 'heading' && el.getAttribute('aria-level')) node.level = el.getAttribute('aria-level');
    out.push(node);
    k++;
  }});
  return JSON.stringify({{ epoch: EPOCH, url: location.href, n: out.length, nodes: out }});
}})()"#
    )
    .replace("{name_filter}", name_filter)
}

/// Fail-closed prologue shared by every actuator: verifies the page-side epoch
/// and ref liveness, returns `null` when valid, otherwise the error JSON.
const ACT_GUARD: &str = r#"(() => {
  const E = __EPOCH__, R = __REF__;
  if (window.__aileron_epoch !== E) return JSON.stringify({ok:false, err:'STALE_EPOCH', snapshot_epoch: window.__aileron_epoch ?? 0});
  const el = (window.__aileron_refs || {})[R];
  if (!el || !el.isConnected) return JSON.stringify({ok:false, err:'DETACHED'});
"#;

/// `click` / `hover` / `focus` actuator.
#[must_use]
pub fn actuator_js(kind: &str, epoch: u64, ref_idx: u64) -> String {
    let action = match kind {
        "click" => "el.scrollIntoView({block:'center'}); el.click();",
        "hover" => {
            "el.scrollIntoView({block:'center'}); \
             el.dispatchEvent(new MouseEvent('mouseover',{bubbles:true})); \
             el.dispatchEvent(new MouseEvent('mouseenter',{bubbles:true})); \
             el.dispatchEvent(new MouseEvent('mousemove',{bubbles:true}));"
        }
        "focus" => "el.scrollIntoView({block:'center'}); el.focus();",
        _ => return String::new(),
    };
    ACT_GUARD
        .replace("__EPOCH__", &epoch.to_string())
        .replace("__REF__", &ref_idx.to_string())
        + action
        + r#" return JSON.stringify({ok:true, epoch: E, ref: R}); })()"#
}

/// `type` actuator: focus, optionally clear, set the value through the native
/// setter, then fire `input` + `change` (framework-compatible).
#[must_use]
pub fn type_js(epoch: u64, ref_idx: u64, text: &str, clear: bool) -> String {
    let text_js = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
    let clear_js = if clear { "el.value = '';" } else { "" };
    let body = r#"el.scrollIntoView({block:'center'}); el.focus();
  if (el.disabled || el.readOnly) return JSON.stringify({ok:false, err:'READONLY'});
  __CLEAR__
  const proto = el.tagName === 'TEXTAREA' ? HTMLTextAreaElement.prototype : (el.isContentEditable ? null : HTMLInputElement.prototype);
  if (proto) { const set = Object.getOwnPropertyDescriptor(proto, 'value').set; set.call(el, __TEXT__); }
  else { el.textContent = __TEXT__; }
  el.dispatchEvent(new Event('input', {bubbles:true}));
  el.dispatchEvent(new Event('change', {bubbles:true}));
  return JSON.stringify({ok:true, epoch: E, ref: R, v: String(el.value ?? '').slice(0,40)}); })()"#
        .replace("__CLEAR__", clear_js)
        .replace("__TEXT__", &text_js);
    let mut js = ACT_GUARD
        .replace("__EPOCH__", &epoch.to_string())
        .replace("__REF__", &ref_idx.to_string());
    js.push_str(&body);
    js
}

/// `press_key` actuator: keydown/keyup on the focused element.
#[must_use]
pub fn key_js(key: &str) -> String {
    let upper = key.to_ascii_uppercase();
    let (k, code) = match upper.as_str() {
        "ENTER" => ("Enter", 13),
        "TAB" => ("Tab", 9),
        "ESCAPE" | "ESC" => ("Escape", 27),
        "BACKSPACE" => ("Backspace", 8),
        "DELETE" => ("Delete", 46),
        "ARROWDOWN" | "DOWN" => ("ArrowDown", 40),
        "ARROWUP" | "UP" => ("ArrowUp", 38),
        "ARROWLEFT" | "LEFT" => ("ArrowLeft", 37),
        "ARROWRIGHT" | "RIGHT" => ("ArrowRight", 39),
        "SPACE" => (" ", 32),
        "HOME" => ("Home", 36),
        "END" => ("End", 35),
        "PAGEUP" => ("PageUp", 33),
        "PAGEDOWN" => ("PageDown", 34),
        other if other.len() == 1 => (other, other.as_bytes()[0] as i32),
        _ => (key, 0),
    };
    r#"(() => {
  const t = document.activeElement || document.body;
  const opts = {key: __K__, keyCode: __C__, which: __C__, bubbles: true, cancelable: true};
  t.dispatchEvent(new KeyboardEvent('keydown', opts));
  t.dispatchEvent(new KeyboardEvent('keyup', opts));
  return JSON.stringify({ok:true, key: __K__}); })()"#
        .replace(
            "__K__",
            &serde_json::to_string(k).unwrap_or_else(|_| "\"\"".into()),
        )
        .replace("__C__", &code.to_string())
}

/// `submit` actuator: Enter keydown + native form submission (synthetic
/// KeyboardEvents are untrusted and do not trigger default form submit, so
/// the form is submitted explicitly via `requestSubmit`).
#[must_use]
pub fn submit_js() -> String {
    r#"(() => {
  const t = document.activeElement || document.body;
  const opts = {key: 'Enter', keyCode: 13, which: 13, bubbles: true, cancelable: true};
  t.dispatchEvent(new KeyboardEvent('keydown', opts));
  t.dispatchEvent(new KeyboardEvent('keyup', opts));
  const f = t.closest && t.closest('form');
  if (f) { if (f.requestSubmit) { f.requestSubmit(); } else { f.submit(); } }
  return JSON.stringify({ok:true, action:'enter', form: !!f});
})()"#
        .to_string()
}

/// `select_option` actuator for `<select multiple?>` elements.
#[must_use]
pub fn select_js(epoch: u64, ref_idx: u64, values: &[String]) -> String {
    let vals: Vec<String> = values
        .iter()
        .map(|v| serde_json::to_string(v).unwrap_or_else(|_| "\"\"".into()))
        .collect();
    let body = r#"if (el.tagName !== 'SELECT') return JSON.stringify({ok:false, err:'NOT_SELECT'});
  const want = new Set([__VALS__]);
  let picked = 0;
  for (const o of el.options) {
    const hit = want.has(o.value) || want.has(o.text);
    o.selected = hit;
    if (hit) picked++;
  }
  el.dispatchEvent(new Event('input', {bubbles:true}));
  el.dispatchEvent(new Event('change', {bubbles:true}));
  return JSON.stringify({ok:true, picked}); })()"#
        .replace("__VALS__", &vals.join(","));
    let mut js = ACT_GUARD
        .replace("__EPOCH__", &epoch.to_string())
        .replace("__REF__", &ref_idx.to_string());
    js.push_str(&body);
    js
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walker_embeds_epoch_and_tier_filter() {
        let t1 = walker_js(7, "T1", 150);
        assert!(t1.contains("const EPOCH = 7"));
        assert!(t1.contains("const MAXREFS = 150"));
        // T1 filters unnamed elements; T2 keeps them.
        let t2 = walker_js(7, "T2", 150);
        assert!(!t2.contains("if (!nm) return;"));
        let t1b = walker_js(7, "T1", 150);
        assert!(t1b.contains("if (!nm) return;"));
    }

    #[test]
    fn actuator_guards_embed_epoch_and_ref() {
        let js = actuator_js("click", 42, 9);
        assert!(js.contains("const E = 42, R = 9;"));
        assert!(js.contains("STALE_EPOCH"));
        assert!(js.contains("DETACHED"));
        assert!(js.contains("el.click()"));
        assert!(actuator_js("bogus", 1, 1).is_empty());
    }

    #[test]
    fn type_js_json_escapes_text() {
        let js = type_js(1, 2, "he said \"hi\"\nline2", true);
        assert!(js.contains(r#"he said \"hi\"\nline2"#));
        assert!(js.contains("el.value = '';")); // clear
        let js2 = type_js(1, 2, "x", false);
        assert!(!js2.contains("el.value = '';"));
    }

    #[test]
    fn key_js_maps_common_keys() {
        assert!(key_js("ENTER").contains("keyCode: 13"));
        assert!(key_js("TAB").contains("keyCode: 9"));
        assert!(key_js("ESCAPE").contains("keyCode: 27"));
        assert!(key_js("a").contains("keyCode: 65")); // uppercased 'A' = 65
        assert!(key_js("ArrowDown").contains("keyCode: 40"));
    }

    #[test]
    fn select_js_carries_values() {
        let js = select_js(3, 5, &["US".into(), "\"quoted\"".into()]);
        assert!(js.contains("\"US\""));
        assert!(js.contains("\\\"quoted\\\""));
        assert!(js.contains("NOT_SELECT"));
    }

    #[test]
    fn submit_js_requests_form_submit() {
        assert!(submit_js().contains("requestSubmit"));
        assert!(submit_js().contains("KeyboardEvent"));
    }
}
