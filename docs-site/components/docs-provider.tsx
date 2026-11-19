'use client';

import { RootProvider, type RootProviderProps } from 'fumadocs-ui/provider/next';
import { usePathname, useRouter } from 'next/navigation';

type DocsProviderProps = {
  children: React.ReactNode;
  i18n: NonNullable<RootProviderProps['i18n']>;
  translatedPaths: Record<string, string[]>;
};

export function DocsProvider({ children, i18n, translatedPaths }: DocsProviderProps) {
  const pathname = usePathname();
  const router = useRouter();

  function onLocaleChange(locale: string) {
    const pagePath = pathname.replace(/^\/[^/]+(?=\/|$)/, '');
    const destination = `/${locale}${pagePath}`;
    const isDocsPage = pagePath === '/docs' || pagePath.startsWith('/docs/');
    const available = translatedPaths[locale];
    if (available && isDocsPage && !available.includes(destination)) {
      router.push(`/${locale}/docs`);
      return;
    }
    router.push(destination);
  }

  return <RootProvider i18n={{ ...i18n, onLocaleChange }}>{children}</RootProvider>;
}
