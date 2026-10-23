import { RootProvider } from 'fumadocs-ui/provider/next';
import { i18nProvider } from 'fumadocs-ui/i18n';
import { translations } from '@/lib/layout.shared';
import { notFound } from 'next/navigation';
import { i18n } from '@/lib/i18n';
import '../global.css';
export const metadata = { icons: { icon: '/favicon.png' }, title: { default: 'SAG Wiki · 文档', template: '%s | SAG Wiki' }, description: 'SAG Wiki 使用文档：本地知识库、精准检索、引用问答与 Agent CLI。' };
export default async function Layout({ children, params }: { children: React.ReactNode; params: Promise<{lang:string}> }) {
  const { lang } = await params;
  if (lang !== 'zh' && lang !== 'en') notFound();
  return <html lang={lang === 'zh' ? 'zh-CN' : 'en'} suppressHydrationWarning><body><RootProvider i18n={i18nProvider(translations, lang)}>{children}</RootProvider></body></html>;
}
