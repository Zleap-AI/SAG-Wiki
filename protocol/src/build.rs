use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

pub const BUILD_VERSION: &str = env!("SAG_RELEASE_VERSION");

pub fn compare_versions(left: &str, right: &str) -> Result<Ordering> {
    let left = semver::Version::parse(left).context("SAG 版本号无效")?;
    let right = semver::Version::parse(right).context("SAG 版本号无效")?;
    Ok(left.cmp_precedence(&right))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildInfo {
    #[serde(default)]
    pub version: Option<String>,
    pub build_id: String,
    pub channel: String,
    pub protocol_version: u32,
}

impl BuildInfo {
    pub fn current() -> Self {
        Self {
            version: Some(BUILD_VERSION.into()),
            build_id: crate::BUILD_ID.into(),
            channel: crate::BUILD_CHANNEL.into(),
            protocol_version: crate::VERSION,
        }
    }

    pub fn version(&self) -> Result<&str> {
        self.version
            .as_deref()
            .context("SAG 服务未提供版本号，无法安全判断升级顺序。请更新 CLI 并重启服务。")
    }

    pub fn check_protocol(&self, client: &Self) -> Result<()> {
        if self.protocol_version != client.protocol_version {
            bail!("SAG 服务协议不兼容，请升级桌面端和 CLI；现有服务未修改")
        }
        if self.channel != client.channel {
            bail!("SAG 服务构建类型为 {}，当前客户端为 {}。请使用相同构建，或通过 SAG_DATA_DIR 隔离开发数据。", self.channel, client.channel)
        }
        Ok(())
    }

    pub fn check_compatible(&self, client: &Self) -> Result<()> {
        self.check_protocol(client)?;
        if self.build_id == client.build_id {
            return Ok(());
        }
        if self.channel != "release" {
            bail!("SAG 服务文件与当前客户端构建不同，请使用同一批开发产物")
        }
        if compare_versions(self.version()?, client.version()?)? == Ordering::Less {
            bail!(
                "SAG 服务版本 {} 低于客户端要求的 {}，请升级 CLI；现有服务未修改",
                self.version()?,
                client.version()?
            )
        }
        Ok(())
    }
}

impl From<&crate::ServiceInfo> for BuildInfo {
    fn from(info: &crate::ServiceInfo) -> Self {
        Self {
            version: info.version.clone(),
            build_id: info.build_id.clone(),
            channel: info.channel.clone(),
            protocol_version: info.protocol_version,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_order_handles_numeric_components_prereleases_and_build_metadata() {
        assert_eq!(
            compare_versions("0.10.0", "0.9.0").unwrap(),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("1.0.0-rc.1", "1.0.0").unwrap(),
            Ordering::Less
        );
        assert_eq!(
            compare_versions("1.0.0+new", "1.0.0+old").unwrap(),
            Ordering::Equal
        );
        assert!(compare_versions("unknown", "1.0.0").is_err());
    }
}
