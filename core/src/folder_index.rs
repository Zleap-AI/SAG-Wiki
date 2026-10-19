use anyhow::{bail, Context, Result};
use sag_protocol::types::{FolderIndexOptions, Settings};
use std::{collections::HashMap, fs, io::{ErrorKind, Write}, path::{Path, PathBuf}};

const START: &str = "# SAG Wiki index options: begin";
const END: &str = "# SAG Wiki index options: end";

pub struct Rules {
    root: PathBuf,
    directories: HashMap<PathBuf, FolderIndexOptions>,
}

impl Rules {
    pub fn new(root: &Path, settings: &Settings) -> Result<Self> {
        let options = read(root)?.unwrap_or_else(|| FolderIndexOptions {
            file_types: settings.file_types.clone(), convert_other_formats: settings.convert_other_formats,
            conversion_formats: settings.conversion_formats.clone(),
        });
        Ok(Self { root: root.into(), directories: HashMap::from([(root.into(), options)]) })
    }
    pub fn directory(&mut self, directory: &Path) -> Result<&FolderIndexOptions> {
        if !self.directories.contains_key(directory) {
            let options = match read(directory)? {
                Some(options) => options,
                None => self.directory(directory.parent().unwrap())?.clone(),
            };
            self.directories.insert(directory.into(), options);
        }
        Ok(&self.directories[directory])
    }
    pub fn file(&mut self, path: &Path) -> Result<&FolderIndexOptions> {
        let directory = match path.strip_prefix(self.root.join(crate::docling::OUTPUT_DIRECTORY)) {
            Ok(relative) => self.root.join(relative.parent().unwrap()),
            Err(_) => path.parent().unwrap().into(),
        };
        self.directory(&directory)
    }
    pub fn converts_files(&self) -> bool {
        self.directories.values().any(|options| options.convert_other_formats)
    }
}

fn content(root: &Path) -> Result<String> {
    match fs::read_to_string(root.join(".sagignore")) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error).context("无法读取 .sagignore"),
    }
}

fn block(content: &str) -> Result<Option<std::ops::Range<usize>>> {
    let start = content.find(START);
    let end = content.find(END);
    match (start, end) {
        (None, None) => Ok(None),
        (Some(start), Some(end)) if start < end => Ok(Some(start..end + END.len())),
        _ => bail!(".sagignore 中的索引选项标记不完整，请先修复该文件"),
    }
}

pub fn read(root: &Path) -> Result<Option<FolderIndexOptions>> {
    let content = content(root)?;
    let Some(range) = block(&content)? else { return Ok(None); };
    let json = content[range].lines().nth(1).context(".sagignore 中缺少索引选项")?;
    let json = json.strip_prefix("# ").context(".sagignore 中的索引选项格式不正确")?;
    Ok(Some(serde_json::from_str(json).context("无法解析 .sagignore 中的索引选项")?))
}

pub fn write(root: &Path, options: &FolderIndexOptions) -> Result<()> {
    let path = root.join(".sagignore");
    let mut content = content(root)?;
    let mut replacement = format!("{START}\n# {}\n", serde_json::to_string(options)?);
    replacement.push_str(END);
    if let Some(range) = block(&content)? { content.replace_range(range, &replacement); }
    else {
        if !content.is_empty() && !content.ends_with('\n') { content.push('\n'); }
        content.push_str(&replacement);
        content.push('\n');
    }
    let mut file = tempfile::NamedTempFile::new_in(root)?;
    if path.exists() { file.as_file().set_permissions(fs::metadata(&path)?.permissions())?; }
    file.write_all(content.as_bytes())?;
    file.persist(path).map_err(|error| error.error).context("无法保存 .sagignore")?;
    Ok(())
}
