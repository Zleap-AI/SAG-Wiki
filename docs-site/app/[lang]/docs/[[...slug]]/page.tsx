import { source } from '@/lib/source';
import { DocsBody, DocsDescription, DocsPage, DocsTitle, MarkdownCopyButton } from 'fumadocs-ui/layouts/docs/page';
import { notFound } from 'next/navigation';
import { getMDXComponents } from '@/components/mdx';
import { createRelativeLink } from 'fumadocs-ui/mdx';
import type { Metadata } from 'next';
type Props = { params: Promise<{ lang: string; slug?: string[] }> };
export default async function Page({ params }: Props) {
 const {lang,slug}=await params; const page=source.getPage(slug,lang); if(!page) notFound();
 const MDX=page.data.body;
 const markdownUrl = `/api/markdown/${lang}${page.slugs.length ? '/' + page.slugs.map(encodeURIComponent).join('/') : ''}`;
 return <DocsPage toc={page.data.toc} full={page.data.full}><div className="doc-title-row"><DocsTitle>{page.data.title}</DocsTitle><MarkdownCopyButton markdownUrl={markdownUrl} title={lang === 'zh' ? '复制页面' : 'Copy page'} /></div><DocsDescription className="mb-3">{page.data.description}</DocsDescription><DocsBody><MDX components={getMDXComponents({a:createRelativeLink(source,page)})}/></DocsBody></DocsPage>;
}
export function generateStaticParams() { return source.generateParams(); }
export async function generateMetadata({params}:Props): Promise<Metadata> { const {lang,slug}=await params; const page=source.getPage(slug,lang); if(!page) notFound(); return {title:page.data.title === 'SAG Wiki' ? {absolute: lang === 'zh' ? 'SAG Wiki 文档' : 'SAG Wiki Docs'} : page.data.title,description:page.data.description}; }
