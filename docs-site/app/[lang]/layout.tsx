import { RootProvider } from 'fumadocs-ui/provider/next';
import { i18nProvider } from 'fumadocs-ui/i18n';
import { translations } from '@/lib/layout.shared';
import { notFound } from 'next/navigation';
import { i18n } from '@/lib/i18n';
import '../global.css';
export const metadata = { icons: { icon: '/favicon.png' }, title: { default: 'SAG Wiki · 飞牛文档', template: '%s | SAG Wiki' }, description: 'SAG Wiki 飞牛版：原生文件授权、知识库索引、搜索与引用问答。' };
export default async function Layout({ children, params }: { children: React.ReactNode; params: Promise<{lang:string}> }) {
  const { lang } = await params;
  if (lang !== 'zh' && lang !== 'en') notFound();
  return <html lang={lang === 'zh' ? 'zh-CN' : 'en'} suppressHydrationWarning><body><RootProvider i18n={i18nProvider(translations, lang)}>{children}</RootProvider></body></html>;
}
