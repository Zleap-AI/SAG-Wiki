import { defineI18n } from 'fumadocs-core/i18n';

export const docsLocales = {
  en: {
    htmlLang: 'en',
    label: 'Docs',
    description: 'SAG Wiki documentation for the local knowledge base, search, Q&A and Agent CLI.',
  },
  zh: {
    htmlLang: 'zh-CN',
    label: '文档',
    description: 'SAG Wiki 使用文档：本地知识库、精准检索、引用问答与 Agent CLI。',
  },
  ja: {
    htmlLang: 'ja',
    label: 'ドキュメント',
    description: 'ローカルナレッジベース、検索、引用付きの質問応答、Agent CLI の使い方を紹介します。',
  },
} as const;

export type DocsLocale = keyof typeof docsLocales;

export function isDocsLocale(value: string): value is DocsLocale {
  return Object.hasOwn(docsLocales, value);
}

export const i18n = defineI18n({
  defaultLanguage: 'zh',
  languages: Object.keys(docsLocales) as DocsLocale[],
  fallbackLanguage: null,
});
