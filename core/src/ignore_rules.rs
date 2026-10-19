use anyhow::{Context, Result};
use ignore::{IncrementalIgnore, WalkBuilder};
use std::path::Path;

pub fn builder(root: &Path) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);
    builder.standard_filters(false)
        .parents(false)
        .git_ignore(true)
        .require_git(false)
        .add_custom_ignore_filename(".sagignore");
    builder
}

pub fn matcher(root: &Path) -> IncrementalIgnore {
    builder(root).build_matchers().pop().unwrap()
}

pub fn is_ignored(matcher: &mut IncrementalIgnore, path: &Path, directory: bool) -> Result<bool> {
    let relative = path.strip_prefix(matcher.root())?;
    let (matched, error) = matcher.matched_with_errors(relative, directory);
    if let Some(error) = error { return Err(error).context("无法读取项目忽略规则"); }
    Ok(matched.is_ignore())
}

pub fn is_rule_file(path: &Path) -> bool {
    path.file_name().is_some_and(|name| name == ".gitignore" || name == ".sagignore")
}
