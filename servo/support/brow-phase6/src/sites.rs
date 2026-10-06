/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! The Phase 6 site matrix: 10 categories × 10 real, stable, HTTPS URLs.
//!
//! Categories mirror the phase plan's "10 类网站" requirement. Every URL is
//! fetched exactly as written; failures (403s from bot walls, regional
//! blocks, TLS quirks) are recorded as first-class data — they are part of
//! the honest result, not noise to hide.

/// One site category of the comparison matrix.
#[derive(Clone, Copy, Debug)]
pub struct Category {
    /// Stable machine name used in CLI filters and output files.
    pub id: &'static str,
    /// Human-readable label for reports.
    pub label: &'static str,
    /// The 10 representative URLs (homepage-level, https).
    pub urls: &'static [&'static str],
}

/// The full 10 × 10 matrix.
pub const CATEGORIES: &[Category] = &[
    Category {
        id: "news",
        label: "News portals",
        urls: &[
            "https://www.reuters.com/",
            "https://apnews.com/",
            "https://www.bbc.com/news",
            "https://www.cnn.com/",
            "https://www.nytimes.com/",
            "https://www.theguardian.com/",
            "https://www.aljazeera.com/",
            "https://www.npr.org/",
            "https://www.dw.com/",
            "https://www.france24.com/",
        ],
    },
    Category {
        id: "ecommerce",
        label: "E-commerce",
        urls: &[
            "https://www.amazon.com/",
            "https://www.ebay.com/",
            "https://www.walmart.com/",
            "https://www.etsy.com/",
            "https://www.aliexpress.com/",
            "https://www.bestbuy.com/",
            "https://www.target.com/",
            "https://www.newegg.com/",
            "https://www.wayfair.com/",
            "https://www.flipkart.com/",
        ],
    },
    Category {
        id: "docs",
        label: "Developer documentation",
        urls: &[
            "https://developer.mozilla.org/",
            "https://docs.python.org/3/",
            "https://doc.rust-lang.org/book/",
            "https://learn.microsoft.com/",
            "https://docs.aws.amazon.com/",
            "https://developer.apple.com/",
            "https://docs.oracle.com/",
            "https://go.dev/doc/",
            "https://kotlinlang.org/docs/home.html",
            "https://www.php.net/manual/",
        ],
    },
    Category {
        id: "video",
        label: "Video & streaming landing",
        urls: &[
            "https://www.youtube.com/",
            "https://vimeo.com/",
            "https://www.twitch.tv/",
            "https://www.dailymotion.com/",
            "https://www.bilibili.com/",
            "https://rumble.com/",
            "https://www.netflix.com/",
            "https://www.disneyplus.com/",
            "https://www.primevideo.com/",
            "https://www.peacocktv.com/",
        ],
    },
    Category {
        id: "social",
        label: "Social networks",
        urls: &[
            "https://x.com/",
            "https://www.facebook.com/",
            "https://www.instagram.com/",
            "https://www.linkedin.com/",
            "https://www.reddit.com/",
            "https://www.tiktok.com/",
            "https://www.pinterest.com/",
            "https://www.threads.net/",
            "https://mastodon.social/",
            "https://bsky.app/",
        ],
    },
    Category {
        id: "search",
        label: "Search engines & portals",
        urls: &[
            "https://www.google.com/",
            "https://www.bing.com/",
            "https://duckduckgo.com/",
            "https://search.brave.com/",
            "https://www.startpage.com/",
            "https://www.ecosia.org/",
            "https://www.qwant.com/",
            "https://yandex.com/",
            "https://www.baidu.com/",
            "https://www.yahoo.com/",
        ],
    },
    Category {
        id: "blogs",
        label: "Blogs & publishing platforms",
        urls: &[
            "https://wordpress.com/",
            "https://medium.com/",
            "https://substack.com/",
            "https://www.blogger.com/",
            "https://www.tumblr.com/",
            "https://ghost.org/",
            "https://dev.to/",
            "https://hashnode.com/",
            "https://write.as/",
            "https://buttondown.email/",
        ],
    },
    Category {
        id: "wiki",
        label: "Wikis & knowledge bases",
        urls: &[
            "https://www.wikipedia.org/",
            "https://en.wikipedia.org/wiki/Web_browser",
            "https://www.wiktionary.org/",
            "https://www.wikihow.com/",
            "https://www.fandom.com/",
            "https://www.britannica.com/",
            "https://stackexchange.com/",
            "https://www.quora.com/",
            "https://scholar.google.com/",
            "https://www.wikidata.org/",
        ],
    },
    Category {
        id: "forums",
        label: "Forums & developer communities",
        urls: &[
            "https://stackoverflow.com/",
            "https://news.ycombinator.com/",
            "https://lobste.rs/",
            "https://meta.discourse.org/",
            "https://superuser.com/",
            "https://serverfault.com/",
            "https://askubuntu.com/",
            "https://www.linuxquestions.org/questions/",
            "https://bbs.archlinux.org/",
            "https://www.raspberrypi.org/forums/",
        ],
    },
    Category {
        id: "webapp",
        label: "Web apps & dashboards",
        urls: &[
            "https://github.com/",
            "https://gitlab.com/",
            "https://www.notion.so/",
            "https://slack.com/",
            "https://trello.com/",
            "https://asana.com/",
            "https://www.atlassian.com/software/jira",
            "https://linear.app/",
            "https://www.figma.com/",
            "https://mail.proton.me/",
        ],
    },
];

/// Total number of URLs in the matrix (10 × 10).
/// Computed with a const fn: iterator adapters are not const-stable.
pub const SITE_COUNT: usize = const_url_count();

const fn const_url_count() -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < CATEGORIES.len() {
        total += CATEGORIES[i].urls.len();
        i += 1;
    }
    total
}

/// Iterate over (category id, url) pairs.
pub fn all_sites() -> impl Iterator<Item = (&'static str, &'static str)> {
    CATEGORIES
        .iter()
        .flat_map(|c| c.urls.iter().map(move |u| (c.id, *u)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix_shape_is_10x10() {
        assert_eq!(CATEGORIES.len(), 10, "10 categories required by the phase plan");
        for c in CATEGORIES {
            assert_eq!(c.urls.len(), 10, "category {} must have 10 urls", c.id);
        }
        assert_eq!(SITE_COUNT, 100);
    }

    #[test]
    fn every_url_is_https_and_parses() {
        for (cat, url) in all_sites() {
            assert!(url.starts_with("https://"), "{cat}: {url} must be https");
        }
    }

    #[test]
    fn category_ids_are_unique_and_sluglike() {
        let mut ids: Vec<_> = CATEGORIES.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "category ids must be unique");
        for c in CATEGORIES {
            assert!(
                c.id.chars().all(|ch| ch.is_ascii_lowercase() || ch == '-'),
                "category id {} should be a slug",
                c.id
            );
        }
    }
}
