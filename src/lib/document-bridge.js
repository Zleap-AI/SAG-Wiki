(() => {
  const scrollRoot = () => document.scrollingElement || document.documentElement;
  const reportScroll = () => {
    const root = scrollRoot();
    window.parent.postMessage({type: 'sag-reader-scroll', top: root.scrollTop, left: root.scrollLeft}, '*');
  };
  let quietTimer;
  document.addEventListener('scroll', () => {
    document.documentElement.classList.add('sag-scroll-active');
    clearTimeout(quietTimer);
    quietTimer = setTimeout(() => document.documentElement.classList.remove('sag-scroll-active'), 800);
    reportScroll();
  }, {capture: true, passive: true});
  document.addEventListener('click', event => {
    const link = event.target.closest('a[href]');
    if (!link || !/^(https?:|mailto:)/i.test(link.href)) return;
    event.preventDefault();
    window.parent.postMessage({type: 'sag-external-link', url: link.href}, '*');
  });
  document.addEventListener('contextmenu', event => event.preventDefault());
  window.addEventListener('message', event => {
    if (event.source !== window.parent) return;
    if (event.data?.type === 'sag-restore-scroll') {
      if (!Number.isFinite(event.data.top) || !Number.isFinite(event.data.left)) return;
      const root = scrollRoot();
      root.scrollTop = event.data.top;
      root.scrollLeft = event.data.left;
      reportScroll();
      return;
    }
    if (event.data?.type !== 'sag-select-match' || !Number.isInteger(event.data.index)) return;
    document.querySelectorAll('.search-hit.active').forEach(mark => mark.classList.remove('active'));
    document.querySelectorAll('.search-active-paragraph').forEach(block => block.classList.remove('search-active-paragraph'));
    const marks = document.querySelectorAll(`[data-search-match="${event.data.index}"]`);
    marks.forEach(mark => {
      mark.classList.add('active');
      if (event.data.paragraph) mark.parentElement.closest('p,li,pre,td,th,h1,h2,h3,h4,h5,h6,div,blockquote,body').classList.add('search-active-paragraph');
    });
    if (event.data.scroll !== false) marks[0]?.scrollIntoView({block: 'center'});
    reportScroll();
  });
})();
