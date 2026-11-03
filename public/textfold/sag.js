// SAG integration boundary. reader.js, reader.css and i18n.js are upstream originals.
(() => {
  let initialized = false;
  const send = (type, extra = {}) => parent.postMessage({type, ...extra}, location.origin);
  addEventListener('error', event => send('sag-fold-error', {message: event.message}));
  addEventListener('unhandledrejection', event => send('sag-fold-error', {message: String(event.reason)}));
  addEventListener('message', event => {
    if (event.source !== parent || event.origin !== location.origin) return;
    const data = event.data;
    if (data.type !== 'sag-fold-init' || initialized) return;
    initialized = true;
    for (const [key, value] of Object.entries(data.palette)) document.documentElement.style.setProperty(key, value);
    document.documentElement.dataset.theme = data.dark ? 'dark' : 'light';
    document.getElementById('paper-data').textContent = JSON.stringify(data.document);
    // Set the requested opening level without fighting the reader's saved position.
    location.hash = `level=${encodeURIComponent(window.DR_I18N.t('en', true).levels[data.level])}`;
    const script = document.createElement('script');
    script.src = 'reader.js';
    script.onload = () => {
      if (!window.__reader) return;
      window.__reader.setUi(data.ui);
      document.documentElement.dataset.theme = data.dark ? 'dark' : 'light';
      // The isolated animation tree declares its own upstream palette, so it
      // needs the same explicit host overrides as the visible document.
      const stageHost = document.querySelector('.zhost');
      for (const [key, value] of Object.entries(data.palette)) stageHost.style.setProperty(key, value);
      stageHost.dataset.theme = document.documentElement.dataset.theme;
      const hint = document.getElementById('hint'), help = document.createElement('button');
      help.type = 'button'; help.className = 'fold-help'; help.textContent = '?';
      help.setAttribute('aria-label', data.ui === 'zh' ? '阅读操作说明' : 'Reading controls');
      help.setAttribute('aria-controls', 'hint'); help.setAttribute('aria-expanded', 'false');
      const showHelp = visible => {
        if (visible) {
          const labels = window.DR_I18N.t(window.__reader.ui, true);
          hint.textContent = labels.hint + (document.querySelector('.mmap').hidden ? '' : ` ${labels.mm_hint}`);
        }
        hint.hidden = !visible; help.setAttribute('aria-expanded', String(visible));
      };
      help.addEventListener('click', () => showHelp(hint.hidden));
      document.addEventListener('click', event => {
        if (!hint.hidden && !help.contains(event.target) && !hint.contains(event.target)) showHelp(false);
      });
      document.addEventListener('keydown', event => {
        if (event.key === 'Escape' && !hint.hidden) { showHelp(false); help.focus(); event.stopPropagation(); }
      });
      const bar = document.getElementById('zoombar'), views = document.querySelector('.tview');
      const tabs = document.createElement('div'); tabs.className = 'sag-primary-tabs';
      bar.before(tabs); tabs.append(bar);
      const cards = bar.querySelector('[data-l="0"]'), map = document.createElement('button');
      map.type = 'button'; map.className = 'sag-map-tab'; map.dataset.view = 'map'; tabs.append(map);
      const syncTabs = () => {
        const labels = window.DR_I18N.t(window.__reader.ui, true);
        cards.textContent = labels.tv_cards; map.textContent = labels.tv_map;
        const showingMap = window.__reader.level === 0 && views.querySelector('[data-view="map"]').getAttribute('aria-checked') === 'true';
        tabs.dataset.map = String(showingMap);
        cards.setAttribute('aria-pressed', String(window.__reader.level === 0 && !showingMap));
        map.setAttribute('aria-pressed', String(showingMap)); map.classList.toggle('on', showingMap);
        if (window.__reader.level === 0) bar.setAttribute('aria-valuetext', showingMap ? labels.tv_map : labels.tv_cards);
      };
      const chooseCards = () => views.querySelector('[data-view="cards"]').click();
      bar.addEventListener('pointerdown', chooseCards, true);
      bar.addEventListener('click', chooseCards, true);
      map.addEventListener('click', () => {
        views.querySelector('[data-view="map"]').click();
        window.__reader.setLevel(0); syncTabs();
      });
      new MutationObserver(syncTabs).observe(views, {childList:true});
      new MutationObserver(syncTabs).observe(document.getElementById('doc'), {attributes:true,attributeFilter:['data-level']});
      syncTabs();
      document.querySelector('.zrow').append(help);
      // Markdown ordered lists retain their source numbering. TextFold's paper
      // parser emits unordered lists; adjust only those marked ordered by SAG.
      for (const block of data.document.blocks) {
        if (!block.ordered) continue;
        const list = document.getElementById(block.u)?.closest('ul');
        if (!list) continue;
        const ol = document.createElement('ol');
        ol.className = list.className; ol.start = block.start;
        ol.append(...list.childNodes); list.replaceWith(ol);
      }
      new MutationObserver(() => send('sag-fold-level', {level: window.__reader.level}))
        .observe(document.getElementById('doc'), {attributes: true, attributeFilter: ['data-level']});
      send('sag-fold-ready');
    };
    script.onerror = () => send('sag-fold-error', {message: 'TextFold reader could not be loaded'});
    document.body.append(script);
  });
  // External links keep the reader open and use SAG's existing link handler.
  document.addEventListener('click', event => {
    const link = event.target.closest('a[href]');
    if (!link || link.getAttribute('href').startsWith('#')) return;
    event.preventDefault(); event.stopPropagation();
    send('sag-fold-link', {href: link.getAttribute('href')});
  }, true);
})();
