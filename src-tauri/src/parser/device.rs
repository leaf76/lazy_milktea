use crate::types::DeviceInfo;
use regex::Regex;
use once_cell::sync::Lazy;

static RE_FP: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^\s*Build fingerprint:\s*(?P<fp>.+?)\s*$").unwrap()
});

static RE_SDK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bro\.build\.version\.sdk\s*=\s*(?P<sdk>\d+)\b").unwrap()
});

static RE_REL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bro\.build\.version\.release\s*=\s*(?P<rel>[^\s]+)\b").unwrap()
});

static RE_MODEL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bro\.product\.model\s*=\s*(?P<model>.+?)\s*$").unwrap()
});

static RE_BRAND: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bro\.product\.brand\s*=\s*(?P<brand>.+?)\s*$").unwrap()
});

static RE_BUILD_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bro\.build\.id\s*=\s*(?P<bid>[^\s]+)\b").unwrap()
});

static RE_ANR: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\bANR in\b").unwrap()
});

static RE_FATAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)FATAL EXCEPTION").unwrap()
});

static RE_TOMB: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)\btombstone\b").unwrap()
});

/// Extract device info and event counts from bugreport content
pub fn extract_device_info(content: &str) -> (DeviceInfo, usize, usize) {
    let mut device = DeviceInfo::default();
    let mut anr_count = 0usize;
    let mut crash_count = 0usize;

    for line in content.lines() {
        // Device info extraction
        if device.fingerprint.is_empty() {
            if let Some(c) = RE_FP.captures(line) {
                device.fingerprint = c["fp"].trim().trim_matches('\'').to_string();
            }
        }

        if device.android_version.is_empty() {
            if let Some(c) = RE_REL.captures(line) {
                device.android_version = c["rel"].trim().to_string();
            }
        }

        if device.api_level == 0 {
            if let Some(c) = RE_SDK.captures(line) {
                device.api_level = c["sdk"].parse().unwrap_or(0);
            }
        }

        if device.model.is_empty() {
            if let Some(c) = RE_MODEL.captures(line) {
                device.model = c["model"].trim().to_string();
            }
        }

        if device.brand.is_empty() {
            if let Some(c) = RE_BRAND.captures(line) {
                device.brand = c["brand"].trim().to_string();
            }
        }

        if device.build_id.is_empty() {
            if let Some(c) = RE_BUILD_ID.captures(line) {
                device.build_id = c["bid"].trim().to_string();
            }
        }

        // Event counting
        if RE_ANR.is_match(line) {
            anr_count += 1;
        }

        if RE_FATAL.is_match(line) || RE_TOMB.is_match(line) {
            crash_count += 1;
        }
    }

    (device, anr_count, crash_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_device_info() {
        let sample = r#"
Build fingerprint: 'google/sunfish/sunfish:13/TQ3A.230605.012/abcd:user/release-keys'
ro.build.version.release=13
ro.build.version.sdk=33
ro.product.brand=google
ro.product.model=Pixel 4a
ro.build.id=TQ3A.230605.012
--------- beginning of crash
FATAL EXCEPTION: main
ANR in com.example.app (pid 1234)
"#;

        let (device, anr_count, crash_count) = extract_device_info(sample);

        assert_eq!(device.brand, "google");
        assert_eq!(device.model, "Pixel 4a");
        assert_eq!(device.android_version, "13");
        assert_eq!(device.api_level, 33);
        assert_eq!(device.build_id, "TQ3A.230605.012");
        assert!(device.fingerprint.contains("sunfish"));
        assert!(crash_count >= 1);
        assert!(anr_count >= 1);
    }
}
