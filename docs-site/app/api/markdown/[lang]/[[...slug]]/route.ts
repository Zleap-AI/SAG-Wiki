import { docsLlms, resolveDocsPage } from '@/lib/source';
import { docsLocales, isDocsLocale } from '@/lib/i18n';

type Context = { params: Promise<{ lang: string; slug?: string[] }> };

export async function GET(request: Request, { params }: Context) {
  const { lang, slug } = await params;
  if (!isDocsLocale(lang)) {
    return new Response('Not found', { status: 404 });
  }
  const page = resolveDocsPage(slug, lang);
  if (!page || !page.locale || !isDocsLocale(page.locale)) return new Response('Not found', { status: 404 });
  if (page.locale !== lang) {
    const path = ['/api/markdown', page.locale, ...page.slugs.map(encodeURIComponent)].join('/');
    return Response.redirect(new URL(path, request.url), 307);
  }
  return new Response(await docsLlms.page(page), {
    headers: {
      'Content-Type': 'text/markdown; charset=utf-8',
      'Content-Language': docsLocales[lang].htmlLang,
    },
  });
}
