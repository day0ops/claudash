use crate::types::Credentials;
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Resolved credential info.
pub struct CredentialInfo {
    pub access_token: String,
    pub plan_name: String,
}

/// Read OAuth credentials: try macOS Keychain first, then fall back to file.
pub fn read_credentials() -> Option<CredentialInfo> {
    // Try macOS Keychain first
    if let Some(info) = read_from_keychain() {
        return Some(info);
    }

    // Fall back to .credentials.json
    read_from_file()
}

/// Map subscription type string to display name.
fn plan_display_name(sub_type: &str) -> &'static str {
    let lower = sub_type.to_lowercase();
    if lower.contains("max") {
        "Max"
    } else if lower.contains("enterprise") {
        "Enterprise"
    } else if lower.contains("team") {
        "Team"
    } else if lower.contains("pro") {
        "Pro"
    } else {
        "Free"
    }
}

/// Compute the Keychain service name, accounting for CLAUDE_CONFIG_DIR.
fn keychain_service_name() -> String {
    const BASE: &str = "Claude Code-credentials";
    match env::var("CLAUDE_CONFIG_DIR") {
        Ok(dir) if !dir.is_empty() => {
            let hash = sha256_bytes(dir.as_bytes());
            // First 4 bytes = 8 hex chars
            let suffix: String = hash[..4].iter().map(|b| format!("{:02x}", b)).collect();
            format!("{}-{}", BASE, suffix)
        }
        _ => BASE.to_string(),
    }
}

/// Minimal SHA-256 implementation (no external crate).
/// Only used for short inputs (config dir path).
fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    // Initial hash values
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Round constants
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    // Pre-processing: pad message
    let bit_len = (data.len() as u64) * 8;
    let mut msg = data.to_vec();
    msg.push(0x80);
    while (msg.len() % 64) != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bit_len.to_be_bytes());

    // Process each 64-byte block
    for block in msg.chunks_exact(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;

        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let temp1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = s0.wrapping_add(maj);

            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        h[0] = h[0].wrapping_add(a);
        h[1] = h[1].wrapping_add(b);
        h[2] = h[2].wrapping_add(c);
        h[3] = h[3].wrapping_add(d);
        h[4] = h[4].wrapping_add(e);
        h[5] = h[5].wrapping_add(f);
        h[6] = h[6].wrapping_add(g);
        h[7] = h[7].wrapping_add(hh);
    }

    let mut result = [0u8; 32];
    for (i, val) in h.iter().enumerate() {
        result[i * 4..i * 4 + 4].copy_from_slice(&val.to_be_bytes());
    }
    result
}

/// Try to read credentials from macOS Keychain.
fn read_from_keychain() -> Option<CredentialInfo> {
    let service = keychain_service_name();
    let output = Command::new("/usr/bin/security")
        .args(["find-generic-password", "-s", &service, "-w"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json_str = String::from_utf8(output.stdout).ok()?;
    let json_str = json_str.trim();
    parse_credentials_json(json_str)
}

/// Try to read credentials from file.
fn read_from_file() -> Option<CredentialInfo> {
    let path = credentials_file_path();
    let contents = std::fs::read_to_string(path).ok()?;
    parse_credentials_json(&contents)
}

/// Parse credential JSON string into CredentialInfo.
fn parse_credentials_json(json: &str) -> Option<CredentialInfo> {
    let creds: Credentials = serde_json::from_str(json).ok()?;
    let oauth = creds.claude_ai_oauth?;
    let token = oauth.access_token.filter(|t| !t.is_empty())?;
    let sub_type = oauth.subscription_type.unwrap_or_default();
    Some(CredentialInfo {
        access_token: token,
        plan_name: plan_display_name(&sub_type).to_string(),
    })
}

/// Determine credentials file path.
fn credentials_file_path() -> PathBuf {
    if let Ok(dir) = env::var("CLAUDE_CONFIG_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join(".credentials.json");
        }
    }
    let home = env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home)
        .join(".claude")
        .join(".credentials.json")
}

/// Get the config dir hash suffix (empty string if no custom config dir).
pub fn config_dir_hash_suffix() -> String {
    match env::var("CLAUDE_CONFIG_DIR") {
        Ok(dir) if !dir.is_empty() => {
            let hash = sha256_bytes(dir.as_bytes());
            let suffix: String = hash[..4].iter().map(|b| format!("{:02x}", b)).collect();
            format!("-{}", suffix)
        }
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plan_display_name() {
        assert_eq!(plan_display_name("pro"), "Pro");
        assert_eq!(plan_display_name("max_5x"), "Max");
        assert_eq!(plan_display_name("enterprise"), "Enterprise");
        assert_eq!(plan_display_name("team_pro"), "Team");
        assert_eq!(plan_display_name(""), "Free");
    }

    #[test]
    fn test_sha256_known_value() {
        // SHA256("abc") = ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad
        let result = sha256_bytes(b"abc");
        assert_eq!(result[0], 0xba);
        assert_eq!(result[1], 0x78);
        assert_eq!(result[2], 0x16);
        assert_eq!(result[3], 0xbf);
    }

    #[test]
    fn test_keychain_service_name_default() {
        // Only valid when CLAUDE_CONFIG_DIR is unset
        env::remove_var("CLAUDE_CONFIG_DIR");
        assert_eq!(keychain_service_name(), "Claude Code-credentials");
    }

    #[test]
    fn test_parse_credentials_json() {
        let json = r#"{"claudeAiOauth":{"accessToken":"tok123","subscriptionType":"pro"}}"#;
        let info = parse_credentials_json(json).unwrap();
        assert_eq!(info.access_token, "tok123");
        assert_eq!(info.plan_name, "Pro");
    }

    #[test]
    fn test_parse_credentials_json_missing_token() {
        let json = r#"{"claudeAiOauth":{"accessToken":"","subscriptionType":"pro"}}"#;
        assert!(parse_credentials_json(json).is_none());
    }
}
