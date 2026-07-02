//! 插件包安装/卸载/扫描（spec §2/§4）。
//! 包 = zip（.jplug/.zip），根目录须有 manifest.toml；解到 `<plugins_dir>/<id>/`。

use super::manifest::{self, Manifest};
use anyhow::{anyhow, Context, Result};
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// zip 包体积上限（压缩后）。
pub const MAX_ZIP_BYTES: usize = 32 * 1024 * 1024;
/// 解压总量上限（防 zip bomb）。
const MAX_UNPACKED_BYTES: u64 = 128 * 1024 * 1024;
const MAX_FILES: usize = 2000;

#[derive(thiserror::Error, Debug)]
pub enum InstallError {
    #[error("插件包无效: {0}")]
    BadZip(String),
    #[error("manifest 无效: {0}")]
    BadManifest(String),
    #[error("已安装相同或更高版本（{installed}）")]
    VersionConflict { installed: String },
    #[error("{0}")]
    Other(#[from] anyhow::Error),
}

/// 安装 zip 包：校验 → （版本比较）→ 解到临时目录 → 原子换入。返回解析后的 manifest。
pub fn install_zip(plugins_dir: &Path, bytes: &[u8], force: bool) -> Result<Manifest, InstallError> {
    if bytes.is_empty() {
        return Err(InstallError::BadZip("空请求体".into()));
    }
    if bytes.len() > MAX_ZIP_BYTES {
        return Err(InstallError::BadZip(format!("包超过上限 {} MiB", MAX_ZIP_BYTES / 1024 / 1024)));
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| InstallError::BadZip(e.to_string()))?;

    // 根目录 manifest.toml
    let manifest_text = {
        let mut f = zip
            .by_name("manifest.toml")
            .map_err(|_| InstallError::BadManifest("包根目录缺 manifest.toml".into()))?;
        let mut s = String::new();
        f.read_to_string(&mut s).map_err(|e| InstallError::BadZip(e.to_string()))?;
        s
    };
    let m = manifest::parse(&manifest_text).map_err(|e| InstallError::BadManifest(e.to_string()))?;

    // 同 id 重装：更高版本覆盖；相等或更低须 force（spec §4）
    let target = plugins_dir.join(&m.id);
    if let Ok(old_text) = std::fs::read_to_string(target.join("manifest.toml")) {
        if let Ok(old) = manifest::parse(&old_text) {
            if manifest::cmp_version(&m.version, &old.version) != std::cmp::Ordering::Greater && !force {
                return Err(InstallError::VersionConflict { installed: old.version });
            }
        }
    }

    // 解到临时目录（zip-slip 防护 + 体量上限），成功后整目录换入
    let tmp = plugins_dir.join(format!(".tmp-{}", m.id));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).context("创建临时目录失败")?;
    let extracted = extract_all(&mut zip, &tmp);
    if let Err(e) = extracted {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let _ = std::fs::remove_dir_all(&target);
    std::fs::rename(&tmp, &target).context("安装目录换入失败")?;
    Ok(m)
}

fn extract_all(zip: &mut zip::ZipArchive<Cursor<&[u8]>>, dest: &Path) -> Result<(), InstallError> {
    if zip.len() > MAX_FILES {
        return Err(InstallError::BadZip(format!("文件数超过上限 {MAX_FILES}")));
    }
    let mut total: u64 = 0;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| InstallError::BadZip(e.to_string()))?;
        // enclosed_name 已拒绝 ..、绝对路径等 zip-slip 形态；拿不到 = 恶意条目
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| InstallError::BadZip(format!("非法路径条目: {:?}", entry.name())))?;
        let out = dest.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|e| InstallError::Other(e.into()))?;
            continue;
        }
        total = total.saturating_add(entry.size());
        if total > MAX_UNPACKED_BYTES {
            return Err(InstallError::BadZip("解压总量超过上限".into()));
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|e| InstallError::Other(e.into()))?;
        }
        let mut w = std::fs::File::create(&out).map_err(|e| InstallError::Other(e.into()))?;
        std::io::copy(&mut entry, &mut w).map_err(|e| InstallError::Other(e.into()))?;
    }
    Ok(())
}

/// 卸载：删除插件目录（状态/设置由调用方清理）。
pub fn uninstall(plugins_dir: &Path, id: &str) -> Result<()> {
    if !manifest::is_valid_id(id) {
        return Err(anyhow!("非法插件 id"));
    }
    let dir = plugins_dir.join(id);
    if dir.exists() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("删除 {dir:?} 失败"))?;
    }
    Ok(())
}

/// 启动扫描：列出插件目录下的所有子目录及其 manifest 解析结果。
/// 坏 manifest 不隐身——以 Err 返回，宿主标记 error 展示给用户。
pub fn scan(plugins_dir: &Path) -> Vec<(String, PathBuf, Result<Manifest>)> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(plugins_dir) else {
        return out;
    };
    for e in entries.flatten() {
        let dir = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if !dir.is_dir() || name.starts_with('.') {
            continue;
        }
        let manifest = std::fs::read_to_string(dir.join("manifest.toml"))
            .map_err(|e| anyhow!("读 manifest.toml 失败: {e}"))
            .and_then(|text| manifest::parse(&text))
            .and_then(|m| {
                if m.id != name {
                    Err(anyhow!("manifest.id ({}) 与目录名 ({name}) 不一致", m.id))
                } else {
                    Ok(m)
                }
            });
        out.push((name, dir, manifest));
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn make_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        let mut w = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            w.start_file(*name, opts).unwrap();
            w.write_all(content.as_bytes()).unwrap();
        }
        w.finish().unwrap();
        buf.into_inner()
    }

    const MANIFEST_V1: &str =
        "id = \"demo\"\nname = \"Demo\"\nversion = \"1.0.0\"\napiVersion = \"0.2\"\n";
    const MANIFEST_V2: &str =
        "id = \"demo\"\nname = \"Demo\"\nversion = \"2.0.0\"\napiVersion = \"0.2\"\n";

    #[test]
    fn installs_and_upgrades() {
        let dir = tempfile::tempdir().unwrap();
        let z1 = make_zip(&[("manifest.toml", MANIFEST_V1), ("assets/a.css", "body{}")]);
        let m = install_zip(dir.path(), &z1, false).unwrap();
        assert_eq!(m.version, "1.0.0");
        assert!(dir.path().join("demo/assets/a.css").exists());

        // 同版本重装 → 409 语义
        assert!(matches!(
            install_zip(dir.path(), &z1, false),
            Err(InstallError::VersionConflict { .. })
        ));
        // force 覆盖
        install_zip(dir.path(), &z1, true).unwrap();
        // 升级正常
        let z2 = make_zip(&[("manifest.toml", MANIFEST_V2)]);
        assert_eq!(install_zip(dir.path(), &z2, false).unwrap().version, "2.0.0");
        // 升级后旧文件不残留（整目录换入）
        assert!(!dir.path().join("demo/assets/a.css").exists());
    }

    #[test]
    fn rejects_zip_slip_and_missing_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let evil = make_zip(&[("manifest.toml", MANIFEST_V1), ("../evil.txt", "x")]);
        assert!(matches!(install_zip(dir.path(), &evil, false), Err(InstallError::BadZip(_))));
        assert!(!dir.path().join("demo").exists()); // 失败不留残余

        let none = make_zip(&[("readme.md", "x")]);
        assert!(matches!(install_zip(dir.path(), &none, false), Err(InstallError::BadManifest(_))));
    }

    #[test]
    fn scan_reports_broken_manifest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("ok")).unwrap();
        std::fs::write(
            dir.path().join("ok/manifest.toml"),
            "id = \"ok\"\nname = \"OK\"\nversion = \"1\"\napiVersion = \"0.2\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("broken")).unwrap();
        std::fs::write(dir.path().join("broken/manifest.toml"), "not toml [").unwrap();
        std::fs::create_dir_all(dir.path().join("mismatch")).unwrap();
        std::fs::write(
            dir.path().join("mismatch/manifest.toml"),
            "id = \"other\"\nname = \"X\"\nversion = \"1\"\napiVersion = \"0.2\"\n",
        )
        .unwrap();

        let scanned = scan(dir.path());
        assert_eq!(scanned.len(), 3);
        assert!(scanned.iter().find(|(n, ..)| n == "ok").unwrap().2.is_ok());
        assert!(scanned.iter().find(|(n, ..)| n == "broken").unwrap().2.is_err());
        assert!(scanned.iter().find(|(n, ..)| n == "mismatch").unwrap().2.is_err());
    }
}
