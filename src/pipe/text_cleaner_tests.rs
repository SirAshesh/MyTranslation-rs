use super::*;

#[test]
fn test_cjk_detection() {
    assert!(contains_cjk("こんにちは"));
    assert!(contains_cjk("何で？"));
    assert!(contains_cjk("这不仅是测试"));
    assert!(!contains_cjk("Hello world!"));
    assert!(!contains_cjk("Apa kabar"));
}

#[test]
fn test_join_manga_lines() {
    let ja_lines = ["「お前を", "絶対に許さない」"];
    assert_eq!(join_manga_lines(&ja_lines), "「お前を絶対に許さない」");

    let en_lines = ["HE HAS NO", "REASON"];
    assert_eq!(join_manga_lines(&en_lines), "HE HAS NO REASON");

    let hyphen_lines = ["SOME-", "THING"];
    assert_eq!(join_manga_lines(&hyphen_lines), "SOMETHING");

    let stutter_lines = ["THA-", "THAT'S", "NOT WHAT I SAID!!"];
    assert_eq!(
        join_manga_lines(&stutter_lines),
        "THA- THAT'S NOT WHAT I SAID!!"
    );

    let short_stutter = ["I-", "I DIDN'T MEAN THAT!"];
    assert_eq!(
        join_manga_lines(&short_stutter),
        "I- I DIDN'T MEAN THAT!"
    );

    let honorific_lines = ["TANAKA-", "SAN SAID SO."];
    assert_eq!(
        join_manga_lines(&honorific_lines),
        "TANAKA-SAN SAID SO."
    );
}

#[test]
fn test_cjk_not_rejected_as_junk() {
    assert!(!is_junk_or_sfx("何", 0.70, 20.0, 100.0));
    assert!(!is_junk_or_sfx("誰？", 0.75, 25.0, 80.0));
    assert!(!is_junk_or_sfx("嘘…", 0.80, 20.0, 60.0));
}

#[test]
fn test_sentence_capitalization() {
    let raw = "HE HAS NO REASON TO COME OUT HERE ANYMORE.";
    assert_eq!(
        normalize_source_text(raw),
        "He has no reason to come out here anymore."
    );

    let stutter = "THA- THAT'S NOT WHAT I SAID!!";
    assert_eq!(
        normalize_source_text(stutter),
        "Tha- that's not what I said!!"
    );
}

#[test]
fn test_manhwa_dialogue_with_numbers_and_names_not_dropped() {
    assert!(!is_junk_or_sfx("LORD BLACKWOOD", 1.0, 247.0, 24.0));
    assert!(!is_junk_or_sfx("LORD BLACKWOOD,", 1.0, 302.0, 29.0));
    assert!(!is_junk_or_sfx("7 LONG YEARS.", 1.0, 204.0, 28.0));
    assert!(!is_junk_or_sfx("2 MILLION", 0.99, 138.0, 27.0));
    assert!(!is_junk_or_sfx("2 WEEKS SINCE YOU", 0.98, 256.0, 24.0));
    assert!(!is_junk_or_sfx("ATTACK NOW!", 0.99, 180.0, 24.0));
    assert!(!is_junk_or_sfx("ACROSS THE HALL", 0.99, 210.0, 24.0));
}

#[test]
fn test_low_confidence_artwork_noise_dropped() {
    assert!(is_junk_or_sfx("Go", 0.47, 45.0, 25.0));
    assert!(is_junk_or_sfx("On", 0.54, 150.0, 80.0));
    assert!(is_junk_or_sfx("poond", 0.45, 36.0, 14.0));
    assert!(is_junk_or_sfx("JDRU", 0.50, 61.0, 24.0));
    assert!(is_junk_or_sfx("L GKI", 0.57, 83.0, 10.0));
    assert!(is_junk_or_sfx("GeG", 0.71, 52.0, 29.5));
    assert!(is_junk_or_sfx("cors", 0.75, 21.8, 17.6));
    assert!(!is_junk_or_sfx("GO", 0.96, 52.0, 26.0));
}

#[test]
fn test_vocal_dialogue_interjections_preserved() {
    assert!(!is_junk_or_sfx("NGH...", 1.0, 84.0, 31.0));
    assert!(!is_junk_or_sfx("MMM....", 1.0, 108.0, 32.0));
    assert!(!is_junk_or_sfx("HMM?", 0.85, 68.0, 28.0));
    assert!(!is_junk_or_sfx("HMPH!", 0.90, 74.0, 28.0));
    assert!(!is_junk_or_sfx("PHEW...", 0.72, 70.0, 26.0));
    assert!(!is_junk_or_sfx("UGH!", 0.88, 60.0, 26.0));
    assert!(!is_junk_or_sfx("ARGH!", 0.92, 80.0, 28.0));
    assert!(is_junk_or_sfx("NGH", 0.45, 40.0, 20.0));
    assert!(is_junk_or_sfx("FWIP!", 0.95, 70.0, 30.0));
}

#[test]
fn test_watermarks_and_korean_sfx_gibberish_dropped() {
    assert!(is_junk_or_sfx("Mangayyorg", 0.99, 200.0, 40.0));
    assert!(is_junk_or_sfx("mangayy.org", 0.99, 200.0, 40.0));
    assert!(is_junk_or_sfx("Erisscanscom", 0.99, 110.0, 24.0));
    assert!(is_junk_or_sfx("ErisScans.com", 0.99, 110.0, 24.0));
    assert!(is_junk_or_sfx("erisscans", 0.99, 110.0, 24.0));

    for token in [
        "kyt", "Erxi", "Yotot..", "Yorot.", "otor..", "Yor..", "Ktoo", "Yloh", "Myok",
        "Tritm", "Rim", "Wok",
    ] {
        assert!(
            is_junk_or_sfx(token, 0.98, 90.0, 70.0),
            "Expected gibberish SFX {} to be dropped",
            token
        );
    }

    assert!(!is_junk_or_sfx("Hik!", 0.98, 118.0, 56.0));
    assert!(!is_junk_or_sfx("Hic, ngh....", 0.98, 164.0, 34.0));
}

#[test]
fn test_large_artwork_single_token_boxes_filtered_except_exclamations() {
    assert!(is_junk_or_sfx("CRASH!", 0.99, 140.0, 110.0));
    assert!(is_junk_or_sfx("BAM", 0.99, 120.0, 120.0));
    assert!(is_junk_or_sfx("05", 0.97, 234.0, 243.0));
    assert!(is_junk_or_sfx("19", 1.0, 25.0, 24.0));
    assert!(!is_junk_or_sfx("19?", 0.96, 40.0, 24.0));
    assert!(!is_junk_or_sfx("100!", 0.96, 55.0, 26.0));
    assert!(!is_junk_or_sfx("3...", 0.95, 40.0, 24.0));
    assert!(!is_junk_or_sfx("Huh?", 0.99, 236.0, 154.0));
    assert!(!is_junk_or_sfx("Aah...!", 0.99, 305.0, 102.0));
    assert!(!is_junk_or_sfx("Oh", 0.99, 140.0, 246.0));
    assert!(!is_junk_or_sfx("WAIT!!", 0.99, 160.0, 100.0));
    assert!(!is_junk_or_sfx("No!", 0.99, 100.0, 100.0));
}
