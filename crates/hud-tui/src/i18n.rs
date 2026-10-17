//! UI texts from Fluent resources in `locales/{de,en}/hud.ftl`.

use std::borrow::Cow;
use std::collections::HashMap;

use fluent_templates::fluent_bundle::FluentValue;
use fluent_templates::{LanguageIdentifier, Loader, langid, static_loader};

use crate::settings::Language;

static_loader! {
    static LOCALES = {
        locales: "./locales",
        fallback_language: "en",
        // Unicode isolation marks around arguments render as visible garbage
        // in many terminals.
        customise: |bundle| bundle.set_use_isolating(false),
    };
}

static DE: LanguageIdentifier = langid!("de");
static EN: LanguageIdentifier = langid!("en");

fn id(language: Language) -> &'static LanguageIdentifier {
    match language {
        Language::De => &DE,
        Language::En => &EN,
    }
}

/// Text for a message id.
#[must_use]
pub fn t(language: Language, message: &str) -> String {
    LOCALES.lookup(id(language), message)
}

/// Text for a message id with named string arguments.
#[must_use]
pub fn t_args(language: Language, message: &str, args: &[(&'static str, &str)]) -> String {
    let args: HashMap<Cow<'static, str>, FluentValue<'_>> = args
        .iter()
        .map(|(name, value)| (Cow::Borrowed(*name), FluentValue::from(*value)))
        .collect();
    LOCALES.lookup_with_args(id(language), message, &args)
}

/// Text for a message id with one numeric argument, used for plural rules.
#[must_use]
pub fn t_count(language: Language, message: &str, name: &'static str, count: u32) -> String {
    let args: HashMap<Cow<'static, str>, FluentValue<'_>> =
        HashMap::from([(Cow::Borrowed(name), FluentValue::from(count))]);
    LOCALES.lookup_with_args(id(language), message, &args)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    fn ids(source: &str) -> BTreeSet<String> {
        source
            .lines()
            .filter(|line| line.starts_with(|c: char| c.is_ascii_lowercase()))
            .filter_map(|line| line.split_once(" =").map(|(id, _)| id.trim().to_owned()))
            .collect()
    }

    #[test]
    fn both_languages_define_the_same_message_ids() {
        let de = ids(include_str!("../locales/de/hud.ftl"));
        let en = ids(include_str!("../locales/en/hud.ftl"));
        assert_eq!(de, en);
    }

    #[test]
    fn german_text_differs_from_english() {
        assert_eq!(t(Language::De, "tab-settings"), "Einstellungen");
        assert_eq!(t(Language::En, "tab-settings"), "Settings");
    }

    #[test]
    fn plural_rules_pick_singular_and_plural() {
        assert_eq!(
            t_count(Language::De, "state-fast-forwarded", "count", 1),
            "Fast-Forward: 1 Commit"
        );
        assert_eq!(
            t_count(Language::De, "state-fast-forwarded", "count", 3),
            "Fast-Forward: 3 Commits"
        );
        assert_eq!(
            t_count(Language::En, "skip-ahead", "count", 2),
            "2 local commits not pushed"
        );
    }

    #[test]
    fn arguments_are_inserted_without_isolation_marks() {
        let text = t_args(Language::En, "tab-placeholder", &[("tab", "Log")]);
        assert_eq!(text, "Log is not built yet.");
    }
}
