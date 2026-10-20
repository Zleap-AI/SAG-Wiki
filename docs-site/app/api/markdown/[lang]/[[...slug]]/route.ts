import { docsLlms, source } from '@/lib/source';
import { i18n } from '@/lib/i18n';

type Context = { params: Promise<{ lang: string; slug?: string[] }> };

export async function GET(_request: Request, { params }: Context) {
  const { lang, slug } = await params;
  if (!i18n.languages.some(language => language === lang)) {
    return new Response('Not found', { status: 404 });
  }
  const page = source.getPage(slug, lang);
  if (!page) return new Response('Not found', { status: 404 });
  return new Response(await docsLlms.page(page), {
    headers: { 'Content-Type': 'text/markdown; charset=utf-8' },
  });
}
