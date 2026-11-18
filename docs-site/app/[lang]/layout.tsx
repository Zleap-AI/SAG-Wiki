import { RootProvider } from 'fumadocs-ui/provider/next';
import { i18nProvider } from 'fumadocs-ui/i18n';
import { translations } from '@/lib/layout.shared';
import { notFound } from 'next/navigation';
import { docsLocales, isDocsLocale } from '@/lib/i18n';
import type { Metadata } from 'next';
import '../global.css';

export async function generateMetadata({ params }: { params: Promise<{ lang: string }> }): Promise<Metadata> {
  const { lang } = await params;
  if (!isDocsLocale(lang)) notFound();
  const locale = docsLocales[lang];
  return {
    icons: { icon: '/favicon.png' },
    title: { default: `SAG Wiki · ${locale.label}`, template: '%s | SAG Wiki' },
    description: locale.description,
  };
}

export default async function Layout({ children, params }: { children: React.ReactNode; params: Promise<{ lang: string }> }) {
  const { lang } = await params;
  if (!isDocsLocale(lang)) notFound();
  return <html lang={docsLocales[lang].htmlLang} suppressHydrationWarning><body><RootProvider i18n={i18nProvider(translations, lang)}>{children}</RootProvider></body></html>;
}
