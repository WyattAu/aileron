-- ==UserScript==
-- @name        Wide GitHub
-- @match       https://github.com/*/*
-- @grant       none
-- ==/UserScript==

return [[
  (function() {
    var container = document.querySelector('.application-main');
    if (container) {
      container.style.maxWidth = '100%';
    }
    var repoContent = document.querySelector('[data-target="repo-content-turbo-frame"]');
    if (repoContent) {
      repoContent.style.maxWidth = '100%';
    }
  })();
]]
