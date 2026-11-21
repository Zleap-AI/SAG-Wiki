import { resolveDocsPage, source } from '@/lib/source';
import { docsLocales, isDocsLocale } from '@/lib/i18n';
import { translations } from '@/lib/layout.shared';
import { DocsBody, DocsDescription, DocsPage, DocsTitle, MarkdownCopyButton } from 'fumadocs-ui/layouts/docs/page';
import { notFound, redirect } from 'next/navigation';
import { getMDXComponents } from '@/components/mdx';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import type { Metadata } from 'next';

type Props = { params: Promise<{ lang: string; slug?: string[] }> };

async function getPage({ params }: Props) {
  const { lang, slug } = await params;
  if (!isDocsLocale(lang)) notFound();
  const page = resolveDocsPage(slug, lang);
  if (!page) notFound();
  if (page.locale !== lang) redirect(page.url);
  return { page, lang };
}

export default async function Page(props: Props) {
  const { page, lang } = await getPage(props);
  const MDX = page.data.body;
  const markdownUrl = ['/api/markdown', lang, ...page.slugs.map(encodeURIComponent)].join('/');
  return <DocsPage toc={page.data.toc} full={page.data.full}>
    <div className="doc-title-row">
      <DocsTitle>{page.data.title}</DocsTitle>
      <MarkdownCopyButton markdownUrl={markdownUrl} title={translations.get(lang)['Copy Markdown(page actions)']} />
    </div>
    <DocsDescription className="mb-3">{page.data.description}</DocsDescription>
    <DocsBody><MDX components={getMDXComponents({ a: createRelativeLink(source, page) })} /></DocsBody>
  </DocsPage>;
}

export function generateStaticParams() { return source.generateParams(); }

export async function generateMetadata(props: Props): Promise<Metadata> {
  const { page, lang } = await getPage(props);
  return {
    title: page.data.title === 'SAG Wiki' ? { absolute: `SAG Wiki ${docsLocales[lang].label}` } : page.data.title,
    description: page.data.description,
  };
}
