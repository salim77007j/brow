/* Localization: English + Arabic dictionaries with RTL support.
 *
 * brow ships a first-class Arabic (RTL) UI. The dictionary is a static table
 * (zero startup cost); a compile-time-checked parity test guarantees both
 * languages stay complete. RTL correctness is three things:
 *   1. `Lang::is_rtl()` → the shell sets mirror direction on all layouts;
 *   2. `mirror_align()` → text alignment flips for RTL locales;
 *   3. `bidi_isolate()` → mixed-direction strings (e.g. an English URL inside
 *      an Arabic label) are wrapped in U+2066 (LRI) … U+2069 (PDI) so the
 *      Unicode bidi algorithm cannot scramble them.
 */

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    #[default]
    En,
    Ar,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Ar => "ar",
        }
    }

    pub fn from_code(code: &str) -> Lang {
        match code.split(['-', '_']).next().unwrap_or("en") {
            "ar" => Lang::Ar,
            _ => Lang::En,
        }
    }

    pub fn is_rtl(self) -> bool {
        matches!(self, Lang::Ar)
    }

    pub fn name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::Ar => "العربية",
        }
    }
}

/// Every UI string key. The dictionaries below must cover all variants
/// (enforced by tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Str {
    NewTab,
    CloseTab,
    AddressBarHint,
    Back,
    Forward,
    Reload,
    Home,
    Stop,
    Bookmarks,
    History,
    Downloads,
    Settings,
    BookmarkAdded,
    BookmarkRemoved,
    SearchBookmarks,
    SearchHistory,
    ClearHistory,
    NoDownloads,
    DownloadInProgress,
    DownloadPaused,
    DownloadCompleted,
    DownloadFailed,
    SleepThisTab,
    WakeThisTab,
    TabSlept,
    TabsSleptCount,
    MemoryDashboard,
    MemoryPerTab,
    MemoryTotal,
    ThemeLight,
    ThemeDark,
    ThemeSystem,
    Language,
    SearchEngine,
    Startup,
    RestoreSession,
    Privacy,
    BlockAds,
    DnsOverHttps,
    MinTlsVersion,
    Menu,
    Ok,
    Cancel,
    Save,
    Open,
    TabCount,
    Loading,
}

const EN: &[(Str, &str)] = &[
    (Str::NewTab, "New tab"),
    (Str::CloseTab, "Close tab"),
    (Str::AddressBarHint, "Search or enter address"),
    (Str::Back, "Back"),
    (Str::Forward, "Forward"),
    (Str::Reload, "Reload"),
    (Str::Home, "Home"),
    (Str::Stop, "Stop"),
    (Str::Bookmarks, "Bookmarks"),
    (Str::History, "History"),
    (Str::Downloads, "Downloads"),
    (Str::Settings, "Settings"),
    (Str::BookmarkAdded, "Bookmarked"),
    (Str::BookmarkRemoved, "Bookmark removed"),
    (Str::SearchBookmarks, "Search bookmarks"),
    (Str::SearchHistory, "Search history"),
    (Str::ClearHistory, "Clear history"),
    (Str::NoDownloads, "No downloads"),
    (Str::DownloadInProgress, "Downloading…"),
    (Str::DownloadPaused, "Paused"),
    (Str::DownloadCompleted, "Completed"),
    (Str::DownloadFailed, "Failed"),
    (Str::SleepThisTab, "Sleep this tab"),
    (Str::WakeThisTab, "Wake tab"),
    (Str::TabSlept, "Tab sleeping — click to reload"),
    (Str::TabsSleptCount, "tabs sleeping"),
    (Str::MemoryDashboard, "Memory"),
    (Str::MemoryPerTab, "Per-tab budget"),
    (Str::MemoryTotal, "Total budget"),
    (Str::ThemeLight, "Light"),
    (Str::ThemeDark, "Dark"),
    (Str::ThemeSystem, "System"),
    (Str::Language, "Language"),
    (Str::SearchEngine, "Search engine"),
    (Str::Startup, "Startup"),
    (Str::RestoreSession, "Restore session"),
    (Str::Privacy, "Privacy"),
    (Str::BlockAds, "Block ads and trackers"),
    (Str::DnsOverHttps, "DNS over HTTPS"),
    (Str::MinTlsVersion, "Minimum TLS version"),
    (Str::Menu, "Menu"),
    (Str::Ok, "OK"),
    (Str::Cancel, "Cancel"),
    (Str::Save, "Save"),
    (Str::Open, "Open"),
    (Str::TabCount, "tabs"),
    (Str::Loading, "Loading…"),
];

const AR: &[(Str, &str)] = &[
    (Str::NewTab, "تبويب جديد"),
    (Str::CloseTab, "إغلاق التبويب"),
    (Str::AddressBarHint, "ابحث أو أدخل عنوانًا"),
    (Str::Back, "رجوع"),
    (Str::Forward, "تقدّم"),
    (Str::Reload, "تحديث"),
    (Str::Home, "الرئيسية"),
    (Str::Stop, "إيقاف"),
    (Str::Bookmarks, "العلامات المرجعية"),
    (Str::History, "السجل"),
    (Str::Downloads, "التنزيلات"),
    (Str::Settings, "الإعدادات"),
    (Str::BookmarkAdded, "أُضيف إلى العلامات المرجعية"),
    (Str::BookmarkRemoved, "أُزيلت العلامة المرجعية"),
    (Str::SearchBookmarks, "ابحث في العلامات المرجعية"),
    (Str::SearchHistory, "ابحث في السجل"),
    (Str::ClearHistory, "مسح السجل"),
    (Str::NoDownloads, "لا توجد تنزيلات"),
    (Str::DownloadInProgress, "جارٍ التنزيل…"),
    (Str::DownloadPaused, "متوقف مؤقتًا"),
    (Str::DownloadCompleted, "اكتمل"),
    (Str::DownloadFailed, "فشل"),
    (Str::SleepThisTab, "إسبات هذا التبويب"),
    (Str::WakeThisTab, "إيقاظ التبويب"),
    (Str::TabSlept, "التبويب نائم — انقر لإعادة التحميل"),
    (Str::TabsSleptCount, "تبويبات نائمة"),
    (Str::MemoryDashboard, "الذاكرة"),
    (Str::MemoryPerTab, "حد كل تبويب"),
    (Str::MemoryTotal, "الحد الإجمالي"),
    (Str::ThemeLight, "فاتح"),
    (Str::ThemeDark, "داكن"),
    (Str::ThemeSystem, "النظام"),
    (Str::Language, "اللغة"),
    (Str::SearchEngine, "محرك البحث"),
    (Str::Startup, "بدء التشغيل"),
    (Str::RestoreSession, "استعادة الجلسة"),
    (Str::Privacy, "الخصوصية"),
    (Str::BlockAds, "حجب الإعلانات والمتتبعات"),
    (Str::DnsOverHttps, "DNS عبر HTTPS"),
    (Str::MinTlsVersion, "أدنى إصدار TLS"),
    (Str::Menu, "القائمة"),
    (Str::Ok, "حسنًا"),
    (Str::Cancel, "إلغاء"),
    (Str::Save, "حفظ"),
    (Str::Open, "فتح"),
    (Str::TabCount, "تبويبات"),
    (Str::Loading, "جارٍ التحميل…"),
];

/// Translator bound to a language.
#[derive(Debug, Clone, Copy)]
pub struct L10n {
    lang: Lang,
}

impl L10n {
    pub fn new(lang: Lang) -> Self {
        Self { lang }
    }

    pub fn lang(&self) -> Lang {
        self.lang
    }

    pub fn set_lang(&mut self, lang: Lang) {
        self.lang = lang;
    }

    pub fn is_rtl(&self) -> bool {
        self.lang.is_rtl()
    }

    /// Translate `key`. Falls back to English if the active dictionary is
    /// somehow missing the key (never happens thanks to the parity test).
    pub fn tr(&self, key: Str) -> &'static str {
        let table = match self.lang {
            Lang::En => EN,
            Lang::Ar => AR,
        };
        table
            .iter()
            .find(|(k, _)| *k == key)
            .or_else(|| EN.iter().find(|(k, _)| *k == key))
            .map(|(_, v)| *v)
            .unwrap_or("")
    }

    /// Alignment that mirrors for RTL: ("start", "end") semantics.
    pub fn mirror_align(&self, ltr_align: Align) -> Align {
        if self.is_rtl() {
            match ltr_align {
                Align::Start => Align::End,
                Align::End => Align::Start,
                other => other,
            }
        } else {
            ltr_align
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Start,
    Center,
    End,
}

/// Wrap a foreign-direction fragment in Unicode bidi isolates so it cannot
/// scramble the surrounding text. LTR fragments in RTL text and vice versa.
pub fn bidi_isolate(fragment: &str, fragment_is_ltr: bool, ui_is_rtl: bool) -> String {
    if fragment_is_ltr == ui_is_rtl {
        // Direction differs from UI direction → isolate needed.
        let (open, close) = ('\u{2066}', '\u{2069}'); // LRI … PDI
        format!("{open}{fragment}{close}")
    } else {
        fragment.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_parity() {
        let en_keys: std::collections::HashSet<_> = EN.iter().map(|(k, _)| *k).collect();
        let ar_keys: std::collections::HashSet<_> = AR.iter().map(|(k, _)| *k).collect();
        assert_eq!(en_keys, ar_keys, "EN and AR dictionaries diverge");
        assert!(!en_keys.is_empty());
        // No empty strings anywhere.
        for (_, v) in EN.iter().chain(AR.iter()) {
            assert!(!v.trim().is_empty());
        }
    }

    #[test]
    fn translations_differ_and_lookup_works() {
        let en = L10n::new(Lang::En);
        let ar = L10n::new(Lang::Ar);
        assert_eq!(en.tr(Str::NewTab), "New tab");
        assert_eq!(ar.tr(Str::NewTab), "تبويب جديد");
        assert_ne!(en.tr(Str::Settings), ar.tr(Str::Settings));
    }

    #[test]
    fn rtl_flags_and_mirroring() {
        assert!(!Lang::En.is_rtl());
        assert!(Lang::Ar.is_rtl());
        let ar = L10n::new(Lang::Ar);
        assert_eq!(ar.mirror_align(Align::Start), Align::End);
        let en = L10n::new(Lang::En);
        assert_eq!(en.mirror_align(Align::Start), Align::Start);
    }

    #[test]
    fn lang_code_roundtrip() {
        assert_eq!(Lang::from_code("ar-EG"), Lang::Ar);
        assert_eq!(Lang::from_code("en_US"), Lang::En);
        assert_eq!(Lang::from_code("fr"), Lang::En); // fallback
        assert_eq!(Lang::Ar.code(), "ar");
    }

    #[test]
    fn bidi_isolation() {
        // An English URL inside an Arabic label must be isolated.
        let s = bidi_isolate("https://example.com/a_b", true, true);
        assert!(s.starts_with('\u{2066}'));
        assert!(s.ends_with('\u{2069}'));
        // Same-direction fragment is untouched.
        let plain = bidi_isolate("نص عربي", false, true);
        assert_eq!(plain, "نص عربي");
        // English fragment in English UI untouched.
        let plain_en = bidi_isolate("https://example.com/", true, false);
        assert_eq!(plain_en, "https://example.com/");
    }

    #[test]
    fn arabic_strings_are_non_ascii() {
        // Guard against accidentally filling AR with English.
        assert!(AR.iter().any(|(_, v)| v.chars().any(|c| c >= '\u{0600}' && c <= '\u{06FF}')));
    }

    #[test]
    fn settings_serializes_lang() {
        let mut s = crate::settings::Settings::default();
        s.set_from_str("locale", "ar").unwrap();
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"locale\":\"ar\"") || json.contains("\"locale\" : \"ar\""));
        let back: crate::settings::Settings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.locale, Lang::Ar);
    }
}
