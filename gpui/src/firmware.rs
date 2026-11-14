//! Firmware information and conservative official-source checks. No firmware writes.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FirmwareInfo {
    pub versions: Vec<String>,
    pub receiver_connected: bool,
    pub receiver_version: Option<String>,
    pub details: Vec<ComponentVersion>,
}
impl FirmwareInfo {
    pub fn from_status(status: &serde_json::Value) -> Self {
        let version = status
            .get("firmwareVersion")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty());
        let versions = if let Some(version) = version {
            vec![version.to_owned()]
        } else {
            status
                .get("firmware")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str())
                .filter(|s| {
                    let text = s.to_ascii_lowercase();
                    !text.starts_with("serial:")
                        && !text.starts_with("transport:")
                        && !text.starts_with("receiver:")
                        && !text.trim().is_empty()
                })
                .map(str::to_owned)
                .collect()
        };
        Self {
            details: status
                .get("firmwareDetails")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default(),
            versions,
            receiver_connected: status
                .pointer("/firmwareContext/receiverConnected")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            receiver_version: status
                .get("receiverFirmwareVersion")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .map(str::to_owned),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ComponentVersion {
    pub component: String,
    pub prefix: String,
    pub version: String,
    pub build: u32,
    pub active: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct VerifiedRelease {
    pub device_key: String,
    pub component: String,
    pub firmware_prefix: String,
    pub version: String,
    pub build: u32,
    pub download_url: String,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Availability {
    Unknown,
    UpToDate,
    UpdateAvailable { version: String, url: String },
}
fn version_parts(version: &str) -> Option<Vec<u32>> {
    if version.is_empty() || version.len() > 48 {
        return None;
    }
    let parts = version
        .split('.')
        .map(|v| {
            if !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()) {
                v.parse().ok()
            } else {
                None
            }
        })
        .collect::<Option<Vec<u32>>>()?;
    (parts.len() >= 2 && parts.len() <= 4).then_some(parts)
}
pub fn official_firmware_download(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((host, path)) = rest.split_once('/') else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    let path = path.to_ascii_lowercase();
    let official = [
        "download01.logi.com",
        "download01.logitech.com",
        "download.logitech.com",
    ]
    .contains(&host.as_str());
    official
        && !path.contains(['\n', '\r', '\\'])
        && !path.contains("lghub")
        && !path.contains("optionsplus")
        && !path.contains("options+")
        && [".exe", ".msi", ".zip", ".bin", ".hex", ".dfu"]
            .iter()
            .any(|ext| path.split('?').next().unwrap_or("").ends_with(ext))
}
/// Only a model-matched, component-matched release from a verified catalogue
/// can enter this comparison. Product webpages are never converted to releases.
pub fn availability(
    device_key: &str,
    info: &FirmwareInfo,
    release: Option<&VerifiedRelease>,
) -> Availability {
    let Some(release) = release else {
        return Availability::Unknown;
    };
    if release.device_key != device_key || !official_firmware_download(&release.download_url) {
        return Availability::Unknown;
    }
    let Some(installed) = info.details.iter().find(|v| {
        v.active && v.component == release.component && v.prefix == release.firmware_prefix
    }) else {
        return Availability::Unknown;
    };
    let (Some(mut current), Some(mut latest)) = (
        version_parts(&installed.version),
        version_parts(&release.version),
    ) else {
        return Availability::Unknown;
    };
    let width = current.len().max(latest.len());
    current.resize(width, 0);
    latest.resize(width, 0);
    current.push(installed.build);
    latest.push(release.build);
    if latest > current {
        Availability::UpdateAvailable {
            version: format!("{} (build {})", release.version, release.build),
            url: release.download_url.clone(),
        }
    } else {
        Availability::UpToDate
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OfficialSource {
    pub article: u64,
    pub product_url: &'static str,
    /// Historical release notes, never treated as a latest-version manifest.
    pub release_notes: Option<(&'static str, &'static str)>,
}
pub fn source_for(brand: &str, name: &str) -> Option<OfficialSource> {
    if !brand.eq_ignore_ascii_case("Logitech") {
        return None;
    }
    let model = name.to_ascii_uppercase();
    if model == "G915 X" || model.starts_with("G915 X ") {
        let (article, url) = if model.contains("TKL") {
            (25605919771543,"https://support.logi.com/hc/en-us/articles/25605919771543-Getting-Started-G915-X-LIGHTSPEED-TKL")
        } else if model.contains("LIGHTSPEED") {
            (
                25605957109911,
                "https://support.logi.com/hc/en-us/articles/25605957109911-FAQ-G915-X-LIGHTSPEED",
            )
        } else {
            (
                25605873768343,
                "https://support.logi.com/hc/en-us/articles/25605873768343-Specification-G915-X",
            )
        };
        return Some(OfficialSource {
            article,
            product_url: url,
            release_notes: None,
        });
    }
    if model == "G903" || model.starts_with("G903 ") {
        return Some(OfficialSource {article:360024478554,product_url:"https://support.logi.com/hc/en-us/articles/360024478554-G903-Wired-Wireless-Gaming-Mouse",release_notes:Some(("https://support.logi.com/hc/en-us/articles/360053585533-Logitech-G-HUB","2020-08-25 — historical G903 firmware notes; not a latest-version claim"))});
    }
    None
}
#[derive(Clone, Debug)]
pub enum CheckState {
    Checking,
    Checked {
        at: std::time::Instant,
        source_updated: Option<String>,
        release: Option<VerifiedRelease>,
    },
    Failed(String),
}
#[derive(Deserialize)]
struct ArticleResponse {
    article: Article,
}
#[derive(Deserialize)]
struct Article {
    id: u64,
    html_url: String,
    #[serde(default)]
    updated_at: Option<String>,
}
/// Validate the official article response only. Never compare the surrounding
/// software release version, a different hardware variant, or an old changelog.
pub fn validate_source(bytes: &[u8], source: OfficialSource) -> Result<Option<String>, String> {
    let response: ArticleResponse = serde_json::from_slice(bytes)
        .map_err(|_| "Official source did not return usable product information.".to_string())?;
    if response.article.id != source.article
        || !response
            .article
            .html_url
            .starts_with("https://support.logi.com/hc/")
    {
        return Err("The official response did not match this product source.".into());
    }
    Ok(response
        .article
        .updated_at
        .filter(|s| {
            s.len() >= 10
                && s.as_bytes()[0..10]
                    .iter()
                    .all(|c| c.is_ascii_digit() || *c == b'-')
        })
        .map(|s| s[..10].to_owned()))
}
pub fn check(source: OfficialSource) -> CheckState {
    let url = format!(
        "https://support.logi.com/api/v2/help_center/en-us/articles/{}.json",
        source.article
    );
    let result = crate::proc::quiet(
        crate::proc::curl()
            .args([
                "-sSfL",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--max-redirs",
                "2",
                "--max-filesize",
                "2097152",
                "-m",
                "10",
            ])
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null()),
    )
    .output();
    match result {
        Ok(output) if output.status.success()=>match validate_source(&output.stdout,source) {Ok(date)=>CheckState::Checked{at:std::time::Instant::now(),source_updated:date,release:None},Err(e)=>CheckState::Failed(e)},
        _=>CheckState::Failed("Could not reach the official source. Update availability remains unknown; try again later.".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn serial_is_not_a_version_and_receiver_firmware_is_not_inferred_from_the_device() {
        let info = FirmwareInfo::from_status(
            &json!({"firmware":["Firmware: 86.00","Serial: secret","Transport: USB","Receiver: USB Receiver"],"firmwareContext":{"receiverConnected":true}}),
        );
        assert_eq!(info.versions, vec!["Firmware: 86.00"]);
        assert!(info.receiver_connected);
        assert!(info.receiver_version.is_none());
    }
    #[test]
    fn exact_keyboard_variants_have_separate_official_sources() {
        assert_ne!(
            source_for("Logitech", "G915 X"),
            source_for("Logitech", "G915 X LIGHTSPEED")
        );
        assert_ne!(
            source_for("Logitech", "G915 X LIGHTSPEED"),
            source_for("Logitech", "G915 X LIGHTSPEED TKL")
        );
        assert!(source_for("Logitech", "G915").is_none());
        assert!(source_for("Other", "G903").is_none());
    }
    #[test]
    fn old_software_release_numbers_are_never_accepted_as_latest_device_firmware() {
        let source = source_for("Logitech", "G903").unwrap();
        let data = json!({"article":{"id":source.article,"html_url":source.product_url,"updated_at":"2026-11-13T10:00:00Z","body":"Software Version: 2026.10.999. Firmware Update 7.2.13"}});
        assert_eq!(
            validate_source(&serde_json::to_vec(&data).unwrap(), source).unwrap(),
            Some("2026-11-13".into())
        );
        assert!(validate_source(
            br#"{"article":{"id":1,"html_url":"https://example.com/"}}"#,
            source
        )
        .is_err());
        assert!(validate_source(b"not json", source).is_err());
    }
}

#[cfg(test)]
mod version_comparison_tests {
    use super::*;
    fn setup() -> (FirmwareInfo, VerifiedRelease) {
        (
            FirmwareInfo {
                details: vec![ComponentVersion {
                    component: "Device firmware".into(),
                    prefix: "KBD".into(),
                    version: "1.09".into(),
                    build: 10,
                    active: true,
                }],
                ..Default::default()
            },
            VerifiedRelease {
                device_key: "046d:c359|g915 x".into(),
                component: "Device firmware".into(),
                firmware_prefix: "KBD".into(),
                version: "1.10".into(),
                build: 1,
                download_url: "https://download01.logi.com/firmware/test-device-firmware.zip"
                    .into(),
            },
        )
    }
    #[test]
    fn newer_version_or_newer_build_provides_the_verified_download() {
        let (mut info, release) = setup();
        assert!(
            matches!(availability(&release.device_key,&info,Some(&release)),Availability::UpdateAvailable{url,..} if url==release.download_url)
        );
        info.details[0].version = release.version.clone();
        info.details[0].build = 0;
        assert!(matches!(
            availability(&release.device_key, &info, Some(&release)),
            Availability::UpdateAvailable { .. }
        ));
        info.details[0].build = 1;
        assert_eq!(
            availability(&release.device_key, &info, Some(&release)),
            Availability::UpToDate
        );
        info.details[0].version = "1.11".into();
        assert_eq!(
            availability(&release.device_key, &info, Some(&release)),
            Availability::UpToDate
        );
    }
    #[test]
    fn incomplete_or_mismatched_data_stays_unknown() {
        let (mut info, mut release) = setup();
        assert_eq!(
            availability("other device", &info, Some(&release)),
            Availability::Unknown
        );
        assert_eq!(
            availability(&release.device_key, &info, None),
            Availability::Unknown
        );
        info.details[0].prefix = "OTHER".into();
        assert_eq!(
            availability(&release.device_key, &info, Some(&release)),
            Availability::Unknown
        );
        info.details[0].prefix = release.firmware_prefix.clone();
        release.version = "latest".into();
        assert_eq!(
            availability(&release.device_key, &info, Some(&release)),
            Availability::Unknown
        );
        assert!(!official_firmware_download(
            "https://download01.logi.com/lghub_installer.exe"
        ));
        assert!(!official_firmware_download(
            "https://support.logi.com/hc/en-us/articles/123"
        ));
        assert!(!official_firmware_download(
            "http://download01.logi.com/firmware.zip"
        ));
        assert!(!official_firmware_download(
            "https://download01.logi.com.evil.example/firmware.zip"
        ));
    }
}
