const translations = {
    en: {
        title: 'SAG Web Clipper', language: 'Language', automatic: 'Follow system',
        unknown: 'App installed. Checking the Safari extension status…',
        on: 'Installed · enabled. Return to a webpage and choose “Save as Markdown” from its right-click menu.',
        off: 'Installed · not enabled. Enable it in Safari Settings → Extensions.',
        open: 'Quit and open Safari settings…', done: 'Done', retry: 'Check again',
        timeout: 'Safari has not returned the extension status. Open Safari settings or check again.',
        'not-found': 'Safari has not detected the extension. Open SAG Web Clipper from Applications, then check again.',
        failed: 'Unable to check the Safari extension status: ',
    },
    zh: {
        title: 'SAG 网页剪藏', language: '语言', automatic: '跟随系统',
        unknown: '应用已安装，正在检查 Safari 扩展的启用状态…',
        on: '已安装 · 已启用。回到网页，右键选择“保存为 Markdown”即可使用。',
        off: '已安装 · 待启用。请在 Safari 设置 → 扩展中启用。',
        open: '退出并打开 Safari 设置…', done: '完成', retry: '重新检查',
        timeout: 'Safari 尚未返回扩展状态。请打开 Safari 设置查看，或重新检查。',
        'not-found': 'Safari 尚未检测到扩展。请从“应用程序”打开 SAG Web Clipper 后重新检查。',
        failed: '无法检查 Safari 扩展状态：',
    },
};
let enabled;
let problem = '';
const language = document.querySelector('#language');
language.value = localStorage.getItem('language') || '';
function render() {
    const chinese = (language.value || navigator.language).startsWith('zh');
    const t = translations[chinese ? 'zh' : 'en'];
    document.documentElement.lang = chinese ? 'zh-CN' : 'en';
    document.title = document.querySelector('h1').textContent = t.title;
    document.querySelector('label').textContent = t.language;
    language.options[0].textContent = t.automatic;
    document.querySelector('#state').textContent = problem ? t[problem] || t.failed + problem : t[enabled === true ? 'on' : enabled === false ? 'off' : 'unknown'];
    const button = document.querySelector('#action');
    button.textContent = enabled === true ? t.done : t.open;
    const retry = document.querySelector('#retry');
    retry.textContent = t.retry;
    retry.hidden = enabled === true;
    retry.disabled = enabled == null && !problem;
}
function show(state, error = '') {
    enabled = state;
    problem = error;
    render();
}
language.addEventListener('change', () => {
    localStorage.setItem('language', language.value);
    render();
});
document.querySelector('#action').addEventListener('click', () => {
    webkit.messageHandlers.controller.postMessage(enabled === true ? 'close' : 'open-preferences');
});
document.querySelector('#retry').addEventListener('click', () => {
    webkit.messageHandlers.controller.postMessage('check-state');
});
render();
