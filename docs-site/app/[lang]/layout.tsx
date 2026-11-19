import { DocsProvider } from '@/components/docs-provider';
import { source } from '@/lib/source';
import { i18nProvider } from 'fumadocs-ui/i18n';
import { translations } from '@/lib/layout.shared';
import { notFound } from 'next/navigation';
import { docsLocales, i18n, isDocsLocale } from '@/lib/i18n';
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
  const translatedPaths = Object.fromEntries(i18n.languages
    .filter(isDocsLocale)
    .filter(locale => 'fallbackToEnglish' in docsLocales[locale])
    .map(locale => [locale, source.getPages(locale).map(page => page.url)]));
  return (
    <html lang={docsLocales[lang].htmlLang} suppressHydrationWarning>
      <body>
        <DocsProvider i18n={i18nProvider(translations, lang)} translatedPaths={translatedPaths}>
          {children}
        </DocsProvider>
      </body>
    </html>
  );
}
