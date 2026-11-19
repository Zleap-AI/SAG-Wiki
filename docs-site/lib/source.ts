import { llms, loader } from 'fumadocs-core/source';
import { lucideIconsPlugin } from 'fumadocs-core/source/lucide-icons';
import { defineDocs } from 'fumadocs-mdx/macro';
import { docsLocales, i18n, type DocsLocale } from './i18n';
const docs = defineDocs({ dir: 'content/docs', docs: { postprocess: { includeProcessedMarkdown: true } } });
export const source = loader({ baseUrl: '/docs', i18n, source: docs.toFumadocsSource(), plugins: [lucideIconsPlugin()] });
export const docsLlms = llms(source, { renderPage: async (page) => `# ${page.data.title} (${page.url})\n\n${await page.data.getText('processed')}` });

// Search indexes and page trees contain only pages translated into their locale.
export function resolveDocsPage(slug: string[] | undefined, locale: DocsLocale) {
  const page = source.getPage(slug, locale);
  if (page || !('fallbackToEnglish' in docsLocales[locale])) return page;
  return source.getPage(slug, 'en') ?? source.getPage(slug, 'zh');
}
