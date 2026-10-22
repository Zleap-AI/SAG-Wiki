const translations = {
    en: {
        title: 'SAG Web Clipper', language: 'Language', automatic: 'Follow system',
        unknown: 'Enable SAG Web Clipper in Safari Settings → Extensions.',
        on: 'SAG Web Clipper is enabled. Right-click a webpage to save it as Markdown.',
        off: 'SAG Web Clipper is disabled. Enable it in Safari Settings → Extensions.',
        open: 'Quit and open Safari settings…',
    },
    zh: {
        title: 'SAG 网页剪藏', language: '语言', automatic: '跟随系统',
        unknown: '请在 Safari 设置 → 扩展中启用 SAG 网页剪藏。',
        on: 'SAG 网页剪藏已启用。在网页上右键即可保存为 Markdown。',
        off: 'SAG 网页剪藏尚未启用。请在 Safari 设置 → 扩展中启用。',
        open: '退出并打开 Safari 设置…',
    },
};
let enabled;
const language = document.querySelector('#language');
language.value = localStorage.getItem('language') || '';
function render() {
    const chinese = (language.value || navigator.language).startsWith('zh');
    const t = translations[chinese ? 'zh' : 'en'];
    document.documentElement.lang = chinese ? 'zh-CN' : 'en';
    document.title = document.querySelector('h1').textContent = t.title;
    document.querySelector('label').textContent = t.language;
    language.options[0].textContent = t.automatic;
    document.querySelector('#state').textContent = t[enabled === true ? 'on' : enabled === false ? 'off' : 'unknown'];
    document.querySelector('button').textContent = t.open;
}
function show(state) {
    enabled = state;
    render();
}
language.addEventListener('change', () => {
    localStorage.setItem('language', language.value);
    render();
});
document.querySelector('button').addEventListener('click', () => {
    webkit.messageHandlers.controller.postMessage('open-preferences');
});
render();
