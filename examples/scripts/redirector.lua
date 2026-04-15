-- ==UserScript==
-- @name        Nitter Redirector
-- @match       https://twitter.com/*
-- @match       https://x.com/*
-- @grant       none
-- ==/UserScript==

return [[
  (function() {
    var nitterHost = 'nitter.net';
    if (window.location.host !== nitterHost) {
      window.location.host = nitterHost;
      window.location.pathname = window.location.pathname;
    }
  })();
]]
