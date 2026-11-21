use std::{fs, path::Path};

pub fn bundle(out: &Path) {
    let directory = Path::new("../docs-site/content/docs");
    println!("cargo:rerun-if-changed=build_help.rs");
    println!("cargo:rerun-if-changed=assistant-pages.txt");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut entries = String::new();
    for slug in include_str!("assistant-pages.txt").lines() {
        for locale in ["zh-CN", "en", "zh-TW", "ja", "ko", "es"] {
            let suffix = if locale == "zh-CN" {
                String::new()
            } else {
                format!(".{locale}")
            };
            let path = directory.join(format!("{slug}{suffix}.mdx"));
            if !path.is_file() && !matches!(locale, "zh-CN" | "en") {
                continue;
            }
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let (title, markdown) =
                markdown(&source).unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            let markdown = if slug == "updates" {
                markdown
                    .replace("](/zh/docs/", "](https://sag-wiki.zleap.com/zh/docs/")
                    .replace("](/en/docs/", "](https://sag-wiki.zleap.com/en/docs/")
            } else {
                markdown
            };
            let file = out.join(format!("help-{slug}-{locale}.md"));
            fs::write(&file, markdown).unwrap();
            entries.push_str(&format!(
                "({slug:?}, {locale:?}, {title:?}, include_str!({file:?})),\n"
            ));
        }
    }
    fs::write(out.join("help-documents.rs"), format!("[{entries}]")).unwrap();
}

fn markdown(source: &str) -> Result<(&str, String), &'static str> {
    let source = source
        .strip_prefix("---\n")
        .or_else(|| source.strip_prefix("---\r\n"))
        .ok_or("missing metadata")?;
    let (metadata, body) = source
        .split_once("\n---\n")
        .or_else(|| source.split_once("\n---\r\n"))
        .ok_or("unclosed metadata")?;
    let title = metadata
        .lines()
        .find_map(|line| line.strip_prefix("title: "))
        .map(|title| title.trim_matches('"'))
        .ok_or("missing title")?;
    let mut output = format!("# {title}\n\n");
    let mut fence = None;
    for line in body.lines() {
        let trimmed = line.trim();
        let runs = |ch| trimmed.chars().take_while(|c| *c == ch).count();
        if let Some((ch, length)) = fence {
            if runs(ch) >= length && trimmed.chars().all(|c| c == ch) {
                fence = None;
            }
        } else if let Some(ch) = ['`', '~'].into_iter().find(|&ch| runs(ch) >= 3) {
            fence = Some((ch, runs(ch)));
        } else if let Some(callout) = trimmed.strip_prefix("<Callout title=\"") {
            let (label, rest) = callout.split_once("\">").ok_or("unsupported Callout")?;
            let content = rest.strip_suffix("</Callout>").unwrap_or(rest).trim();
            output.push_str(&format!("**{label}**\n\n{content}\n"));
            continue;
        } else if trimmed == "</Callout>"
            || trimmed.starts_with("<ReleaseDownloads ")
            || trimmed.starts_with("{/* Generated from CHANGELOG.md.")
            || trimmed.starts_with("<span data-docs-changelog-sha=")
            || trimmed.starts_with("![")
        {
            continue;
        } else if trimmed.starts_with('<')
            && trimmed
                .chars()
                .nth(1)
                .is_some_and(|c| c.is_ascii_uppercase())
        {
            return Err("unsupported MDX component");
        }
        output.push_str(line);
        output.push('\n');
    }
    if fence.is_some() {
        return Err("unclosed code fence");
    }
    Ok((title, output))
}
