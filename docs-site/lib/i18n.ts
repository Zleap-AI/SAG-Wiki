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
    fallbackToEnglish: true,
    htmlLang: 'ja',
    label: 'ドキュメント',
    description: 'ローカルナレッジベース、検索、引用付きの質問応答、Agent CLI の使い方を紹介します。',
  },
  'zh-TW': {
    fallbackToEnglish: true,
    htmlLang: 'zh-TW', label: '文件',
    description: 'SAG Wiki 使用指南：本機知識庫、搜尋、引用問答與 Agent CLI。',
  },
  ko: {
    fallbackToEnglish: true,
    htmlLang: 'ko', label: '문서',
    description: 'SAG Wiki 사용 가이드입니다. 로컬 지식 베이스, 검색, 출처가 포함된 질의응답과 Agent CLI를 소개합니다.',
  },
  es: {
    fallbackToEnglish: true,
    htmlLang: 'es', label: 'Documentación',
    description: 'Guía de SAG Wiki para la base de conocimiento local, la búsqueda, las respuestas con citas y Agent CLI.',
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
