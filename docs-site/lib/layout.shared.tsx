import { i18n } from './i18n';
import { uiTranslations } from 'fumadocs-ui/i18n';
import type { BaseLayoutProps } from 'fumadocs-ui/layouts/shared';
export const translations = i18n.translations().extend(uiTranslations()).add({
  zh: {
    displayName: '简体中文', 'Search(search trigger)': '搜索文档',
    'Copy Markdown(page actions)': '复制页面', 'Copied Markdown(page actions)': '已复制',
    'Search(search dialog)': '搜索文档', 'No results found(search dialog)': '未找到相关文档',
    'Search failed(search dialog)': '搜索失败', 'Close Search(search dialog)(aria-label)': '关闭搜索',
    'Open Search(search trigger)(aria-label)': '打开搜索',
    'On this page(table of contents)': '本页目录', 'No Headings(table of contents)': '暂无标题',
    'Table of Contents(inline table of contents)': '目录',
    'Next Page(pagination)': '下一页', 'Previous Page(pagination)': '上一页',
    'Choose a language(language switcher)': '选择语言',
    'Choose a language(language switcher)(aria-label)': '选择语言',
    'Language(language switcher)': '语言',
    'Copy Text(code block)(aria-label)': '复制代码', 'Copied Text(code block)(aria-label)': '已复制',
    'Copy Anchor Link(heading anchor)(aria-label)': '复制标题链接', 'Copied Anchor Link(heading anchor)(aria-label)': '已复制标题链接',
    'Collapse Sidebar(sidebar)(aria-label)': '折叠侧边栏', 'Open Sidebar(sidebar)(aria-label)': '打开侧边栏',
    'Close Sidebar(sidebar)(aria-label)': '关闭侧边栏', 'Open Sidebar(aria-label)': '打开侧边栏', 'Close Sidebar(aria-label)': '关闭侧边栏',
    'Hide Sidebar(sidebar)': '隐藏侧边栏', 'Show Sidebar(sidebar)': '显示侧边栏',
    'Toggle Theme(theme switcher)(aria-label)': '切换主题', 'Light(theme switcher)(aria-label)': '浅色',
    'Dark(theme switcher)(aria-label)': '深色', 'System(theme switcher)(aria-label)': '跟随系统',
  },
  en: { displayName: 'English', 'Copy Markdown(page actions)': 'Copy page', 'Copied Markdown(page actions)': 'Copied' },
});
export function baseOptions(lang: string): BaseLayoutProps {
  return {
    i18n: { defaultLanguage: i18n.defaultLanguage, languages: [...i18n.languages] },
    nav: { title: <span className="site-brand"><img className="brand-mark" src="/images/brand-icon.png" width={36} height={36} alt="" aria-hidden="true" /><span>SAG Wiki <small>{lang === 'zh' ? '文档' : 'Docs'}</small></span></span>, url: `/${lang}/docs` },
    links: [{ type: 'icon', url: 'https://github.com/Zleap-AI/SAG-Wiki', label: 'Zleap-AI/SAG-Wiki', text: 'Zleap-AI/SAG-Wiki', external: true, icon: <><svg aria-hidden="true" viewBox="0 0 24 24" fill="currentColor"><path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" /></svg><span className="docs-github-label">Zleap-AI/SAG-Wiki</span></> }],
  };
}
