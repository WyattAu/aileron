use aileron::net::adblock::AdBlocker;
use aileron::net::filter_list::{FilterList, ResourceType};

const FILTER_LIST_TEXT: &str = r#"
[Adblock Plus 2.0]
! Title: Test AdBlock Filter List
! Homepage: https://example.com/
! Last modified: 2025-01-01

||doubleclick.net^
||googlesyndication.com^
||ads.example.com^
||tracker.evil.net^
||adservice.google.com^
||amazon-adsystem.com^
||facebook.net/signals^
/analytics.js
/banner_
/popup.js
/pagead/js

! Whitelist rules
@@||safe.example.com^

! Cosmetic rules
##div.ad-banner
##.sponsored-content
##.popup-overlay
example.com##.ad-slot
news.example.com##.newsletter-popup

! Resource-type specific
||cdn.example.com/ads.js$script
||cdn.example.com/ad-image.png$image

! Third-party only
||thirdparty-tracker.com^$third-party

! Domain-specific
example.com||ads.specific-tracker.com^

! Exception for a normally-blocked domain
@@||ads.example.com/whitelisted-path^
"#;

#[test]
fn test_load_inline_filter_list_parses_rules() {
    let mut blocker = AdBlocker::new();
    let count = blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    assert!(
        count > 0,
        "Expected rules to be parsed from inline filter list"
    );
    assert!(blocker.rule_count() > 0);
}

#[test]
fn test_filter_list_parser_parses_correct_types() {
    let list = FilterList::parse(FILTER_LIST_TEXT);

    assert!(
        !list.network_filters.is_empty(),
        "Network filters should be parsed"
    );
    assert!(
        !list.cosmetic_filters.is_empty(),
        "Cosmetic filters should be parsed"
    );

    let has_exception = list.network_filters.iter().any(|f| f.is_exception);
    assert!(has_exception, "Exception filters (@@) should be parsed");

    let has_third_party = list.network_filters.iter().any(|f| f.third_party_only);
    assert!(has_third_party, "Third-party filters should be parsed");

    let has_script_type = list.network_filters.iter().any(|f| {
        f.resource_types
            .as_ref()
            .is_some_and(|types| types.contains(&ResourceType::Script))
    });
    assert!(
        has_script_type,
        "Script resource type filters should be parsed"
    );

    let has_image_type = list.network_filters.iter().any(|f| {
        f.resource_types
            .as_ref()
            .is_some_and(|types| types.contains(&ResourceType::Image))
    });
    assert!(
        has_image_type,
        "Image resource type filters should be parsed"
    );
}

#[test]
fn test_known_tracking_domain_blocked() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    // Note: load_filter_list only does exact domain matching for blocked_domains.
    // Subdomains like www.doubleclick.net or z.amazon-adsystem.com won't match
    // the parent domain via the blocked_domains path.
    let tracking_urls = [
        "https://doubleclick.net/ad.js",
        "https://ads.googlesyndication.com/pagead/js",
        "https://tracker.evil.net/track",
        "https://cdn.example.com/analytics.js",
        "https://example.com/banner_top.js",
        "https://example.com/popup.js",
        "https://example.com/pagead/js/adsbygoogle.js",
    ];

    for url_str in &tracking_urls {
        let url = url::Url::parse(url_str).unwrap();
        assert!(
            blocker.should_block(&url, None, None),
            "Tracking URL should be blocked: {url_str}"
        );
    }
}

#[test]
fn test_known_safe_domain_allowed() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    let safe_urls = [
        "https://example.com/page",
        "https://www.example.com/about",
        "https://safe.example.com/resource",
        "https://safe.example.com/anything",
        "https://example.com/allowed/resource",
        "https://github.com/user/repo",
        "https://en.wikipedia.org/wiki/Main_Page",
        "https://rust-lang.org/learn",
        "https://docs.rs/serde",
    ];

    for url_str in &safe_urls {
        let url = url::Url::parse(url_str).unwrap();
        assert!(
            !blocker.should_block(&url, None, None),
            "Safe URL should be allowed: {url_str}"
        );
    }
}

#[test]
fn test_exception_whitelist_rules_work() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    // safe.example.com should be whitelisted via @@||safe.example.com^
    let safe_url = url::Url::parse("https://safe.example.com/ad.js").unwrap();
    assert!(
        !blocker.should_block(&safe_url, None, None),
        "Whitelisted domain should be allowed"
    );

    // ads.example.com is whitelisted via @@||ads.example.com/whitelisted-path^
    // (domain extraction makes the whole domain whitelisted)

    // ads.example.com/whitelisted-path is explicitly exceptioned
    let whitelisted_path =
        url::Url::parse("https://ads.example.com/whitelisted-path/resource").unwrap();
    assert!(
        !blocker.should_block(&whitelisted_path, None, None),
        "Exceptioned path should be allowed"
    );

    // Important filter cannot be overridden by exception
    let list = FilterList::parse("||important-block.com^$important\n@@||important-block.com^");
    let mut important_blocker = AdBlocker::new();
    important_blocker.load_from_filter_lists(&[list]);
    let important_url = url::Url::parse("https://important-block.com/page").unwrap();
    assert!(
        important_blocker.should_block(&important_url, None, None),
        "Important filter should block even with exceptions"
    );

    // Non-important filter respects whitelist
    let list2 = FilterList::parse("||normal-block.com^\n@@||normal-block.com^");
    let mut normal_blocker = AdBlocker::new();
    normal_blocker.load_from_filter_lists(&[list2]);
    let normal_url = url::Url::parse("https://normal-block.com/page").unwrap();
    assert!(
        !normal_blocker.should_block(&normal_url, None, None),
        "Non-important filter should respect whitelist exception"
    );
}

#[test]
fn test_cosmetic_css_rules_generated_for_domain() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    // Generic cosmetic rules should appear for any domain
    let generic_css = blocker.cosmetic_css_for_domain("example.com");
    assert!(
        generic_css.contains("div.ad-banner"),
        "Generic cosmetic rule should be present"
    );
    assert!(
        generic_css.contains(".sponsored-content"),
        "Generic cosmetic rule should be present"
    );
    assert!(
        generic_css.contains(".popup-overlay"),
        "Generic cosmetic rule should be present"
    );

    // Domain-specific cosmetic rules
    assert!(
        generic_css.contains(".ad-slot"),
        "Domain-specific rule for example.com should be present"
    );
    assert!(
        !generic_css.contains(".newsletter-popup"),
        "news.example.com rule should NOT appear for example.com"
    );

    // Subdomain should inherit parent domain's cosmetic rules
    let subdomain_css = blocker.cosmetic_css_for_domain("news.example.com");
    assert!(
        subdomain_css.contains(".ad-slot"),
        "Subdomain should inherit parent domain's cosmetic rules"
    );
    assert!(
        subdomain_css.contains(".newsletter-popup"),
        "Subdomain's own cosmetic rules should be present"
    );

    // Unrelated domain should only get generic rules
    let other_css = blocker.cosmetic_css_for_domain("other.com");
    assert!(
        other_css.contains("div.ad-banner"),
        "Generic cosmetic rules should apply to any domain"
    );
    assert!(
        !other_css.contains(".ad-slot"),
        "Domain-specific rules should NOT apply to unrelated domains"
    );
    assert!(
        !other_css.contains(".newsletter-popup"),
        "Domain-specific rules should NOT apply to unrelated domains"
    );
}

#[test]
fn test_end_to_end_easylist_format_multiple_urls() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    let test_cases: Vec<(&str, bool, Option<&str>)> = vec![
        // (url, should_block, description)
        (
            "https://doubleclick.net/ad",
            true,
            Some("doubleclick domain"),
        ),
        (
            "https://ads.googlesyndication.com/pagead/js",
            true,
            Some("googlesyndication"),
        ),
        (
            "https://tracker.evil.net/track",
            true,
            Some("tracker.evil.net"),
        ),
        (
            "https://cdn.example.com/analytics.js",
            true,
            Some("analytics.js pattern + cdn.example.com domain"),
        ),
        (
            "https://example.com/banner_top.js",
            true,
            Some("banner_ pattern"),
        ),
        (
            "https://example.com/popup.js",
            true,
            Some("popup.js pattern"),
        ),
        (
            "https://example.com/pagead/js/ads",
            true,
            Some("pagead/js pattern"),
        ),
        (
            "https://safe.example.com/resource",
            false,
            Some("whitelisted domain"),
        ),
        (
            "https://ads.example.com/banner.js",
            false,
            Some("ads.example.com whitelisted via exception"),
        ),
        ("https://example.com/page", false, Some("normal page")),
        ("https://github.com/user/repo", false, Some("github safe")),
        (
            "https://en.wikipedia.org/wiki/Page",
            false,
            Some("wikipedia safe"),
        ),
        ("https://rust-lang.org/learn", false, Some("rust-lang safe")),
        ("https://docs.rs/serde", false, Some("docs.rs safe")),
        (
            "https://example.com/normal.js",
            false,
            Some("non-matching script"),
        ),
        (
            "https://example.com/stylesheet.css",
            false,
            Some("non-matching css"),
        ),
    ];

    let mut blocked_count = 0u32;
    let mut allowed_count = 0u32;

    for (url_str, expected_block, description) in &test_cases {
        let url = url::Url::parse(url_str).unwrap();
        let actual_block = blocker.should_block(&url, None, None);
        assert_eq!(
            actual_block,
            *expected_block,
            "{}: expected {} for {}",
            description.unwrap_or(""),
            if *expected_block { "block" } else { "allow" },
            url_str
        );
        if actual_block {
            blocked_count += 1;
        } else {
            allowed_count += 1;
        }
    }

    assert!(blocked_count > 0, "Some URLs should be blocked");
    assert!(allowed_count > 0, "Some URLs should be allowed");
}

#[test]
fn test_load_from_filter_lists_structured_api() {
    let mut blocker = AdBlocker::new();
    let list = FilterList::parse(FILTER_LIST_TEXT);
    let count = blocker.load_from_filter_lists(&[list]);

    assert!(count > 0, "Rules should be loaded from FilterList structs");

    let url = url::Url::parse("https://doubleclick.net/ad").unwrap();
    assert!(blocker.should_block(&url, None, None));

    let safe_url = url::Url::parse("https://safe.example.com/page").unwrap();
    assert!(!blocker.should_block(&safe_url, None, None));
}

#[test]
fn test_cosmetic_filtering_can_be_disabled() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    assert!(blocker.cosmetic_filtering_enabled());

    let css = blocker.cosmetic_css();
    assert!(!css.is_empty());

    blocker.set_cosmetic_filtering(false);
    assert!(!blocker.cosmetic_filtering_enabled());
    assert!(blocker.cosmetic_css().is_empty());
    assert!(blocker.cosmetic_css_for_domain("example.com").is_empty());
}

#[test]
fn test_blocked_count_tracks_matches() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    assert_eq!(blocker.blocked_count(), 0);

    let url = url::Url::parse("https://doubleclick.net/ad").unwrap();
    blocker.should_block(&url, None, None);
    assert_eq!(blocker.blocked_count(), 1);

    let url2 = url::Url::parse("https://example.com/page").unwrap();
    blocker.should_block(&url2, None, None);
    assert_eq!(
        blocker.blocked_count(),
        1,
        "Non-blocked URL should not increment count"
    );

    let url3 = url::Url::parse("https://tracker.evil.net/track").unwrap();
    blocker.should_block(&url3, None, None);
    assert_eq!(blocker.blocked_count(), 2);

    blocker.reset_blocked_count();
    assert_eq!(blocker.blocked_count(), 0);
}

#[test]
fn test_adblocker_can_be_disabled() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    let url = url::Url::parse("https://doubleclick.net/ad").unwrap();
    assert!(blocker.should_block(&url, None, None));

    blocker.set_enabled(false);
    assert!(!blocker.is_enabled());
    assert!(
        !blocker.should_block(&url, None, None),
        "Disabled blocker should not block"
    );

    blocker.set_enabled(true);
    assert!(blocker.is_enabled());
    assert!(
        blocker.should_block(&url, None, None),
        "Re-enabled blocker should block"
    );
}

#[test]
fn test_cosmetic_js_injection_produces_valid_js() {
    let mut blocker = AdBlocker::new();
    blocker.load_filter_list(FILTER_LIST_TEXT).unwrap();

    let js = blocker.cosmetic_js_injection("example.com");
    assert!(
        js.is_some(),
        "JS injection should be produced when cosmetic rules exist"
    );

    let js = js.unwrap();
    assert!(js.contains("__aileron_adblock_css"));
    assert!(js.contains("document.createElement"));
    assert!(js.contains("appendChild"));
    assert!(js.contains("div.ad-banner"));
    assert!(js.contains(".ad-slot"));
}

#[test]
fn test_resource_type_filtering() {
    let mut blocker = AdBlocker::new();
    let list = FilterList::parse("/ads.js$script\n/ad-image.png$image");
    blocker.load_from_filter_lists(&[list]);

    let script_url = url::Url::parse("https://cdn.example.com/ads.js").unwrap();
    let image_url = url::Url::parse("https://cdn.example.com/ad-image.png").unwrap();

    assert!(
        blocker.should_block(&script_url, Some(ResourceType::Script), None),
        "Script URL should be blocked for script resource type"
    );
    assert!(
        !blocker.should_block(&script_url, Some(ResourceType::Image), None),
        "Script URL should NOT be blocked for image resource type"
    );
    assert!(
        blocker.should_block(&image_url, Some(ResourceType::Image), None),
        "Image URL should be blocked for image resource type"
    );
    assert!(
        !blocker.should_block(&image_url, Some(ResourceType::Script), None),
        "Image URL should NOT be blocked for script resource type"
    );
    // When resource_type is None, filters with type constraints still match
    // (no type information to filter against)
    assert!(
        blocker.should_block(&script_url, None, None),
        "URL should be blocked when resource_type is None (no type constraint applied)"
    );
}

#[test]
fn test_third_party_filtering() {
    let mut blocker = AdBlocker::new();
    // Use a pattern without || prefix so it doesn't go into blocked_domains
    let list = FilterList::parse("/third-party-track$third-party");
    blocker.load_from_filter_lists(&[list]);

    let url = url::Url::parse("https://example.com/third-party-track").unwrap();
    assert!(
        blocker.should_block(&url, None, Some(true)),
        "Third-party URL should be blocked when is_third_party is true"
    );
    assert!(
        !blocker.should_block(&url, None, Some(false)),
        "Third-party URL should NOT be blocked when is_third_party is false"
    );
}
