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
    links: [{ type: 'icon', url: 'https://gitlab.zexrtq.com/zleap/data/sag-wiki-fnos', label: 'GitLab · sag-wiki-fnos', text: 'sag-wiki-fnos', external: true, icon: <span className="docs-github-label">GitLab · sag-wiki-fnos</span> }],
  };
}
