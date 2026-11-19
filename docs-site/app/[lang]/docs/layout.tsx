import { isDocsLocale } from '@/lib/i18n';
import { notFound } from 'next/navigation';
import { source } from '@/lib/source';
import { DocsLayout } from 'fumadocs-ui/layouts/docs';
import { baseOptions } from '@/lib/layout.shared';
export default async function Layout({ children, params }: { children: React.ReactNode; params: Promise<{lang:string}> }) {
 const { lang } = await params;
 if (!isDocsLocale(lang)) notFound();
 return <DocsLayout tree={source.getPageTree(lang)} {...baseOptions(lang)}>{children}</DocsLayout>;
}
