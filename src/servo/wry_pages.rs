//! HTML page generators and JavaScript constants for aileron:// internal pages.

/// JavaScript that intercepts network requests for monitoring.
/// Stores captured requests in window._aileron_requests[].
pub const NETWORK_MONITOR_JS: &str = r#"
(function() {
    if (window._aileron_network_monitor) return;
    window._aileron_network_monitor = true;
    window._aileron_requests = [];
    
    var origFetch = window.fetch;
    window.fetch = function() {
        var url = arguments[0] instanceof Request ? arguments[0].url : String(arguments[0]);
        var method = arguments[1] && arguments[1].method ? arguments[1].method : 'GET';
        var entry = { url: url, method: method, type: 'fetch', time: new Date().toISOString(), status: null };
        window._aileron_requests.push(entry);
        return origFetch.apply(this, arguments).then(function(resp) {
            entry.status = resp.status;
            return resp;
        }).catch(function(err) {
            entry.status = 'ERR';
            throw err;
        });
    };
    
    var origOpen = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function(method, url) {
        this._aileron_entry = { url: String(url), method: method, type: 'xhr', time: new Date().toISOString(), status: null };
        window._aileron_requests.push(this._aileron_entry);
        return origOpen.apply(this, arguments);
    };
    var origSend = XMLHttpRequest.prototype.send;
    XMLHttpRequest.prototype.send = function() {
        var self = this;
        this.addEventListener('load', function() {
            if (self._aileron_entry) self._aileron_entry.status = self.status;
        });
        this.addEventListener('error', function() {
            if (self._aileron_entry) self._aileron_entry.status = 'ERR';
        });
        return origSend.apply(this, arguments);
    };
})();
"#;

pub const NETWORK_LOG_JS: &str = r#"
JSON.stringify(window._aileron_requests || [])
"#;

pub const NETWORK_CLEAR_JS: &str = r#"
window._aileron_requests = [];
'Network log cleared';
"#;

pub const CONSOLE_CAPTURE_JS: &str = r#"
(function() {
    if (window._aileron_console_capture) return;
    window._aileron_console_capture = true;
    window._aileron_console = [];
    
    ['log', 'warn', 'error', 'info'].forEach(function(level) {
        var orig = console[level];
        console[level] = function() {
            var msg = Array.prototype.slice.call(arguments).map(function(a) {
                try { return typeof a === 'object' ? JSON.stringify(a).slice(0, 200) : String(a); }
                catch(e) { return String(a); }
            }).join(' ');
            window._aileron_console.push({ level: level, msg: msg, time: new Date().toISOString() });
            if (window._aileron_console.length > 200) window._aileron_console.shift();
            return orig.apply(console, arguments);
        };
    });
})();
"#;

pub const CONSOLE_LOG_JS: &str = r#"
JSON.stringify(window._aileron_console || [])
"#;

/// JavaScript that monitors for navigation errors and stores them
/// for detection after page load completes.
pub const ERROR_MONITOR_JS: &str = r#"
(function() {
    if (window._aileron_error_monitor) return;
    window._aileron_error_monitor = true;

    // Send a navigation error report to Aileron via IPC.
    // Uses window.location.href for the URL since it's the failed destination.
    function reportNavError(message) {
        try {
            window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke('__aileron_ipc', {
                message: '__aileron_nav_error__|' + (window.location.href || '') + '|' + message
            });
        } catch(e) {
            // Fallback: wry IPC via postMessage to aileron:// scheme
            try { window.postMessage('__aileron_nav_error__|' + (window.location.href || '') + '|' + message, '*'); } catch(e2) {}
        }
    }

    // Check on load complete whether the page looks like a WebKitGTK error page.
    // WebKitGTK error pages have titles like "Problem loading page" or contain
    // short error messages in the body with no real content.
    function checkForErrorPage() {
        var title = (document.title || '').toLowerCase();
        var isLikelyError = false;
        var errorMsg = '';

        // WebKitGTK error page indicators
        if (title.indexOf('problem loading') !== -1
            || title.indexOf('unable to connect') !== -1
            || title.indexOf('could not connect') !== -1
            || title.indexOf('network error') !== -1
            || title.indexOf('connection refused') !== -1
            || title.indexOf('ssl') !== -1 && title.indexOf('error') !== -1
            || title.indexOf('certificate') !== -1
            || title.indexOf('not found') !== -1
            || title.indexOf('server not found') !== -1
            || title.indexOf('host not found') !== -1
            || title.indexOf('timed out') !== -1
            || title.indexOf('unauthorized') !== -1
            || title.indexOf('forbidden') !== -1) {
            isLikelyError = true;
            errorMsg = document.title || title;
        }

        // Also check for very short pages with error-like content
        if (!isLikelyError) {
            var body = document.body ? document.body.innerText : '';
            if (body.length < 300 && body.length > 0) {
                var bodyLower = body.toLowerCase();
                if (bodyLower.indexOf('error') !== -1
                    || bodyLower.indexOf('could not') !== -1
                    || bodyLower.indexOf('failed to') !== -1
                    || bodyLower.indexOf('unable to') !== -1) {
                    isLikelyError = true;
                    errorMsg = body.substring(0, 200).trim();
                }
            }
        }

        // Also detect blank/empty pages that aren't our own pages
        if (!isLikelyError && document.body) {
            var html = document.body.innerHTML.trim();
            if (html.length === 0 && window.location.protocol !== 'aileron:') {
                isLikelyError = true;
                errorMsg = 'Empty page — possible DNS or connection failure';
            }
        }

        if (isLikelyError) {
            reportNavError(errorMsg);
        }
    }

    // Run check after DOM is ready and also after full load
    if (document.readyState === 'complete' || document.readyState === 'interactive') {
        setTimeout(checkForErrorPage, 100);
    }
    window.addEventListener('load', function() {
        setTimeout(checkForErrorPage, 100);
    });

    // Also monitor for runtime errors during page lifecycle
    window.addEventListener('error', function(e) {
        // Only report if it looks like a network/resource error, not a JS bug
        var target = e.target || {};
        if (target.tagName === 'IMG' || target.tagName === 'LINK' || target.tagName === 'SCRIPT') {
            // Resource loading failure — don't report individual resource errors
            return;
        }
        var msg = (e.message || 'Unknown error').toString();
        if (msg.indexOf('net::') !== -1
            || msg.indexOf('ERR_') !== -1
            || msg.indexOf('NetworkError') !== -1
            || msg.indexOf('Failed to fetch') !== -1) {
            reportNavError(msg);
        }
    }, true);
})();
"#;

pub const CONSOLE_CLEAR_JS: &str = r#"
window._aileron_console = [];
'Console cleared';
"#;

/// JavaScript that saves the current scroll position before navigation.
/// Called before back/forward navigation.
pub const SCROLL_SAVE_JS: &str = r#"
(function() {
    window._aileron_scroll_pos = {
        x: window.scrollX || document.documentElement.scrollLeft || 0,
        y: window.scrollY || document.documentElement.scrollTop || 0
    };
})();
"#;

/// JavaScript that restores the saved scroll position after navigation.
pub const SCROLL_RESTORE_JS: &str = r#"
(function() {
    if (window._aileron_scroll_pos) {
        window.scrollTo(window._aileron_scroll_pos.x, window._aileron_scroll_pos.y);
    }
})();
"#;

// ─── Internal page HTML generators ───────────────────────────────────

/// Reader mode page shown at `aileron://reader`.
/// Extracts article content from the current page and renders a clean reading view.
pub(crate) fn aileron_reader_page() -> String {
    r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Reader Mode</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #1a1a1a; color: #d4d4d4; font-family: Georgia, 'Times New Roman', serif;
         max-width: 680px; margin: 0 auto; padding: 40px 20px; line-height: 1.8; }
  h1 { color: #e0e0e0; font-size: 1.8em; margin-bottom: 0.3em; }
  .meta { color: #666; font-size: 0.9em; margin-bottom: 2em; }
  .meta a { color: #4db4ff; text-decoration: none; }
  h2, h3, h4 { color: #ccc; margin-top: 1.5em; margin-bottom: 0.5em; }
  p { margin-bottom: 1em; }
  a { color: #4db4ff; }
  pre { background: #2a2a2a; padding: 12px; border-radius: 4px; overflow-x: auto;
       font-family: 'SF Mono', 'Fira Code', monospace; font-size: 0.9em; margin: 1em 0; }
  blockquote { border-left: 3px solid #4db4ff; padding-left: 16px; color: #999; margin: 1em 0; }
  img { max-width: 100%; height: auto; margin: 1em 0; border-radius: 4px; }
  .loading { text-align: center; padding: 4em; color: #666; }
  .error { text-align: center; padding: 2em; color: #ff6b6b; }
  .controls { position: fixed; top: 12px; right: 12px; }
  .controls a { color: #666; text-decoration: none; font-size: 0.85em; margin-left: 12px; font-family: sans-serif; }
  .controls a:hover { color: #4db4ff; }
  .font-size-controls { position: fixed; bottom: 12px; right: 12px; }
  .font-size-controls button {
    background: #2a2a2a; color: #888; border: 1px solid #333; padding: 4px 10px;
    border-radius: 4px; cursor: pointer; font-family: sans-serif;
  }
  .font-size-controls button:hover { color: #4db4ff; border-color: #4db4ff; }
</style>
</head>
<body>
<div id="reader-content">
  <div class="loading">Extracting article content...</div>
</div>
<div class="controls">
  <a href="#" id="original-link" title="View original page">Original</a>
</div>
<div class="font-size-controls">
  <button id="font-decrease" title="Decrease font size">A-</button>
  <button id="font-increase" title="Increase font size">A+</button>
</div>
<script>
(function() {
  var originalUrl = '';

  function extractArticle() {
    var title = document.title || '';
    var author = '';
    var published = '';

    var authorMeta = document.querySelector('meta[name="author"]')
      || document.querySelector('meta[property="article:author"]');
    if (authorMeta) author = authorMeta.getAttribute('content') || '';

    var dateMeta = document.querySelector('meta[name="date"]')
      || document.querySelector('meta[property="article:published_time"]')
      || document.querySelector('meta[name="DC.date.issued"]');
    if (dateMeta) published = dateMeta.getAttribute('content') || '';

    var descMeta = document.querySelector('meta[name="description"]');
    var description = descMeta ? descMeta.getAttribute('content') : '';

    var candidates = [
      'article', '[role="main"]', 'main',
      '.post-content', '.article-content', '.content', '#content',
      '.entry-content', '.post-body', '.story-body',
      '.article-body', '.story-content', '.main-content',
      '[data-article-body]', '.rich-text'
    ];

    var article = null;
    for (var i = 0; i < candidates.length; i++) {
      article = document.querySelector(candidates[i]);
      if (article) break;
    }

    if (!article) {
      var divs = document.querySelectorAll('div');
      var best = null;
      var bestScore = 0;
      for (var j = 0; j < divs.length; j++) {
        var d = divs[j];
        var text = d.textContent || '';
        var pCount = d.querySelectorAll('p').length;
        if (pCount >= 3 && text.length > bestScore) {
          bestScore = text.length;
          best = d;
        }
      }
      article = best || document.body;
    }

    var clone = article.cloneNode(true);

    var removeSelectors = [
      'nav', 'header', 'footer', 'aside', '.sidebar', '#sidebar',
      '.comments', '#comments', '.comment', '.ad', '.advertisement',
      '.social-share', '.share-buttons', '.related', '.recommendations',
      'script', 'style', 'noscript', 'iframe', '.newsletter',
      '.popup', '.modal', '[role="navigation"]', '[role="banner"]',
      '.breadcrumb', '.pagination', '.widget', '.promo'
    ];

    for (var k = 0; k < removeSelectors.length; k++) {
      var els = clone.querySelectorAll(removeSelectors[k]);
      for (var l = 0; l < els.length; l++) {
        els[l].parentNode.removeChild(els[l]);
      }
    }

    var text = '';
    var blocks = clone.querySelectorAll('p, h1, h2, h3, h4, h5, h6, li, pre, blockquote, img, table');
    if (blocks.length > 3) {
      for (var m = 0; m < blocks.length; m++) {
        var block = blocks[m];
        var tag = block.tagName.toLowerCase();
        if (tag === 'p') {
          text += '<p>' + block.innerHTML.trim() + '</p>';
        } else if (tag.match(/^h[1-6]$/)) {
          text += '<' + tag + '>' + block.textContent.trim() + '</' + tag + '>';
        } else if (tag === 'li') {
          text += '<li>' + block.innerHTML.trim() + '</li>';
        } else if (tag === 'pre') {
          text += '<pre>' + block.textContent.trim() + '</pre>';
        } else if (tag === 'blockquote') {
          text += '<blockquote>' + block.innerHTML.trim() + '</blockquote>';
        } else if (tag === 'img') {
          var src = block.getAttribute('src') || '';
          var alt = block.getAttribute('alt') || '';
          if (src) text += '<img src="' + src + '" alt="' + alt + '">';
        } else if (tag === 'table') {
          text += block.outerHTML;
        }
      }
    } else {
      text = clone.innerHTML.trim();
    }

    var metaHtml = '';
    if (author || published || description) {
      metaHtml = '<div class="meta">';
      if (author) metaHtml += author;
      if (published) metaHtml += (author ? ' &middot; ' : '') + published;
      if (description && !author && !published) metaHtml += description;
      metaHtml += '</div>';
    }

    var html = '<h1>' + title + '</h1>' + metaHtml + text;

    document.getElementById('reader-content').innerHTML = html;
    document.title = title + ' (Reader)';
  }

  try {
    extractArticle();
  } catch(e) {
    document.getElementById('reader-content').innerHTML =
      '<div class="error">Could not extract article content.</div>';
  }

  if (window._aileron_original_url) {
    originalUrl = window._aileron_original_url;
    var link = document.getElementById('original-link');
    if (link) link.href = originalUrl;
  }

  var baseSize = 18;
  document.getElementById('font-decrease').addEventListener('click', function() {
    baseSize = Math.max(12, baseSize - 2);
    document.body.style.fontSize = baseSize + 'px';
  });
  document.getElementById('font-increase').addEventListener('click', function() {
    baseSize = Math.min(28, baseSize + 2);
    document.body.style.fontSize = baseSize + 'px';
  });
})();
</script>
</body>
</html>"##.to_string()
}

/// Welcome page shown at `aileron://welcome` (default homepage).
pub(crate) fn aileron_welcome_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<title>Aileron</title>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #141414; color: #aaa; font-family: 'SF Mono', 'Fira Code', monospace;
         display: flex; align-items: center; justify-content: center; height: 100vh; }
  .container { text-align: center; max-width: 680px; padding: 2em; }
  h1 { color: #4db4ff; font-size: 2.5em; margin-bottom: 0.3em; letter-spacing: 0.05em; }
  .subtitle { color: #888; font-size: 1.1em; margin-bottom: 1.5em; }
  .section-title { color: #4db4ff; font-size: 0.85em; text-transform: uppercase;
                   letter-spacing: 0.1em; margin: 1em 0 0.4em; border-bottom: 1px solid #2a2a2a;
                   padding-bottom: 0.3em; }
  .keys { text-align: left; display: inline-block; background: #1a1a1a;
          border-radius: 8px; padding: 1.2em 1.8em; border: 1px solid #2a2a2a; }
  .key-row { display: flex; justify-content: space-between; padding: 0.25em 0; }
  .key-row kbd { color: #4db4ff; background: #222; padding: 2px 8px; border-radius: 3px;
                 font-family: inherit; font-size: 0.9em; border: 1px solid #333; }
  .key-row span { color: #888; }
  .footer { margin-top: 1.5em; color: #444; font-size: 0.85em; }
</style>
</head>
<body>
<div class="container" role="main">
  <h1>Aileron</h1>
  <p class="subtitle">Keyboard-Driven Web Environment</p>
  <div class="keys" role="list" aria-label="Keyboard shortcuts">
    <div class="section-title">Navigation</div>
    <div class="key-row"><span>Scroll down / up</span><kbd>j</kbd> / <kbd>k</kbd></div>
    <div class="key-row"><span>Scroll left / right</span><kbd>h</kbd> / <kbd>l</kbd></div>
    <div class="key-row"><span>Half page down / up</span><kbd>Ctrl+D</kbd> / <kbd>Ctrl+U</kbd></div>
    <div class="key-row"><span>Top of page</span><kbd>Ctrl+G</kbd></div>
    <div class="key-row"><span>Bottom of page</span><kbd>G</kbd></div>
    <div class="key-row"><span>Back / Forward</span><kbd>H</kbd> / <kbd>L</kbd></div>
    <div class="key-row"><span>Reload</span><kbd>r</kbd></div>

    <div class="section-title">Modes</div>
    <div class="key-row"><span>Enter Insert mode</span><kbd>i</kbd></div>
    <div class="key-row"><span>Return to Normal mode</span><kbd>Esc</kbd></div>
    <div class="key-row"><span>Command palette</span><kbd>:</kbd> / <kbd>Ctrl+P</kbd></div>
    <div class="key-row"><span>Open terminal</span><kbd>`</kbd></div>

    <div class="section-title">Tiling</div>
    <div class="key-row"><span>Split vertical</span><kbd>Ctrl+W</kbd></div>
    <div class="key-row"><span>Split horizontal</span><kbd>Ctrl+S</kbd></div>
    <div class="key-row"><span>Switch panes</span><kbd>Ctrl+H</kbd> / <kbd>J</kbd> / <kbd>K</kbd> / <kbd>L</kbd></div>
    <div class="key-row"><span>Resize panes</span><kbd>Ctrl+Alt+H</kbd> / <kbd>J</kbd> / <kbd>K</kbd> / <kbd>L</kbd></div>
    <div class="key-row"><span>Close pane</span><kbd>q</kbd></div>
    <div class="key-row"><span>New tab</span><kbd>Ctrl+T</kbd></div>
    <div class="key-row"><span>New window</span><kbd>Ctrl+N</kbd></div>

    <div class="section-title">Tools</div>
    <div class="key-row"><span>DevTools</span><kbd>F12</kbd></div>
    <div class="key-row"><span>Find in page</span><kbd>Ctrl+F</kbd></div>
    <div class="key-row"><span>Link hints</span><kbd>f</kbd></div>
    <div class="key-row"><span>Copy URL</span><kbd>y</kbd></div>
    <div class="key-row"><span>Reload</span><kbd>r</kbd></div>
    <div class="key-row"><span>Bookmark</span><kbd>Ctrl+B</kbd></div>
    <div class="key-row"><span>External browser</span><kbd>Ctrl+E</kbd></div>
    <div class="key-row"><span>Zoom in / out / reset</span><kbd>Ctrl+=</kbd> / <kbd>-</kbd> / <kbd>0</kbd></div>
    <div class="key-row"><span>Reader mode</span><kbd>Ctrl+Shift+R</kbd></div>
    <div class="key-row"><span>Minimal mode</span><kbd>Ctrl+Shift+M</kbd></div>
    <div class="key-row"><span>Network log</span><kbd>Ctrl+Shift+N</kbd></div>
    <div class="key-row"><span>Console log</span><kbd>Ctrl+Shift+J</kbd></div>
    <div class="key-row"><span>Detach pane to window</span><kbd>Ctrl+Shift+D</kbd></div>
    <div class="key-row"><span>Password manager</span><kbd>bw-unlock</kbd> / <kbd>bw-search</kbd></div>
    <div class="key-row"><span>Quickmark</span><kbd>:m</kbd><kbd>a</kbd> <kbd>url</kbd> / <kbd>:g</kbd><kbd>a</kbd></div>

    <div class="section-title">Commands (:palette)</div>
    <div class="key-row"><span>Shell command</span><kbd>:!</kbd> <kbd>cmd</kbd></div>
    <div class="key-row"><span>Print page</span><kbd>:print</kbd></div>
    <div class="key-row"><span>Mute / unmute</span><kbd>:mute</kbd> / <kbd>:unmute</kbd></div>
    <div class="key-row"><span>Theme</span><kbd>:theme</kbd> <kbd>name</kbd></div>
    <div class="key-row"><span>Site settings</span><kbd>:site-settings</kbd></div>
    <div class="key-row"><span>PDF export</span><kbd>:pdf</kbd></div>
    <div class="key-row"><span>Popups</span><kbd>:popups</kbd></div>
    <div class="key-row"><span>Cookies</span><kbd>:cookies</kbd></div>
    <div class="key-row"><span>File browser</span><kbd>files</kbd> in palette</div>
    <div class="key-row"><span>SSH connect</span><kbd>ssh</kbd> <kbd>host</kbd></div>

    <div class="section-title">Terminal</div>
    <div class="key-row"><span>Position cursor</span>click</div>
    <div class="key-row"><span>Select text</span>drag</div>
    <div class="key-row"><span>Clear selection</span>right-click</div>
    <div class="key-row"><span>Paste</span>middle-click</div>
  </div>
  <p class="footer">Type a URL in the command palette and press Enter to navigate. Use <kbd style="color:#4db4ff;background:#222;padding:2px 8px;border-radius:3px;border:1px solid #333">`</kbd> for a terminal, <kbd style="color:#4db4ff;background:#222;padding:2px 8px;border-radius:3px;border:1px solid #333">files</kbd> to browse, or <kbd style="color:#4db4ff;background:#222;padding:2px 8px;border-radius:3px;border:1px solid #333">ssh user@host</kbd> to connect remotely</p>
</div>
<div aria-live="polite" id="status-region"></div>
</body>
</html>"#.to_string()
}

/// New tab page shown at `aileron://new`.
pub(crate) fn aileron_new_tab_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<title>New Tab</title>
<meta charset="utf-8">
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #141414; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace;
         display: flex; flex-direction: column; align-items: center; padding-top: 8vh; }
  h1 { color: #4db4ff; font-size: 1.8em; margin-bottom: 0.8em; letter-spacing: 0.05em; }
  .search-box { display: flex; margin-bottom: 1.5em; }
  .search-box input {
    background: #1a1a1a; border: 1px solid #333; color: #e0e0e0; padding: 10px 16px;
    font-size: 14px; font-family: inherit; width: 400px; border-radius: 4px; outline: none;
  }
  .search-box input:focus { border-color: #4db4ff; }
  .section { max-width: 540px; width: 100%; margin-bottom: 1.5em; }
  .section-title { color: #666; font-size: 11px; text-transform: uppercase; letter-spacing: 0.1em;
                    margin-bottom: 8px; padding-left: 2px; }
  .links { display: flex; flex-wrap: wrap; gap: 8px; }
  .link {
    background: #1a1a1a; border: 1px solid #2a2a2a; border-radius: 6px; padding: 10px 14px;
    text-align: center; cursor: pointer; text-decoration: none; color: #e0e0e0;
    transition: border-color 0.15s; max-width: 120px; min-width: 80px;
  }
  .link:hover { border-color: #4db4ff; }
  .link:focus { outline: 2px solid #4db4ff; outline-offset: 2px; }
  .link .name { font-size: 11px; margin-top: 4px; color: #888; overflow: hidden;
                text-overflow: ellipsis; white-space: nowrap; }
  .link .icon { font-size: 16px; }
  .history-item {
    display: block; padding: 6px 10px; color: #aaa; text-decoration: none;
    border-radius: 4px; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
  }
  .history-item:hover { background: #1a1a1a; color: #e0e0e0; }
  .history-item .htitle { color: #ccc; }
  .history-item .hurl { color: #555; font-size: 10px; margin-left: 8px; }
  .hint { color: #444; font-size: 11px; margin-top: 1em; }
  .hint kbd { background: #1a1a1a; border: 1px solid #333; border-radius: 3px; padding: 1px 5px;
              font-family: inherit; font-size: 10px; color: #888; }
</style>
</head>
<body role="main">
<h1>Aileron</h1>
<div class="search-box">
  <label for="search" class="sr-only">Search</label>
  <input type="text" id="search" placeholder="Search or enter URL..." autofocus aria-label="Search or enter URL">
</div>
<div class="section" id="bookmarks-section" style="display:none">
  <div class="section-title">Bookmarks</div>
  <nav class="links" id="bookmarks-list" aria-label="Bookmarks"></nav>
</div>
<div class="section" id="shortcuts-section">
  <nav class="links" aria-label="Quick links">
    <a class="link" href="aileron://files" tabindex="0" aria-label="Files">
      <div class="icon">&#128193;</div>
      <div class="name">Files</div>
    </a>
    <a class="link" href="aileron://terminal" tabindex="0" aria-label="Terminal">
      <div class="icon">&#9000;</div>
      <div class="name">Terminal</div>
    </a>
    <a class="link" href="aileron://bookmarks" tabindex="0" aria-label="Bookmarks">
      <div class="icon">&#9733;</div>
      <div class="name">Bookmarks</div>
    </a>
    <a class="link" href="aileron://history" tabindex="0" aria-label="History">
      <div class="icon">&#128336;</div>
      <div class="name">History</div>
    </a>
  </nav>
</div>
<div class="section" id="history-section" style="display:none">
  <div class="section-title">Recent</div>
  <div id="history-list" aria-label="Recent history"></div>
</div>
<p class="hint"><kbd>Ctrl+P</kbd> commands &middot; <kbd>gt</kbd> switch tabs &middot; <kbd>gi</kbd> insert mode</p>
<script>
// Request bookmark/history data from Aileron via IPC
try {
    if (window.ipc) {
        window.ipc.postMessage(JSON.stringify({ t: 'get-newtab-data' }));
    }
} catch(e) {}

// Callback to populate data when Aileron responds
window._onNewTabData = function(data) {
    // Bookmarks
    if (data.bookmarks && data.bookmarks.length > 0) {
        var el = document.getElementById('bookmarks-section');
        el.style.display = 'block';
        var list = document.getElementById('bookmarks-list');
        data.bookmarks.forEach(function(b) {
            var a = document.createElement('a');
            a.className = 'link';
            a.href = b.url;
            a.title = b.title || b.url;
            a.tabIndex = 0;
            var initial = (b.title || b.url || '?')[0].toUpperCase();
            a.innerHTML = '<div class="icon">' + initial + '</div><div class="name">' +
                (b.title || b.url).substring(0, 16) + '</div>';
            list.appendChild(a);
        });
    }
    // History
    if (data.history && data.history.length > 0) {
        var el = document.getElementById('history-section');
        el.style.display = 'block';
        var list = document.getElementById('history-list');
        data.history.forEach(function(h) {
            var a = document.createElement('a');
            a.className = 'history-item';
            a.href = h.url;
            a.title = h.title + ' — ' + h.url;
            var host = '';
            try { host = new URL(h.url).hostname; } catch(e) {}
            a.innerHTML = '<span class="htitle">' + (h.title || h.url) + '</span>' +
                '<span class="hurl">' + host + '</span>';
            list.appendChild(a);
        });
    }
};

// Search / URL navigation
document.getElementById('search').addEventListener('keydown', function(e) {
  if (e.key === 'Enter') {
    var q = this.value.trim();
    if (!q) return;
    if (q.indexOf('://') !== -1 || (q.indexOf('.') !== -1 && q.indexOf(' ') === -1)) {
      window.location.href = q.indexOf('://') !== -1 ? q : 'https://' + q;
    } else {
      window.location.href = 'https://duckduckgo.com/?q=' + encodeURIComponent(q);
    }
  }
});
</script>
</body>
</html>"#.to_string()
}

pub fn percent_encode_path(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric()
            || b == b'/'
            || b == b'-'
            || b == b'_'
            || b == b'.'
            || b == b'~'
        {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub(crate) fn percent_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(&String::from_utf8_lossy(&bytes[i + 1..i + 3]), 16)
        {
            result.push(byte);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_else(|_| s.to_string())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    match bytes {
        0..1024 => format!("{bytes} B"),
        n if n < MB => format!("{:.1} KB", n as f64 / KB as f64),
        n if n < GB => format!("{:.1} MB", n as f64 / MB as f64),
        n => format!("{:.1} GB", n as f64 / GB as f64),
    }
}

pub(crate) fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn format_modified_time(meta: &std::fs::Metadata) -> String {
    match meta.modified() {
        Ok(time) => {
            let datetime: chrono::DateTime<chrono::Local> = time.into();
            datetime.format("%Y-%m-%d %H:%M").to_string()
        }
        Err(_) => "-".to_string(),
    }
}

fn file_browser_error_page(path: &str, error: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Files: Error</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #141414; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace; padding: 16px; }}
  .error {{ color: #ff6b6b; padding: 20px; }}
  .path {{ color: #888; margin-bottom: 12px; font-size: 14px; }}
  .breadcrumb a {{ color: #4db4ff; text-decoration: none; }}
  .breadcrumb a:hover {{ text-decoration: underline; }}
  a {{ color: #4db4ff; text-decoration: none; }}
</style>
</head>
<body>
<div class="path">{}</div>
<div class="error">Error: {}</div>
<p style="margin-top:16px"><a href="aileron://files">Go to home directory</a></p>
</body>
</html>"#,
        html_escape(path),
        html_escape(error)
    )
}

pub(crate) fn file_browser_page(uri: &wry::http::Uri) -> String {
    use std::path::Path;

    let dir_path = uri
        .query()
        .and_then(|q| {
            q.split('&')
                .find(|pair| pair.starts_with("path="))
                .map(|pair| percent_decode(&pair[5..]))
        })
        .unwrap_or_else(|| {
            directories::UserDirs::new()
                .map(|d| d.home_dir().to_path_buf())
                .unwrap_or_else(|| std::path::PathBuf::from("/"))
                .to_string_lossy()
                .to_string()
        });

    let path = Path::new(&dir_path);

    let entries = match std::fs::read_dir(path) {
        Ok(rd) => rd.filter_map(|e| e.ok()).collect::<Vec<_>>(),
        Err(e) => return file_browser_error_page(&dir_path, &e.to_string()),
    };

    let mut dirs: Vec<(String, &std::fs::DirEntry)> = Vec::new();
    let mut files: Vec<(String, &std::fs::DirEntry)> = Vec::new();

    for entry in &entries {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if entry.path().is_dir() {
            dirs.push((name, entry));
        } else {
            files.push((name, entry));
        }
    }

    dirs.sort_by_key(|a| a.0.to_lowercase());
    files.sort_by_key(|a| a.0.to_lowercase());

    let mut breadcrumb_parts = Vec::new();
    if dir_path == "/" {
        breadcrumb_parts.push("<a href=\"aileron://files?path=%2F\">/</a>".to_string());
    } else {
        breadcrumb_parts.push(
            "<a href=\"aileron://files?path=%2F\">/</a><span class=\"sep\">/</span>".to_string(),
        );
        let segments: Vec<&str> = dir_path.trim_start_matches('/').split('/').collect();
        let mut accumulated = String::new();
        for (i, seg) in segments.iter().enumerate() {
            accumulated.push_str(seg);
            let encoded = percent_encode_path(&format!("/{accumulated}"));
            if i < segments.len() - 1 {
                breadcrumb_parts.push(format!(
                    "<a href=\"aileron://files?path={}\">{}</a><span class=\"sep\">/</span>",
                    encoded,
                    html_escape(seg)
                ));
            } else {
                breadcrumb_parts.push(format!("<span>{}</span>", html_escape(seg)));
            }
            accumulated.push('/');
        }
    }
    let breadcrumb_html = breadcrumb_parts.join("");

    let parent_url = if dir_path == "/" {
        String::new()
    } else {
        let parent = path.parent().unwrap_or(Path::new("/"));
        let parent_str = parent.to_string_lossy().to_string();
        if parent_str.is_empty() {
            "aileron://files?path=%2F".to_string()
        } else {
            format!("aileron://files?path={}", percent_encode_path(&parent_str))
        }
    };

    let mut rows_html = String::new();
    let mut index: usize = 0;

    if !parent_url.is_empty() {
        rows_html.push_str(&format!(
            "<tr data-index=\"{index}\"><td class=\"dir\"><a href=\"{parent_url}\" data-parent>..</a></td><td class=\"size\">-</td><td class=\"modified\">-</td></tr>\n"
        ));
        index += 1;
    }

    for (name, entry) in &dirs {
        let full_path = entry.path();
        let encoded = percent_encode_path(&full_path.to_string_lossy());
        let meta = entry.metadata().ok();
        let modified = meta.as_ref().map_or("-".to_string(), format_modified_time);
        rows_html.push_str(&format!(
            "<tr data-index=\"{}\"><td class=\"dir\"><a href=\"aileron://files?path={}\">{}/</a></td><td class=\"size\">-</td><td class=\"modified\">{}</td></tr>\n",
            index, encoded, html_escape(name), modified
        ));
        index += 1;
    }

    for (name, entry) in &files {
        let full_path = entry.path();
        let encoded = percent_encode_path(&full_path.to_string_lossy());
        let meta = entry.metadata().ok();
        let size = meta
            .as_ref()
            .map_or("-".to_string(), |m| format_size(m.len()));
        let modified = meta.as_ref().map_or("-".to_string(), format_modified_time);
        rows_html.push_str(&format!(
            "<tr data-index=\"{}\"><td class=\"file\"><a href=\"aileron://open?path={}\">{}</a></td><td class=\"size\">{}</td><td class=\"modified\">{}</td></tr>\n",
            index, encoded, html_escape(name), size, modified
        ));
        index += 1;
    }

    format!(
        r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>Files: {}</title>
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #141414; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace; padding: 16px; }}
  .breadcrumb {{ color: #888; margin-bottom: 12px; font-size: 14px; }}
  .breadcrumb a {{ color: #4db4ff; text-decoration: none; }}
  .breadcrumb a:hover {{ text-decoration: underline; }}
  .breadcrumb .sep {{ color: #555; margin: 0 4px; }}
  table {{ width: 100%; border-collapse: collapse; }}
  th {{ text-align: left; color: #888; font-weight: normal; padding: 4px 8px; border-bottom: 1px solid #333; font-size: 12px; }}
  td {{ padding: 4px 8px; font-size: 13px; }}
  tr {{ cursor: pointer; }}
  tr:hover {{ background: #1e1e2e; }}
  tr.selected {{ background: #264f78; }}
  a {{ color: inherit; text-decoration: none; }}
  .dir {{ color: #74c0fc; }}
  .file {{ color: #e0e0e0; }}
  .size {{ color: #888; text-align: right; width: 100px; }}
  .modified {{ color: #888; width: 180px; }}
  .error {{ color: #ff6b6b; padding: 20px; }}
</style>
</head>
<body>
<div class="breadcrumb">{}</div>
<table><thead><tr><th>Name</th><th class="size">Size</th><th class="modified">Modified</th></tr></thead>
<tbody>
{}
</tbody></table>
<script>
(function() {{
  var selected = 0;
  var rows = document.querySelectorAll('tbody tr[data-index]');
  function updateSelection() {{
    rows.forEach(function(r) {{ r.classList.remove('selected'); }});
    if (rows[selected]) {{
      rows[selected].classList.add('selected');
      rows[selected].scrollIntoView({{ block: 'nearest' }});
    }}
  }}
  document.addEventListener('keydown', function(e) {{
    if (e.target.tagName === 'INPUT') return;
    switch(e.key) {{
      case 'j': case 'ArrowDown':
        e.preventDefault();
        if (selected < rows.length - 1) {{ selected++; updateSelection(); }}
        break;
      case 'k': case 'ArrowUp':
        e.preventDefault();
        if (selected > 0) {{ selected--; updateSelection(); }}
        break;
      case 'Enter':
        e.preventDefault();
        if (rows[selected]) {{
          var link = rows[selected].querySelector('a');
          if (link) link.click();
        }}
        break;
      case 'Backspace': case 'h':
        e.preventDefault();
        var parentLink = document.querySelector('a[data-parent]');
        if (parentLink) parentLink.click();
        break;
    }}
  }});
  updateSelection();
}})();
</script>
</body>
</html>"#,
        html_escape(&dir_path),
        breadcrumb_html,
        rows_html
    )
}

pub(crate) fn aileron_404_page(requested_url: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Page Not Found</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #ccc; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
.container {{ text-align: center; }}
h1 {{ color: #ff6b6b; font-size: 3em; margin-bottom: 0.3em; }}
p {{ color: #888; margin: 0.5em 0; }}
a {{ color: #4db4ff; }}
.url {{ color: #666; font-size: 0.9em; margin-top: 1em; word-break: break-all; }}
</style></head><body>
<div class="container">
<h1>404</h1>
<p>Page not found</p>
<p class="url">{url}</p>
<p><a href="aileron://new">Go to new tab</a></p>
</div></body></html>"#,
        url = html_escape(requested_url)
    )
}

pub(crate) fn aileron_terminal_page() -> String {
    r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>Terminal</title>
<style>
body {{ font-family: monospace; background: #1a1a2e; color: #ccc; display: flex; align-items: center; justify-content: center; height: 100vh; margin: 0; }}
.container {{ text-align: center; }}
h1 {{ color: #4db4ff; }}
p {{ color: #888; }}
kbd {{ background: #333; padding: 2px 6px; border-radius: 3px; border: 1px solid #555; }}
</style></head><body>
<div class="container">
<h1>Terminal</h1>
<p>Use <kbd>Ctrl+Shift+T</kbd> or <kbd>:terminal</kbd> to open a terminal pane.</p>
<p>The terminal renders directly in this window with native performance.</p>
</div></body></html>"#.to_string()
}

pub(crate) fn aileron_settings_page() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>Aileron Settings</title>
<style>
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: #1a1a2e; color: #e0e0e0; font-family: 'SF Mono', 'Fira Code', monospace; padding: 2em; max-width: 700px; }
  h1 { color: #4db4ff; margin-bottom: 0.5em; font-size: 1.8em; }
  .subtitle { color: #666; margin-bottom: 1.5em; font-size: 0.85em; }
  h2 { color: #4db4ff; margin: 1.5em 0 0.5em; font-size: 1.1em; border-bottom: 1px solid #333; padding-bottom: 0.3em; }
  .field { margin: 0.6em 0; }
  label { display: block; margin-bottom: 0.2em; color: #999; font-size: 0.85em; }
  input[type="text"], input[type="url"], input[type="number"], select {
    background: #16213e; border: 1px solid #333; color: #e0e0e0;
    padding: 7px 10px; font-family: inherit; font-size: 13px;
    width: 100%; max-width: 480px; border-radius: 4px; outline: none;
  }
  input:focus, select:focus { border-color: #4db4ff; }
  .toggle-row { display: flex; align-items: center; margin: 0.5em 0; gap: 8px; }
  .toggle-row label { margin: 0; color: #e0e0e0; font-size: 0.95em; cursor: pointer; }
  button {
    background: #4db4ff; color: #000; border: none; padding: 9px 22px;
    font-family: inherit; font-size: 13px; font-weight: bold;
    border-radius: 4px; cursor: pointer; margin-top: 1.2em;
  }
  button:hover { background: #3a9fe0; }
  button:focus { outline: 2px solid #4db4ff; outline-offset: 2px; }
  #status { color: #888; margin-top: 0.6em; font-size: 0.85em; min-height: 1.2em; }
  #status.ok { color: #4caf50; }
  .sr-only { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px;
              overflow: hidden; clip: rect(0,0,0,0); border: 0; }
</style>
</head>
<body>
<h1>Settings</h1>
<p class="subtitle">aileron://settings</p>

<form role="form" aria-label="Aileron settings">

<h2>General</h2>
<div class="field">
  <label for="homepage">Homepage URL</label>
  <input type="url" id="homepage" tabindex="1" aria-label="Homepage URL" />
</div>
<div class="field">
  <label for="search_engine">Search Engine</label>
  <select id="search_engine" tabindex="2" aria-label="Search engine">
    <!-- Populated dynamically from config.search_engines -->
  </select>
</div>
<div class="toggle-row">
  <input type="checkbox" id="restore_session" tabindex="3" role="switch" aria-checked="false" />
  <label for="restore_session">Restore previous session on startup</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="auto_save" role="switch" aria-checked="false" />
  <label for="auto_save">Auto-save workspace</label>
</div>

<h2>Engine</h2>
<div class="field">
  <label for="engine_selection">Rendering Engine</label>
  <select id="engine_selection" aria-label="Rendering engine">
    <option value="auto">auto</option>
    <option value="servo">servo</option>
    <option value="webkit">webkit</option>
  </select>
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">auto = best engine per site, servo = Servo when available, webkit = always WebKit</span>
</div>

<h2>Language</h2>
<div class="field">
  <label for="language">Interface Language</label>
  <select id="language" aria-label="Interface language">
    <option value="en">English</option>
    <option value="zh">中文</option>
    <option value="ja">日本語</option>
    <option value="ko">한국어</option>
    <option value="de">Deutsch</option>
    <option value="fr">Français</option>
    <option value="es">Español</option>
    <option value="pt">Português</option>
    <option value="ru">Русский</option>
  </select>
</div>

<h2>Appearance</h2>
<div class="field">
  <label for="tab_layout">Tab Layout</label>
  <select id="tab_layout" tabindex="4" aria-label="Tab layout">
    <option value="sidebar">Sidebar</option>
    <option value="topbar">Top Bar</option>
    <option value="none">None</option>
  </select>
</div>
<div class="field">
  <label for="tab_sidebar_width">Sidebar Width (px)</label>
  <input type="text" id="tab_sidebar_width" tabindex="5" aria-label="Sidebar width in pixels" />
</div>
<div class="toggle-row">
  <input type="checkbox" id="tab_sidebar_right" tabindex="6" role="switch" aria-checked="false" />
  <label for="tab_sidebar_right">Sidebar on right</label>
</div>

<h2>Theme</h2>
<div class="field">
  <label for="theme">Color Theme</label>
  <select id="theme" aria-label="Color theme">
    <option value="dark">Dark</option>
    <option value="light">Light</option>
    <option value="gruvbox-dark">Gruvbox Dark</option>
    <option value="nord">Nord</option>
    <option value="dracula">Dracula</option>
    <option value="solarized-dark">Solarized Dark</option>
    <option value="solarized-light">Solarized Light</option>
  </select>
</div>

<h2>Privacy</h2>
<div class="toggle-row">
  <input type="checkbox" id="adblock_enabled" tabindex="7" role="switch" aria-checked="false" />
  <label for="adblock_enabled">Block ads</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="https_upgrade_enabled" tabindex="8" role="switch" aria-checked="false" />
  <label for="https_upgrade_enabled">Automatic HTTPS upgrade</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="tracking_protection_enabled" tabindex="9" role="switch" aria-checked="false" />
  <label for="tracking_protection_enabled">Tracking protection</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="popup_blocker_enabled" role="switch" aria-checked="false" />
  <label for="popup_blocker_enabled">Block Popups</label>
</div>
<div class="toggle-row">
  <input type="checkbox" id="adblock_cosmetic_filtering" role="switch" aria-checked="false" />
  <label for="adblock_cosmetic_filtering">Cosmetic filtering (element hiding)</label>
</div>
<div class="field">
  <label for="adblock_update_interval_hours">Filter List Update Interval (hours)</label>
  <input type="number" id="adblock_update_interval_hours" min="1" max="168" aria-label="Filter list update interval in hours" />
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">How often to check for filter list updates</span>
</div>

<h2>Advanced</h2>
<div class="toggle-row">
  <input type="checkbox" id="adaptive_quality" role="switch" aria-checked="false" />
  <label for="adaptive_quality">Adaptive Quality</label>
</div>
<span style="color:#666;font-size:0.8em;display:block;margin:-0.3em 0 0.5em 28px">Automatically reduce rendering quality when frame rate drops below 60fps</span>
<div class="toggle-row">
  <input type="checkbox" id="devtools" tabindex="10" role="switch" aria-checked="false" />
  <label for="devtools">Enable DevTools</label>
</div>
<div class="field">
  <label for="proxy">Proxy URL</label>
  <input type="text" id="proxy" tabindex="11" placeholder="socks5://127.0.0.1:1080" aria-label="Proxy URL" />
</div>
<div class="field">
  <label for="custom_css">Custom CSS</label>
  <input type="text" id="custom_css" tabindex="12" placeholder="body { background: #000 !important; }" aria-label="Custom CSS to inject into pages" />
</div>

<h2>Sync</h2>
<div class="field">
  <label for="sync_target">Sync Target</label>
  <input type="text" id="sync_target" placeholder="user@host:/path or /local/path" aria-label="Sync target path or SSH destination" />
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">SSH target (user@host:path) or local directory. Empty to disable sync.</span>
</div>
<div class="toggle-row">
  <input type="checkbox" id="sync_encrypted" role="switch" aria-checked="false" />
  <label for="sync_encrypted">Encrypt sync data</label>
</div>
<div class="field">
  <label for="sync_passphrase">Encryption Passphrase</label>
  <input type="password" id="sync_passphrase" placeholder="Leave empty to keep current" aria-label="Sync encryption passphrase" autocomplete="new-password" />
  <span class="subtitle" style="color:#666;font-size:0.8em;margin-top:0.2em;display:block">Required if encryption is enabled. Stored in system keyring, not in config file.</span>
</div>
<div class="toggle-row">
  <input type="checkbox" id="sync_auto" role="switch" aria-checked="false" />
  <label for="sync_auto">Auto-sync on file changes</label>
</div>
<div class="field">
  <label for="sync_auto_interval_sec">Auto-sync Interval (seconds)</label>
  <input type="number" id="sync_auto_interval_sec" min="10" max="3600" aria-label="Auto-sync interval in seconds" />
</div>

<button type="button" id="save-btn" tabindex="13" aria-label="Save settings">Save Settings</button>
</form>
<div id="status" aria-live="polite"></div>

<script>
(function() {
  window._onConfigLoaded = function(cfg) {
    document.getElementById('homepage').value = cfg.homepage || '';
    document.getElementById('search_engine').value = cfg.search_engine || '';
    document.getElementById('restore_session').checked = !!cfg.restore_session;
    document.getElementById('tab_layout').value = cfg.tab_layout || 'sidebar';
    document.getElementById('tab_sidebar_width').value = cfg.tab_sidebar_width || 180;
    document.getElementById('tab_sidebar_right').checked = !!cfg.tab_sidebar_right;
    document.getElementById('adblock_enabled').checked = !!cfg.adblock_enabled;
    document.getElementById('https_upgrade_enabled').checked = !!cfg.https_upgrade_enabled;
    document.getElementById('tracking_protection_enabled').checked = !!cfg.tracking_protection_enabled;
    document.getElementById('devtools').checked = !!cfg.devtools;
    document.getElementById('proxy').value = cfg.proxy || '';
    document.getElementById('custom_css').value = cfg.custom_css || '';
    document.getElementById('engine_selection').value = cfg.engine_selection || 'auto';
    document.getElementById('language').value = cfg.language || 'en';
    document.getElementById('adaptive_quality').checked = !!cfg.adaptive_quality;
    document.getElementById('popup_blocker_enabled').checked = !!cfg.popup_blocker_enabled;
    document.getElementById('adblock_update_interval_hours').value = cfg.adblock_update_interval_hours || 24;
    document.getElementById('theme').value = cfg.theme || 'dark';
    document.getElementById('adblock_cosmetic_filtering').checked = !!cfg.adblock_cosmetic_filtering;
    document.getElementById('auto_save').checked = !!cfg.auto_save;
    document.getElementById('sync_target').value = cfg.sync_target || '';
    document.getElementById('sync_encrypted').checked = !!cfg.sync_encrypted;
    document.getElementById('sync_passphrase').value = '';
    document.getElementById('sync_auto').checked = !!cfg.sync_auto;
    document.getElementById('sync_auto_interval_sec').value = cfg.sync_auto_interval_sec || 300;
    // Populate search engine dropdown from config
    (function() {
      var sel = document.getElementById('search_engine');
      sel.innerHTML = '';
      var engines = cfg.search_engines || {};
      var current = cfg.search_engine || '';
      var found = false;
      Object.keys(engines).forEach(function(name) {
        var opt = document.createElement('option');
        opt.value = engines[name];
        opt.textContent = name;
        if (engines[name] === current) { opt.selected = true; found = true; }
        sel.appendChild(opt);
      });
      // If current engine not in the list, add it as a custom option
      if (!found && current) {
        var opt = document.createElement('option');
        opt.value = current;
        opt.textContent = 'Custom';
        opt.selected = true;
        sel.appendChild(opt);
      }
    })();
    document.querySelectorAll('input[role="switch"]').forEach(function(el) {
      el.setAttribute('aria-checked', el.checked ? 'true' : 'false');
      el.addEventListener('change', function() {
        el.setAttribute('aria-checked', el.checked ? 'true' : 'false');
      });
    });
  };
  window._onConfigSaved = function() {
    var s = document.getElementById('status');
    s.textContent = 'Settings saved';
    s.className = 'ok';
    setTimeout(function() { s.textContent = ''; s.className = ''; }, 3000);
  };
  function collectConfig() {
    return {
      homepage: document.getElementById('homepage').value,
      search_engine: document.getElementById('search_engine').value,
      restore_session: document.getElementById('restore_session').checked,
      tab_layout: document.getElementById('tab_layout').value,
      tab_sidebar_width: parseFloat(document.getElementById('tab_sidebar_width').value) || 180,
      tab_sidebar_right: document.getElementById('tab_sidebar_right').checked,
      adblock_enabled: document.getElementById('adblock_enabled').checked,
      https_upgrade_enabled: document.getElementById('https_upgrade_enabled').checked,
      tracking_protection_enabled: document.getElementById('tracking_protection_enabled').checked,
      devtools: document.getElementById('devtools').checked,
      proxy: document.getElementById('proxy').value || null,
      custom_css: document.getElementById('custom_css').value || null,
      engine_selection: document.getElementById('engine_selection').value,
      language: document.getElementById('language').value || null,
      adaptive_quality: document.getElementById('adaptive_quality').checked,
      popup_blocker_enabled: document.getElementById('popup_blocker_enabled').checked,
      adblock_update_interval_hours: parseInt(document.getElementById('adblock_update_interval_hours').value) || 24,
      theme: document.getElementById('theme').value,
      adblock_cosmetic_filtering: document.getElementById('adblock_cosmetic_filtering').checked,
      auto_save: document.getElementById('auto_save').checked,
      sync_target: document.getElementById('sync_target').value || '',
      sync_encrypted: document.getElementById('sync_encrypted').checked,
      sync_passphrase: document.getElementById('sync_passphrase').value || null,
      sync_auto: document.getElementById('sync_auto').checked,
      sync_auto_interval_sec: parseInt(document.getElementById('sync_auto_interval_sec').value) || 300
    };
  }
  document.getElementById('save-btn').addEventListener('click', function() {
    window.ipc.postMessage(JSON.stringify({t:'set-config', config: collectConfig()}));
  });
  document.addEventListener('keydown', function(e) {
    if (e.key === 'Enter' && e.target.tagName !== 'BUTTON') {
      e.preventDefault();
      document.getElementById('save-btn').click();
    }
  });
  window.ipc.postMessage(JSON.stringify({t:'get-config'}));
})();
</script>
</body>
</html>"#.to_string()
}

/// Check if a URL points to a PDF resource (by file extension in path).
/// Used to prevent downloading PDFs so WebKitGTK renders them inline.
pub(crate) fn is_pdf_url(url: &str) -> bool {
    url::Url::parse(url)
        .map(|u| u.path().to_lowercase().ends_with(".pdf"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── HTML page generator tests ────────────────────────────────

    #[test]
    fn test_welcome_page_is_valid_html() {
        let html = aileron_welcome_page();
        assert!(
            html.starts_with("<!DOCTYPE html>"),
            "Should start with DOCTYPE"
        );
        assert!(
            html.contains("<title>Aileron</title>"),
            "Should have Aileron title"
        );
        assert!(html.contains("</html>"), "Should close html tag");
        assert!(html.len() > 100, "Should be substantial content");
    }

    #[test]
    fn test_welcome_page_contains_keybinding_hints() {
        let html = aileron_welcome_page();
        assert!(
            html.contains("kbd"),
            "Should contain keyboard shortcut hints"
        );
        assert!(
            html.contains("Command palette"),
            "Should mention command palette"
        );
        assert!(
            html.contains("Split vertical"),
            "Should mention split vertical"
        );
    }

    #[test]
    fn test_new_tab_page_is_valid_html() {
        let html = aileron_new_tab_page();
        assert!(
            html.starts_with("<!DOCTYPE html>"),
            "Should start with DOCTYPE"
        );
        assert!(
            html.contains("<title>New Tab</title>"),
            "Should have New Tab title"
        );
        assert!(html.contains("</html>"), "Should close html tag");
    }

    #[test]
    fn test_new_tab_page_contains_navigation_hint() {
        let html = aileron_new_tab_page();
        assert!(
            html.contains("Ctrl+P"),
            "Should mention Ctrl+P for navigation"
        );
        assert!(html.contains("Search"), "Should have search functionality");
    }

    #[test]
    fn test_file_browser_page_generates_valid_html() {
        let uri = wry::http::Uri::from_static("aileron://files?path=%2Ftmp");
        let html = file_browser_page(&uri);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Files:"));
        assert!(html.contains("<table"));
    }

    #[test]
    fn test_file_browser_page_uses_home_dir_as_default() {
        let uri = wry::http::Uri::from_static("aileron://files");
        let html = file_browser_page(&uri);
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_file_browser_page_handles_invalid_path() {
        let uri = wry::http::Uri::from_static("aileron://files?path=%2Fnonexistent_dir_xyz");
        let html = file_browser_page(&uri);
        assert!(html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn test_percent_encode_path() {
        assert_eq!(percent_encode_path("/home/user"), "/home/user");
        assert_eq!(
            percent_encode_path("/home/user/my file.txt"),
            "/home/user/my%20file.txt"
        );
        assert_eq!(
            percent_encode_path("/home/user/dir with spaces/"),
            "/home/user/dir%20with%20spaces/"
        );
    }

    #[test]
    fn test_percent_decode() {
        assert_eq!(percent_decode("/home/user"), "/home/user");
        assert_eq!(
            percent_decode("/home/user/my%20file.txt"),
            "/home/user/my file.txt"
        );
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1_048_576), "1.0 MB");
        assert_eq!(format_size(1_073_741_824), "1.0 GB");
    }

    #[test]
    fn test_is_pdf_url() {
        assert!(is_pdf_url("https://example.com/doc.pdf"));
        assert!(is_pdf_url("https://example.com/path/to/FILE.PDF"));
        assert!(is_pdf_url("http://example.com/document.pdf?query=1"));
        assert!(!is_pdf_url("https://example.com/page.html"));
        assert!(!is_pdf_url("https://example.com/"));
        assert!(!is_pdf_url("not a url"));
        assert!(!is_pdf_url("https://example.com/pdfhandler"));
    }
}
