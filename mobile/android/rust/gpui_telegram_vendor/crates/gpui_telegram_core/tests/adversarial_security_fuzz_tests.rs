//! Adversarial Security & Fuzzing Verification Harness for `gpui_telegram_core::core`
//! Subsystems tested:
//! 1. Deep Links URL parser under fuzz vectors (malicious queries, shell metacharacters, unknown schemes, path traversal).
//! 2. Bank card Luhn algorithm validation fuzzing (valid Visa/Mastercard/Amex, single-digit corruptions, transpositions, invalid lengths).
//! 3. MIME type safety, IP-revealing extensions, and quarantine classification.
//! 4. Updater verification anti-tamper (invalid magic, target mismatch, rollback rejection, payload bounds) & Packer integrity.

use gpui_telegram_core::core::bank_card_click_handler::BankCardClickHandler;
use gpui_telegram_core::core::deep_links::{
    DeepLinkResult, DeepLinksRouterDeepLinks,
};
use gpui_telegram_core::core::file_utilities::FileUtilities;
use gpui_telegram_core::core::mime_type::{
    detect_name_type, is_ip_revealing_path, mime_type_for_data, KnownMimeType, NameType,
};
use gpui_telegram_core::core::packer::Packer;
use gpui_telegram_core::core::update_channel::UpdateChannel;
use gpui_telegram_core::core::update_verify::{
    TargetArch, TargetOs, UpdateTarget, UpdateVerify, ENVELOPE_FORMAT, ENVELOPE_MAGIC,
};
use sha2::{Digest, Sha256};

// ============================================================================
// 1. Deep Links URL Parser Adversarial Fuzzing
// ============================================================================

#[test]
fn test_deep_links_fuzzing_malicious_queries_and_shell_metacharacters() {
    let router = DeepLinksRouterDeepLinks::default();

    // Adversarial vector list: shell injection, metacharacters, null bytes, bidirectional overrides, huge queries
    let vectors = [
        // Shell commands & injections
        "tg://resolve?domain=$(reboot)&post=1;whoami;",
        "tg://resolve?domain=test`cat /etc/passwd`&post=1",
        "tg://resolve?domain=test|nc -l 1337&post=1",
        "tg://resolve?domain=test&&rm -rf /&post=1",
        "tg://resolve?domain=test\n/bin/sh -c 'echo hacked'",
        "tg://resolve?domain=test\r\nevil=1",
        // Null bytes & control characters
        "tg://resolve?domain=user\0hidden&post=1",
        "tg://resolve?domain=\u{202E}gnp.elif\u{202D}&post=1", // RTL override
        // URL percent encoding malformations
        "tg://resolve?domain=foo%&post=1",
        "tg://resolve?domain=foo%%&post=1",
        "tg://resolve?domain=foo%ZZ&post=1",
        "tg://resolve?domain=foo%0&post=1",
        "tg://resolve?domain=foo%00bar&post=1",
        "tg://resolve?domain=hello+world%20test",
        // Edge delimiters
        "tg://resolve?&&&&&",
        "tg://resolve?key=",
        "tg://resolve?=value",
        "tg://resolve?===&&&===",
        // Missing or weird paths
        "tg://",
        "tg:///",
        "tg:////",
        "tg:resolve",
        "   ",
        "",
    ];

    for &vec in &vectors {
        // Must parse safely without panicking
        let ctx = DeepLinksRouterDeepLinks::parse_command(vec, true);

        // Dispatch must never crash
        let (result, _) = router.dispatch(&ctx);
        // None of these match the default router's internal registered commands
        assert!(matches!(
            result,
            DeepLinkResult::NotFound | DeepLinkResult::Unsupported
        ));
    }

    // Fuzz with large query payload (64 KB)
    let huge_domain = "A".repeat(65536);
    let huge_url = format!("tg://resolve?domain={}&param=1", huge_domain);
    let ctx_huge = DeepLinksRouterDeepLinks::parse_command(&huge_url, true);
    assert_eq!(ctx_huge.section, "resolve");
    assert_eq!(ctx_huge.param("domain").unwrap().len(), 65536);

    // Fuzz with 1,000 distinct query keys
    let mut multi_param_url = "tg://resolve?".to_string();
    for i in 0..1000 {
        if i > 0 {
            multi_param_url.push('&');
        }
        multi_param_url.push_str(&format!("k_{}={}", i, i * 2));
    }
    let ctx_multi = DeepLinksRouterDeepLinks::parse_command(&multi_param_url, true);
    assert_eq!(ctx_multi.params.len(), 1000);
    assert_eq!(ctx_multi.param("k_500"), Some("1000"));
}

#[test]
fn test_deep_links_unknown_schemes_and_path_traversal() {
    let router = DeepLinksRouterDeepLinks::default();

    // Unknown schemes must not be handled
    let unknown_schemes = [
        "http://telegram.org/resolve?domain=telegram",
        "https://t.me/telegram",
        "javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==",
        "smb://192.168.1.1/share",
        "ftp://malicious.org/script.sh",
    ];

    for &scheme in &unknown_schemes {
        assert!(!router.try_handle(scheme, true));
        let ctx = DeepLinksRouterDeepLinks::parse_command(scheme, true);
        let (res, _) = router.dispatch(&ctx);
        assert_eq!(res, DeepLinkResult::NotFound);
    }

    // Path traversal in tg:// URLs
    let traversals = [
        "tg://../../../../etc/passwd",
        "tg://settings/../../../../etc/shadow",
        "tg://chats/../settings/privacy",
        "tg://%2e%2e%2f%2e%2e%2f",
        "tg://..\\..\\windows\\system32",
    ];

    for &trav in &traversals {
        let ctx = DeepLinksRouterDeepLinks::parse_command(trav, true);
        let (res, _) = router.dispatch(&ctx);
        assert!(
            matches!(res, DeepLinkResult::NotFound | DeepLinkResult::Unsupported),
            "Traversal {} unexpectedly matched",
            trav
        );
    }

    // Test genuine routes and auth gating
    // 1. Auth required, no active session
    let (res_no_auth, skip1) = router.dispatch(&DeepLinksRouterDeepLinks::parse_command(
        "tg://chats/search",
        false,
    ));
    assert_eq!(res_no_auth, DeepLinkResult::NeedsAuth);
    assert!(skip1); // chats/search has skip_activation: true

    // 2. Auth required, active session present
    let (res_auth, skip2) =
        router.dispatch(&DeepLinksRouterDeepLinks::parse_command("tg://chats/search", true));
    assert_eq!(res_auth, DeepLinkResult::Handled);
    assert!(skip2);

    // 3. Settings route
    let (res_settings, _) = router.dispatch(&DeepLinksRouterDeepLinks::parse_command(
        "tg://settings/privacy",
        true,
    ));
    assert_eq!(res_settings, DeepLinkResult::Handled);
}

// ============================================================================
// 2. Bank Card Luhn Algorithm Validation Fuzzing
// ============================================================================

#[test]
fn test_bank_card_luhn_valid_networks_and_formatting() {
    struct CardCase {
        number: &'static str,
        expected_network: &'static str,
        expected_formatted: &'static str,
    }

    let valid_cards = [
        CardCase {
            number: "4532 0151 1283 0366",
            expected_network: "Visa",
            expected_formatted: "4532 0151 1283 0366",
        },
        CardCase {
            number: "5105105105105100",
            expected_network: "Mastercard",
            expected_formatted: "5105 1051 0510 5100",
        },
        CardCase {
            number: "378282246310005",
            expected_network: "American Express",
            expected_formatted: "3782 8224 6310 005",
        },
        CardCase {
            number: "6011 0009 9013 9424",
            expected_network: "Discover",
            expected_formatted: "6011 0009 9013 9424",
        },
        CardCase {
            number: "6200000000000005",
            expected_network: "UnionPay",
            expected_formatted: "6200 0000 0000 0005",
        },
    ];

    for case in &valid_cards {
        let handler = BankCardClickHandler::new(case.number);
        assert!(
            handler.is_valid_luhn(),
            "Card {} failed Luhn check",
            case.number
        );
        assert_eq!(handler.card_network(), case.expected_network);
        assert_eq!(handler.formatted_card_number(), case.expected_formatted);
        assert!(handler.tooltip().contains(case.expected_network));
    }

    // REMEDIATED (VULN-M01-LUHN-01):
    // Mir check (`starts_with("220")`) is evaluated BEFORE Mastercard ('22') check.
    // Mir card starting with 220 is correctly identified as "Mir".
    let mir_handler = BankCardClickHandler::new("2200000000000004");
    assert!(mir_handler.is_valid_luhn());
    assert_eq!(
        mir_handler.card_network(),
        "Mir",
        "Mir card 2200 must be correctly identified as Mir"
    );
}

#[test]
fn test_bank_card_luhn_single_digit_corruption_adversarial_oracle() {
    let test_seeds = [
        "4532015112830366", // Visa 16
        "5105105105105100", // Mastercard 16
        "378282246310005",  // Amex 15
        "6011000990139424", // Discover 16
        "2200000000000004", // Mir 16
        "6200000000000005", // UnionPay 16
    ];

    let mut total_corruptions_tested = 0;

    for &seed in &test_seeds {
        // Assert initial seed is valid
        assert!(
            BankCardClickHandler::new(seed).is_valid_luhn(),
            "Seed {} must be valid",
            seed
        );

        let chars: Vec<char> = seed.chars().collect();
        let len = chars.len();

        for i in 0..len {
            let original_char = chars[i];
            let original_digit = original_char.to_digit(10).unwrap();

            // Try all 9 alternative digits
            for d in 0..10 {
                if d == original_digit {
                    continue;
                }
                let mut mutated = chars.clone();
                mutated[i] = char::from_digit(d, 10).unwrap();
                let mutated_str: String = mutated.into_iter().collect();

                let handler = BankCardClickHandler::new(&mutated_str);
                // In Luhn mod-10, ANY single-digit corruption is guaranteed to produce sum % 10 != 0
                assert!(
                    !handler.is_valid_luhn(),
                    "Luhn failed to catch single-digit error at index {} on seed {}: mutated to {}",
                    i,
                    seed,
                    mutated_str
                );
                total_corruptions_tested += 1;
            }
        }
    }

    assert!(
        total_corruptions_tested >= 850,
        "Expected at least 850 single-digit corruption checks, ran {}",
        total_corruptions_tested
    );
}

#[test]
fn test_bank_card_luhn_boundary_lengths_and_dirty_inputs() {
    // Length boundary tests (valid range: 12..=19)
    assert!(!BankCardClickHandler::new("").is_valid_luhn());
    assert!(!BankCardClickHandler::new("4").is_valid_luhn());
    assert!(!BankCardClickHandler::new("45320151128").is_valid_luhn()); // 11 digits

    // 20 digits (exceeds max length)
    let too_long = "45320151128303660000";
    assert!(!BankCardClickHandler::new(too_long).is_valid_luhn());

    // Non-digit dirty inputs (constructor must filter non-digits)
    let dirty = "💳 Card: 4532 - 0151 - 1283 - 0366 (verified)";
    let handler_dirty = BankCardClickHandler::new(dirty);
    assert_eq!(handler_dirty.card_number(), "4532015112830366");
    assert!(handler_dirty.is_valid_luhn());
    assert_eq!(handler_dirty.card_network(), "Visa");

    // Pure non-numeric garbage
    let garbage = BankCardClickHandler::new("!@#$%^&*()_+ abcdef");
    assert_eq!(garbage.card_number(), "");
    assert!(!garbage.is_valid_luhn());
    assert_eq!(garbage.card_network(), "Unknown");

    // Extreme string (50,000 garbage characters + valid card)
    let huge_dirty = format!("{}4532015112830366", "x".repeat(50000));
    let handler_huge = BankCardClickHandler::new(huge_dirty);
    assert_eq!(handler_huge.card_number(), "4532015112830366");
    assert!(handler_huge.is_valid_luhn());
}

// ============================================================================
// 3. MIME Type Safety & Quarantine Classifications
// ============================================================================

#[test]
fn test_mime_executable_classification_and_double_extension_evasion() {
    // Dangerous executable extensions
    let dangerous = [
        "malware.exe",
        "script.bat",
        "command.cmd",
        "payload.com",
        "installer.msi",
        "macro.vbs",
        "powershell.ps1",
        "registry.reg",
        "screensaver.scr",
        "shortcut.lnk",
        "library.dll",
        "control.cpl",
        "bash.sh",
        "binary.bin",
        "launcher.run",
        "application.app",
        "terminal.command",
        "automator.workflow",
        "applescript.applescript",
        "package.pkg",
        "debian.deb",
        "redhat.rpm",
        "csh.csh",
        "ksh.ksh",
        "output.out",
    ];

    for file in dangerous {
        assert_eq!(
            detect_name_type(file),
            NameType::Executable,
            "File {} was not classified as Executable",
            file
        );
    }

    // Double extension evasion attacks: attackers name files "invoice.pdf.exe" or "photo.jpg.scr"
    let double_exts = [
        "invoice.pdf.exe",
        "statement.xlsx.scr",
        "photo.png.bat",
        "document.docx.sh",
        "archive.zip.cmd",
        "update.tar.gz.app",
    ];

    for file in double_exts {
        assert_eq!(
            detect_name_type(file),
            NameType::Executable,
            "Double extension evasion attack succeeded for {}",
            file
        );
    }

    // Case insensitivity
    assert_eq!(detect_name_type("EVIL.EXE"), NameType::Executable);
    assert_eq!(detect_name_type("Test.BaT"), NameType::Executable);
    assert_eq!(detect_name_type("Run.ScR"), NameType::Executable);
    assert_eq!(detect_name_type("Hook.DLL"), NameType::Executable);

    // Benign classifications
    assert_eq!(detect_name_type("photo.jpg"), NameType::Image);
    assert_eq!(detect_name_type("image.webp"), NameType::Image);
    assert_eq!(detect_name_type("movie.mp4"), NameType::Video);
    assert_eq!(detect_name_type("song.mp3"), NameType::Audio);
    assert_eq!(detect_name_type("report.pdf"), NameType::Document);
    assert_eq!(detect_name_type("backup.tar.gz"), NameType::Archive);
    assert_eq!(detect_name_type("custom.tdesktop-theme"), NameType::ThemeFile);
}

#[test]
fn test_mime_ip_revealing_paths_and_magic_bytes() {
    // IP-revealing formats (files that could load remote assets when rendered)
    let ip_leaking = [
        "page.html",
        "index.htm",
        "vector.svg",
        "stream.m3u8",
        "document.docx",
        "spreadsheet.xlsx",
        "presentation.pptx",
        "earth.kml",
        "map.kmz",
    ];

    for file in ip_leaking {
        assert!(
            is_ip_revealing_path(file),
            "{} should be flagged as IP-revealing",
            file
        );
    }

    // Safe benign files must not be flagged
    assert!(!is_ip_revealing_path("image.png"));
    assert!(!is_ip_revealing_path("audio.mp3"));
    assert!(!is_ip_revealing_path("video.mp4"));
    assert!(!is_ip_revealing_path("plain.txt"));

    // Magic bytes sniffing fuzzing
    let webp_magic = b"RIFF\x00\x00\x00\x00WEBPVP8 ";
    assert_eq!(mime_type_for_data(webp_magic).known, KnownMimeType::WebP);

    let tgs_magic = b"\x1f\x8b\x08\x00\x00\x00\x00\x00";
    assert_eq!(mime_type_for_data(tgs_magic).known, KnownMimeType::Tgs);

    let pdf_magic = b"%PDF-1.7\r\n";
    assert_eq!(mime_type_for_data(pdf_magic).raw_mime, "application/pdf");

    let png_magic = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR";
    assert_eq!(mime_type_for_data(png_magic).raw_mime, "image/png");

    let jpeg_magic = b"\xff\xd8\xff\xe0\x00\x10JFIF";
    assert_eq!(mime_type_for_data(jpeg_magic).raw_mime, "image/jpeg");

    // Empty and truncated inputs must not panic
    assert_eq!(mime_type_for_data(&[]).known, KnownMimeType::Unknown);
    assert_eq!(mime_type_for_data(&[0x52]).known, KnownMimeType::Unknown);
    assert_eq!(mime_type_for_data(b"RIFF").known, KnownMimeType::Unknown);
}

#[test]
fn test_file_utilities_auth_token_sanitization() {
    // Sanitizes web auth tokens from URLs
    let raw_url = "https://t.me/testbot?auth=SECRET_TOKEN_12345&chat_id=100&token=LEAK";
    let cleaned = FileUtilities::url_without_web_auth_tokens(raw_url);
    assert_eq!(cleaned, "https://t.me/testbot?chat_id=100");

    let raw_tg_token = "https://web.telegram.org/?tgauthtoken=ABC&webauthtoken=XYZ&valid=true";
    let cleaned_tg = FileUtilities::url_without_web_auth_tokens(raw_tg_token);
    assert_eq!(cleaned_tg, "https://web.telegram.org/?valid=true");

    let clean_url = "https://t.me/username";
    assert_eq!(
        FileUtilities::url_without_web_auth_tokens(clean_url),
        clean_url
    );
}

// ============================================================================
// 4. Updater Verification Anti-Tamper & Rollback Rejection
// ============================================================================

/// Helper function to build a binary update envelope per `UpdateVerify` spec
fn build_update_envelope(
    magic: &[u8; 4],
    format: u32,
    os: u8,
    arch: u8,
    version: u64,
    channel: u8,
    payload: &[u8],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24 + payload.len());
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&format.to_le_bytes());
    bytes.push(os);
    bytes.push(arch);
    bytes.extend_from_slice(&version.to_le_bytes());
    bytes.push(channel);
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.push(0u8); // pad byte 23
    bytes.extend_from_slice(payload);
    bytes
}

#[test]
fn test_update_verify_magic_and_envelope_anti_tamper() {
    let payload = b"UPDATED_BINARY_DATA";
    let target = UpdateTarget {
        os: TargetOs::Mac,
        arch: TargetArch::Arm,
    };
    let running_version = 1000u64;
    let new_version = 1001u64;

    let valid_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        1, // Mac
        2, // Arm
        new_version,
        0, // Stable
        payload,
    );

    // 1. Untampered valid envelope must pass
    let res = UpdateVerify::parse_and_verify(&valid_envelope, target, running_version);
    assert!(res.is_ok());
    let verified = res.unwrap();
    assert_eq!(verified.version, new_version);
    assert_eq!(verified.channel, UpdateChannel::Stable);
    assert_eq!(verified.payload, payload);

    // 2. Corrupt magic: 1st, 2nd, 3rd, 4th byte mutations
    for i in 0..4 {
        let mut corrupted = valid_envelope.clone();
        corrupted[i] ^= 0xFF;
        let err = UpdateVerify::parse_and_verify(&corrupted, target, running_version);
        assert_eq!(err, Err("Invalid envelope magic (expected TDUP)"));
    }

    // 3. Corrupt envelope format version
    let bad_formats = [0u32, 1, 3, 999, u32::MAX];
    for bad_fmt in bad_formats {
        let corrupt_fmt = build_update_envelope(
            ENVELOPE_MAGIC,
            bad_fmt,
            1,
            2,
            new_version,
            0,
            payload,
        );
        let err = UpdateVerify::parse_and_verify(&corrupt_fmt, target, running_version);
        assert_eq!(err, Err("Unsupported envelope format"));
    }

    // 4. Truncated envelope (< 24 bytes)
    for len in 0..24 {
        let err = UpdateVerify::parse_and_verify(&valid_envelope[..len], target, running_version);
        assert_eq!(err, Err("Update envelope too small"));
    }
}

#[test]
fn test_update_verify_anti_rollback_version_rejection() {
    let payload = b"PAYLOAD";
    let target = UpdateTarget {
        os: TargetOs::Linux,
        arch: TargetArch::X64,
    };
    let running_version = 5_000_000u64;

    // Downgrade attacks (version < running_version)
    let downgrade_versions = [
        0u64,
        1,
        4_999_999,
        running_version - 1,
    ];

    for &ver in &downgrade_versions {
        let envelope = build_update_envelope(
            ENVELOPE_MAGIC,
            ENVELOPE_FORMAT,
            2, // Linux
            1, // X64
            ver,
            0,
            payload,
        );
        let err = UpdateVerify::parse_and_verify(&envelope, target, running_version);
        assert_eq!(
            err,
            Err("Rollback rejected: update version <= running version"),
            "Failed to reject rollback to {}",
            ver
        );
    }

    // Replay attack (version == running_version)
    let replay_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        2,
        1,
        running_version,
        0,
        payload,
    );
    let replay_err = UpdateVerify::parse_and_verify(&replay_envelope, target, running_version);
    assert_eq!(
        replay_err,
        Err("Rollback rejected: update version <= running version")
    );

    // Legitimate upgrades (version > running_version)
    let upgrade_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        2,
        1,
        running_version + 1,
        0,
        payload,
    );
    assert!(UpdateVerify::parse_and_verify(&upgrade_envelope, target, running_version).is_ok());
}

#[test]
fn test_update_verify_target_mismatch_and_payload_truncation() {
    let payload = b"TARGET_TEST_PAYLOAD";
    let mac_target = UpdateTarget {
        os: TargetOs::Mac,
        arch: TargetArch::Arm,
    };
    let running_version = 100;

    // Windows envelope delivered to Mac client
    let win_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        0, // Windows
        1, // X64
        200,
        0,
        payload,
    );
    let err_win = UpdateVerify::parse_and_verify(&win_envelope, mac_target, running_version);
    assert_eq!(err_win, Err("Update target mismatch"));

    // Mac X64 envelope delivered to Mac ARM client
    let mac_x64_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        1, // Mac
        1, // X64
        200,
        0,
        payload,
    );
    let err_arch = UpdateVerify::parse_and_verify(&mac_x64_envelope, mac_target, running_version);
    assert_eq!(err_arch, Err("Update target mismatch"));

    // Invalid OS / Arch bytes
    let bad_os_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        99, // Invalid OS
        2,
        200,
        0,
        payload,
    );
    assert_eq!(
        UpdateVerify::parse_and_verify(&bad_os_envelope, mac_target, running_version),
        Err("Invalid target OS")
    );

    let bad_arch_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        1,
        99, // Invalid Arch
        200,
        0,
        payload,
    );
    assert_eq!(
        UpdateVerify::parse_and_verify(&bad_arch_envelope, mac_target, running_version),
        Err("Invalid target Arch")
    );

    // Payload length declared larger than actual available buffer (truncation attack)
    let mut truncated_envelope = build_update_envelope(
        ENVELOPE_MAGIC,
        ENVELOPE_FORMAT,
        1,
        2,
        200,
        0,
        payload,
    );
    // Tamper with payload_len field to declare 10,000 bytes
    truncated_envelope[19..23].copy_from_slice(&10000u32.to_le_bytes());
    let err_trunc = UpdateVerify::parse_and_verify(&truncated_envelope, mac_target, running_version);
    assert_eq!(err_trunc, Err("Invalid payload length"));
}

#[test]
fn test_packer_hash_verification_and_manifest_parsing() {
    let payload = b"ARCHIVE_BINARY_TEST_DATA_1234567890";
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let expected_digest = hasher.finalize();

    // Untampered payload passes verification
    assert!(Packer::verify_update_payload(payload, expected_digest.as_slice()));

    // 1-bit corrupted payload fails verification
    let mut corrupted = payload.to_vec();
    corrupted[0] ^= 0x01;
    assert!(!Packer::verify_update_payload(&corrupted, expected_digest.as_slice()));

    // Version pack calculation
    let ver = Packer::make_update_version(5, 1, 2);
    assert_eq!(ver, 5_001_002);

    // Manifest JSON parsing
    let manifest_json = r#"{
        "version": 5001002,
        "channel": "stable",
        "signature_hex": "abcdef1234567890",
        "files": ["Telegram", "updater"]
    }"#;
    let manifest = Packer::parse_manifest(manifest_json.as_bytes()).unwrap();
    assert_eq!(manifest.version, 5001002);
    assert_eq!(manifest.channel, "stable");
    assert_eq!(manifest.files.len(), 2);

    // Corrupted JSON
    assert!(Packer::parse_manifest(b"INVALID_JSON").is_err());
}
