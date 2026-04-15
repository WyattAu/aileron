-- ==UserScript==
-- @name        Dark Mode Everywhere
-- @match       *://*
-- @grant       none
-- ==/UserScript==

return [[
  (function() {
    if (document.body) {
      document.body.style.background = '#1a1a1a';
      document.body.style.color = '#d4d4d4';
    }
    document.documentElement.style.colorScheme = 'dark';
  })();
]]
