use crate::{
    models, retrieval,
    store::{self, Store},
    text,
    types::{Candidate, Document, Settings},
};
use anyhow::{Context, Result};
use regex::Regex;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tokio::sync::OnceCell;
use tokio_util::sync::CancellationToken;

type HelpPage = (&'static str, &'static str, &'static str, &'static str);
const PAGES: &[HelpPage] = &include!(concat!(env!("OUT_DIR"), "/help-documents.rs"));
static VERSION_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[0-9]+(?:\.[0-9]+){2,}").unwrap());

pub(crate) struct HelpIndex {
    store: Store,
    root: PathBuf,
    _directory: tempfile::TempDir,
}

impl HelpIndex {
    fn open() -> Result<Self> {
        let directory = tempfile::tempdir()?;
        let root = directory.path().canonicalize()?.join("sag-help");
        fs::create_dir(&root)?;
        let store = Store::open(&directory.path().join("help.sqlite"))?;
        let registered = store.add_root(&root)?;
        for &(slug, locale, _, body) in PAGES {
            let folder = root.join(locale);
            fs::create_dir_all(&folder)?;
            let path = folder.join(format!("{slug}.md"));
            fs::write(&path, body)?;
            store.index_file(&registered, &path, 950)?;
        }
        Ok(Self {
            store,
            root,
            _directory: directory,
        })
    }
}

pub(crate) struct HelpSearch<'a> {
    pub index: &'a OnceCell<HelpIndex>,
    pub settings: &'a Settings,
    pub jev: Option<(&'a reqwest::Client, &'a str)>,
    pub cancel: &'a CancellationToken,
}

impl HelpSearch<'_> {
    pub async fn search(
        &self,
        question: &str,
        progress: impl Fn(&str, usize),
    ) -> Result<retrieval::SearchOutput> {
        let index = self
            .index
            .get_or_try_init(|| async { tokio::task::spawn_blocking(HelpIndex::open).await? })
            .await?;
        let settings = self.settings;
        let language = index.root.join(
            PAGES
                .iter()
                .find_map(|&(_, locale, _, _)| (locale == settings.language).then_some(locale))
                .unwrap_or("en"),
        );
        let english = index.root.join("en");
        let local = language != english;
        let localized_changelog = language.join("updates.md");
        let changelog = if localized_changelog.is_file() {
            localized_changelog
        } else {
            english.join("updates.md")
        };
        let versions: HashSet<_> = VERSION_NUMBER
            .find_iter(question)
            .map(|version| version.as_str())
            .collect();
        let release_notes = if versions.is_empty() {
            Vec::new()
        } else {
            if self.cancel.is_cancelled() {
                anyhow::bail!("已停止");
            }
            index
                .store
                .reference_candidates(&[store::path_text(&changelog)?], i64::MAX as usize)?
                .into_iter()
                .filter(|candidate| {
                    candidate.heading.split(" / ").nth(1)
                        .is_some_and(|version| versions.contains(version))
                })
                .collect::<Vec<_>>()
        };
        let mut options = settings.retrieval.clone();
        options.top_k = options.top_k.min(5);
        let mut question_terms = text::search_terms(question);
        if question_terms
            .iter()
            .any(|term| !matches!(term.as_str(), "sag" | "wiki" | "i"))
        {
            question_terms.retain(|term| !matches!(term.as_str(), "sag" | "wiki" | "i"));
        }
        let search = |scope: Vec<PathBuf>| {
            let progress = &progress;
            let options = &options;
            let question_terms = &question_terms;
            async move {
                let scope: Vec<String> = scope
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                retrieval::retrieve(
                    &index.store,
                    question,
                    &scope,
                    &[],
                    options,
                    self.cancel,
                    |mut candidates, mut evidence| async move {
                        if let Some((client, key)) = self.jev {
                            for candidate in candidates.iter_mut().chain(&mut evidence) {
                                candidate.title = index.source(&candidate.path)?.1.to_owned();
                            }
                            models::score_with_runtime(
                                client,
                                &settings.jev,
                                key,
                                question,
                                candidates,
                                &evidence,
                                settings.timeout_secs,
                                &settings.runtime,
                                self.cancel,
                            )
                            .await
                        } else {
                            for candidate in &mut candidates {
                                // Expanded BM25 expressions include new words, so their
                                // scores can't be compared directly with the initial hits.
                                let title = index.source(&candidate.path)?.1;
                                let overlap = |text: &str| {
                                    let words: HashSet<_> =
                                        text::tokenize(text).into_iter().collect();
                                    question_terms
                                        .iter()
                                        .filter(|term| words.contains(*term))
                                        .count() as f64
                                };
                                let bm25 = -candidate.bm25;
                                candidate.score = 3. * overlap(title)
                                    + 2. * overlap(&candidate.heading)
                                    + overlap(&candidate.content)
                                    + 0.5 * bm25 / (1. + bm25);
                            }
                            Ok(candidates)
                        }
                    },
                    |stage, count| {
                        progress(
                            if self.jev.is_none() && stage == "Jev 正在判断相关原文" {
                                "正在检索原文"
                            } else {
                                stage
                            },
                            count,
                        )
                    },
                )
                .await
            }
        };
        let mut found = if release_notes.is_empty() {
            let mut scope = vec![language];
            if changelog.starts_with(&english) && local {
                scope.push(changelog);
            }
            let found = search(scope).await?;
            if local && found.results.is_empty() {
                search(vec![english]).await?
            } else {
                found
            }
        } else {
            progress("正在检索原文", 0);
            let trace = vec![retrieval::Trace {
                hop: 0,
                candidates: release_notes.iter().map(|candidate| candidate.id).collect(),
                terms: vec![],
                evidence: vec![],
            }];
            retrieval::SearchOutput { results: release_notes, trace }
        };
        for candidate in &mut found.results {
            let (path, title) = index.source(&candidate.path)?;
            candidate.path = path;
            candidate.title = title.to_owned();
        }
        Ok(found)
    }
}

impl HelpIndex {
    fn source(&self, indexed_path: &str) -> Result<(String, &'static str)> {
        let relative = Path::new(indexed_path).strip_prefix(&self.root)?;
        let path = format!("sag-help/{}", relative.to_string_lossy().replace('\\', "/"));
        let (_, _, title, _) = page(&path).context("内置文档不存在")?.1;
        Ok((path, title))
    }
}

fn page(path: &str) -> Option<(usize, &'static HelpPage)> {
    PAGES
        .iter()
        .enumerate()
        .find(|(_, (slug, locale, _, _))| path == format!("sag-help/{locale}/{slug}.md"))
}

pub fn document(path: &str) -> Result<Document> {
    let (id, (_, _, title, body)) = page(path).context("内置文档不存在")?;
    Ok(Document {
        id: id as i64 + 1,
        path: path.into(),
        source_path: None,
        name: (*title).into(),
        body: (*body).into(),
        hash: store::hash(body.as_bytes()),
    })
}

pub fn excerpts(results: &[Candidate]) -> String {
    results
        .iter()
        .take(3)
        .enumerate()
        .map(|(index, candidate)| {
            format!(
                "**{}** [{}]\n\n{}",
                candidate
                    .heading
                    .rsplit(" / ")
                    .next()
                    .unwrap_or(&candidate.title),
                index + 1,
                candidate.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

pub fn no_match(locale: &str) -> &'static str {
    match locale {
        "en" => "I couldn't find that in the built-in documentation.",
        "es" => "No encontré información sobre eso en la documentación incluida.",
        "ja" => "内蔵ドキュメントに該当する情報が見つかりませんでした。",
        "ko" => "내장 문서에서 관련 내용을 찾지 못했습니다.",
        "zh-TW" => "內建文件中找不到相關內容。",
        _ => "内置文档中没有找到相关内容。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn help_suggestions_find_the_right_documents() {
        let index = OnceCell::new();
        for (locale, question, slug) in [
            ("zh-CN", "怎么添加文件夹？", "folders"),
            ("zh-CN", "怎么配置模型？", "models"),
            ("zh-CN", "怎么在浏览器里访问？", "remote-access"),
            ("en", "How do I add a folder?", "folders"),
            ("en", "How do I configure models?", "models"),
            (
                "en",
                "How do I access SAG Wiki in a browser?",
                "remote-access",
            ),
        ] {
            let settings = Settings {
                language: locale.into(),
                ..Default::default()
            };
            let result = HelpSearch {
                index: &index,
                settings: &settings,
                jev: None,
                cancel: &CancellationToken::new(),
            }
            .search(question, |_, _| {})
            .await
            .unwrap();
            assert!(!result.trace.is_empty());
            assert!(
                result
                    .results
                    .iter()
                    .take(3)
                    .any(|result| result.path == format!("sag-help/{locale}/{slug}.md")),
                "{question}: {:?}",
                result
                    .results
                    .iter()
                    .map(|r| (&r.path, &r.heading))
                    .collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn help_finds_bilingual_release_notes_by_version() {
        let index = OnceCell::new();
        for (locale, question, notes_locale) in [
            ("zh-CN", "0.4.0 更新了什么？", "zh-CN"),
            ("en", "What changed in 0.4.0?", "en"),
            ("zh-TW", "SAG Wiki 0.4.0 有什麼更新？", "en"),
            ("ja", "SAG Wiki 0.4.0 では何が変わりましたか？", "en"),
            ("ko", "SAG Wiki v0.4.0에서 무엇이 바뀌었나요?", "en"),
            ("es", "¿Qué cambió en SAG Wiki 0.4.0?", "en"),
        ] {
            let settings = Settings {
                language: locale.into(),
                ..Default::default()
            };
            let output = HelpSearch {
                index: &index,
                settings: &settings,
                jev: None,
                cancel: &CancellationToken::new(),
            }
            .search(question, |_, _| {})
            .await
            .unwrap();
            let path = format!("sag-help/{notes_locale}/updates.md");
            let release_notes = document(&path).unwrap();
            let evidence = output.results.iter().map(|result| {
                assert_eq!(result.path, path);
                assert!(result.heading.ends_with(" / 0.4.0"));
                assert_eq!(result.hash, release_notes.hash);
                result.content.as_str()
            }).collect::<Vec<_>>().join("\n");
            let (_, section) = release_notes.body.split_once("\n## 0.4.0\n").unwrap();
            let labels: Vec<_> = section.lines().take_while(|line| !line.starts_with("## "))
                .filter_map(|line| line.strip_prefix("- **")?.split_once("**").map(|(label, _)| label))
                .collect();
            assert_eq!(labels.len(), 9);
            for label in labels {
                assert!(evidence.contains(label), "{question}: {label}");
            }
            assert!(!release_notes.body.contains("data-docs-changelog-sha"));
            assert!(!release_notes.body.contains("Generated from CHANGELOG.md"));
            assert!(release_notes.body.contains("](https://sag-wiki.zleap.com/"));
            assert!(!release_notes.body.contains("](/"));
        }
    }

    #[tokio::test]
    async fn help_citations_point_into_bundled_markdown() {
        let settings = Settings {
            language: "zh-CN".into(),
            ..Default::default()
        };
        let search = HelpSearch {
            index: &OnceCell::new(),
            settings: &settings,
            jev: None,
            cancel: &CancellationToken::new(),
        };
        let output = search
            .search("怎么在浏览器里访问？", |_, _| {})
            .await
            .unwrap();
        assert!(!output.results.is_empty());
        for result in output.results {
            let page = document(&result.path).unwrap();
            assert_eq!(page.hash, result.hash);
            assert_eq!(page.name, result.title);
            assert!(page.body.starts_with("# "));
            assert!(!page.body.contains("<Callout"));
            assert!(!page.body.contains("<ReleaseDownloads"));
            assert!(!result.heading.is_empty());
            assert!(!result.source_spans.is_empty());
            for span in result.source_spans {
                assert!(!page.body[span.start_byte..span.end_byte].trim().is_empty());
            }
        }
        assert!(document("/tmp/personal.md").is_err());
        assert!(document("sag-help/zh-CN/../en/cli.md").is_err());
        assert!(search
            .search("xyqzxxyalone987", |_, _| {})
            .await
            .unwrap()
            .results
            .is_empty());
    }
}
