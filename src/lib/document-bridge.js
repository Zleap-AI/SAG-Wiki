(() => {
  document.addEventListener('click', event => {
    const link = event.target.closest('a[href]');
    if (!link || !/^(https?:|mailto:)/i.test(link.href)) return;
    event.preventDefault();
    window.parent.postMessage({type: 'sag-external-link', url: link.href}, '*');
  });
  document.addEventListener('contextmenu', event => event.preventDefault());
  window.addEventListener('message', event => {
    if (event.source !== window.parent || event.data?.type !== 'sag-select-match') return;
    document.querySelectorAll('.search-hit.active').forEach(mark => mark.classList.remove('active'));
    const marks = document.querySelectorAll(`[data-search-match="${event.data.index}"]`);
    marks.forEach(mark => mark.classList.add('active'));
    marks[0]?.scrollIntoView({block: 'center'});
  });
})();
