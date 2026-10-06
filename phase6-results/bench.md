# brow-phase6 — 10-category page benchmark

| Category | OK/Err/T/O | TTFB p50 | TTFB p90 | Total p50 | Body p50 | Parse p50 | Nodes p50 |
|---|---|---:|---:|---:|---:|---:|---:|
| news | 4/6/0/0 | 35 ms | 117 ms | 51 ms | 774 B | 2 ms | 1157 |
| ecommerce | 7/2/0/1 | 185 ms | 1066 ms | 185 ms | 326.6 KiB | 3 ms | 1648 |
| docs | 7/3/0/0 | 193 ms | 598 ms | 193 ms | 3.7 KiB | 0 ms | 387 |
| video | 7/3/0/0 | 125 ms | 645 ms | 210 ms | 53.7 KiB | 1 ms | 416 |
| social | 7/3/0/0 | 272 ms | 884 ms | 333 ms | 50.1 KiB | 1 ms | 251 |
| search | 8/1/0/1 | 118 ms | 1071 ms | 187 ms | 83.5 KiB | 0 ms | 201 |
| blogs | 6/4/0/0 | 145 ms | 980 ms | 245 ms | 18.3 KiB | 1 ms | 823 |
| wiki | 3/7/0/0 | 134 ms | 337 ms | 134 ms | 5.4 KiB | 1 ms | 1103 |
| forums | 3/6/0/1 | 44 ms | 853 ms | 45 ms | 5.4 KiB | 0 ms | 821 |
| webapp | 7/3/0/0 | 173 ms | 556 ms | 398 ms | 249.2 KiB | 4 ms | 1809 |

## Per-URL detail

| Category | URL | Ver | Status | Connect | TTFB | Total | Bytes | Outcome | Nodes | Parse |
|---|---|---|---:|---:|---:|---:|---:|---|---:|---:|
| news | https://www.reuters.com/ | HTTP/2 | 401 | 7 ms | 76 ms | 76 ms | 774 B | http-error | - | - |
| news | https://apnews.com/ | HTTP/2 | 403 | 10 ms | 22 ms | 22 ms | 5.4 KiB | http-error | - | - |
| news | https://www.bbc.com/news | HTTP/2 | 200 | 10 ms | 31 ms | 48 ms | 368.2 KiB | ok | 1366 | 2 ms |
| news | https://www.cnn.com/ | HTTP/2 | 302 | 11 ms | 29 ms | 29 ms | 0 B | http-error | - | - |
| news | https://www.nytimes.com/ | HTTP/2 | 403 | 10 ms | 117 ms | 117 ms | 774 B | http-error | - | - |
| news | https://www.theguardian.com/ | HTTP/2 | 302 | 10 ms | 21 ms | 21 ms | 0 B | http-error | - | - |
| news | https://www.aljazeera.com/ | HTTP/2 | 200 | 10 ms | 35 ms | 57 ms | 395.6 KiB | ok | 1157 | 2 ms |
| news | https://www.npr.org/ | HTTP/2 | 200 | 10 ms | 54 ms | 79 ms | 769.8 KiB | ok | 4194 | 10 ms |
| news | https://www.dw.com/ | HTTP/2 | 200 | 7 ms | 328 ms | 328 ms | 11.8 KiB | ok | 35 | 0 ms |
| news | https://www.france24.com/ | HTTP/2 | 302 | 6 ms | 37 ms | 51 ms | 0 B | http-error | - | - |
| ecommerce | https://www.amazon.com/ | HTTP/2 | 200 | 5 ms | 311 ms | 536 ms | 285.9 KiB | ok | 197 | 23 ms |
| ecommerce | https://www.ebay.com/ | HTTP/2 | 403 | 6 ms | 43 ms | 43 ms | 1.8 KiB | http-error | - | - |
| ecommerce | https://www.walmart.com/ | HTTP/2 | 200 | 11 ms | 45 ms | 58 ms | 326.6 KiB | ok | 622 | 1 ms |
| ecommerce | https://www.etsy.com/ | HTTP/2 | 403 | 55 ms | 185 ms | 185 ms | 776 B | http-error | - | - |
| ecommerce | https://www.aliexpress.com/ | HTTP/2 | 200 | 6 ms | 504 ms | 620 ms | 432.6 KiB | ok | 655 | 1 ms |
| ecommerce | https://www.bestbuy.com/ |  | 0 | 4 ms | 37 ms | 37 ms | 0 B | transport-error | - | - |
| ecommerce | https://www.target.com/ | HTTP/2 | 200 | 6 ms | 1033 ms | 1237 ms | 483.3 KiB | ok | 1769 | 3 ms |
| ecommerce | https://www.newegg.com/ | HTTP/2 | 200 | 6 ms | 40 ms | 63 ms | 710.8 KiB | ok | 2345 | 3 ms |
| ecommerce | https://www.wayfair.com/ | HTTP/2 | 200 | 10 ms | 1526 ms | 1697 ms | 965.9 KiB | ok | 1648 | 4 ms |
| ecommerce | https://www.flipkart.com/ | HTTP/2 | 200 | 4 ms | 1066 ms | 1232 ms | 1.1 MiB | ok | 1808 | 4 ms |
| docs | https://developer.mozilla.org/ | HTTP/2 | 302 | 53 ms | 403 ms | 403 ms | 36 B | http-error | - | - |
| docs | https://docs.python.org/3/ | HTTP/2 | 200 | 53 ms | 162 ms | 170 ms | 19.2 KiB | ok | 387 | 0 ms |
| docs | https://doc.rust-lang.org/book/ | HTTP/2 | 200 | 8 ms | 24 ms | 25 ms | 22.0 KiB | ok | 192 | 0 ms |
| docs | https://learn.microsoft.com/ | HTTP/2 | 302 | 5 ms | 193 ms | 193 ms | 0 B | http-error | - | - |
| docs | https://docs.aws.amazon.com/ | HTTP/1.1 | 200 | 5 ms | 212 ms | 232 ms | 588.9 KiB | ok | 1371 | 2 ms |
| docs | https://developer.apple.com/ | HTTP/1.1 | 200 | 55 ms | 29 ms | 44 ms | 133.8 KiB | ok | 1249 | 2 ms |
| docs | https://docs.oracle.com/ | HTTP/2 | 200 | 10 ms | 43 ms | 43 ms | 485 B | ok | 9 | 0 ms |
| docs | https://go.dev/doc/ | HTTP/2 | 200 | 19 ms | 271 ms | 328 ms | 46.7 KiB | ok | 575 | 1 ms |
| docs | https://kotlinlang.org/docs/home.html | HTTP/2 | 200 | 12 ms | 691 ms | 691 ms | 3.7 KiB | ok | 46 | 0 ms |
| docs | https://www.php.net/manual/ | HTTP/2 | 302 | 12 ms | 598 ms | 598 ms | 0 B | http-error | - | - |
| video | https://www.youtube.com/ | HTTP/2 | 200 | 5 ms | 124 ms | 293 ms | 870.2 KiB | ok | 416 | 1 ms |
| video | https://vimeo.com/ | HTTP/2 | 200 | 6 ms | 39 ms | 80 ms | 1.7 MiB | ok | 2759 | 8 ms |
| video | https://www.twitch.tv/ | HTTP/2 | 200 | 51 ms | 125 ms | 210 ms | 198.1 KiB | ok | 183 | 0 ms |
| video | https://www.dailymotion.com/ | HTTP/2 | 200 | 9 ms | 230 ms | 234 ms | 53.7 KiB | ok | 75 | 0 ms |
| video | https://www.bilibili.com/ | HTTP/2 | 200 | 4 ms | 61 ms | 61 ms | 1.3 KiB | ok | 19 | 0 ms |
| video | https://rumble.com/ | HTTP/2 | 403 | 9 ms | 25 ms | 25 ms | 5.5 KiB | http-error | - | - |
| video | https://www.netflix.com/ | HTTP/2 | 200 | 5 ms | 715 ms | 3884 ms | 3.0 MiB | ok | 721 | 10 ms |
| video | https://www.disneyplus.com/ | HTTP/1.1 | 200 | 8 ms | 125 ms | 172 ms | 1.8 MiB | ok | 3580 | 9 ms |
| video | https://www.primevideo.com/ | HTTP/2 | 302 | 10 ms | 220 ms | 220 ms | 0 B | http-error | - | - |
| video | https://www.peacocktv.com/ | HTTP/2 | 302 | 16 ms | 645 ms | 645 ms | 0 B | http-error | - | - |
| social | https://x.com/ | HTTP/2 | 200 | 6 ms | 238 ms | 242 ms | 34.1 KiB | ok | 225 | 0 ms |
| social | https://www.facebook.com/ | HTTP/2 | 200 | 5 ms | 272 ms | 483 ms | 451.7 KiB | ok | 379 | 1 ms |
| social | https://www.instagram.com/ | HTTP/2 | 200 | 6 ms | 267 ms | 972 ms | 483.8 KiB | ok | 532 | 1 ms |
| social | https://www.linkedin.com/ | HTTP/2 | 200 | 7 ms | 284 ms | 298 ms | 136.7 KiB | ok | 853 | 1 ms |
| social | https://www.reddit.com/ | HTTP/2 | 403 | 7 ms | 22 ms | 32 ms | 185.8 KiB | http-error | - | - |
| social | https://www.tiktok.com/ | HTTP/2 | 302 | 6 ms | 232 ms | 232 ms | 136 B | http-error | - | - |
| social | https://www.pinterest.com/ | HTTP/2 | 200 | 7 ms | 335 ms | 365 ms | 1.1 MiB | ok | 251 | 2 ms |
| social | https://www.threads.net/ | HTTP/2 | 301 | 129 ms | 333 ms | 333 ms | 0 B | http-error | - | - |
| social | https://mastodon.social/ | HTTP/2 | 200 | 54 ms | 1407 ms | 1464 ms | 50.1 KiB | ok | 218 | 0 ms |
| social | https://bsky.app/ | HTTP/2 | 200 | 217 ms | 884 ms | 884 ms | 7.3 KiB | ok | 50 | 0 ms |
| search | https://www.google.com/ | HTTP/2 | 200 | 6 ms | 84 ms | 92 ms | 83.5 KiB | ok | 87 | 0 ms |
| search | https://www.bing.com/ | HTTP/2 | 200 | 5 ms | 118 ms | 159 ms | 64.3 KiB | ok | 128 | 0 ms |
| search | https://duckduckgo.com/ |  | 0 | - | 8003 ms | 8003 ms | 0 B | transport-error | - | - |
| search | https://search.brave.com/ | HTTP/2 | 200 | 9 ms | 65 ms | 187 ms | 790.2 KiB | ok | 339 | 1 ms |
| search | https://www.startpage.com/ | HTTP/2 | 200 | 183 ms | 875 ms | 915 ms | 17.1 KiB | ok | 34 | 0 ms |
| search | https://www.ecosia.org/ | HTTP/2 | 403 | 6 ms | 38 ms | 38 ms | 4.3 KiB | http-error | - | - |
| search | https://www.qwant.com/ | HTTP/2 | 200 | 268 ms | 1058 ms | 1578 ms | 138.9 KiB | ok | 356 | 0 ms |
| search | https://yandex.com/ | HTTP/2 | 200 | 234 ms | 1071 ms | 2220 ms | 377.0 KiB | ok | 201 | 1 ms |
| search | https://www.baidu.com/ | HTTP/1.1 | 200 | 11 ms | 54 ms | 85 ms | 712.3 KiB | ok | 407 | 2 ms |
| search | https://www.yahoo.com/ | HTTP/2 | 200 | 5 ms | 161 ms | 303 ms | 1017.6 KiB | ok | 2088 | 6 ms |
| blogs | https://wordpress.com/ | HTTP/2 | 403 | 8 ms | 19 ms | 19 ms | 7.2 KiB | http-error | - | - |
| blogs | https://medium.com/ | HTTP/2 | 403 | 8 ms | 26 ms | 26 ms | 5.4 KiB | http-error | - | - |
| blogs | https://substack.com/ | HTTP/2 | 200 | 9 ms | 554 ms | 565 ms | 236.0 KiB | ok | 823 | 1 ms |
| blogs | https://www.blogger.com/ | HTTP/2 | 302 | 18 ms | 240 ms | 245 ms | 0 B | http-error | - | - |
| blogs | https://www.tumblr.com/ | HTTP/2 | 200 | 6 ms | 935 ms | 1169 ms | 116.6 KiB | ok | 198 | 0 ms |
| blogs | https://ghost.org/ | HTTP/2 | 200 | 55 ms | 145 ms | 268 ms | 118.4 KiB | ok | 900 | 1 ms |
| blogs | https://dev.to/ | HTTP/2 | 200 | 59 ms | 1177 ms | 1354 ms | 263.6 KiB | ok | 1879 | 3 ms |
| blogs | https://hashnode.com/ | HTTP/2 | 200 | 15 ms | 71 ms | 90 ms | 368.3 KiB | ok | 1633 | 3 ms |
| blogs | https://write.as/ | HTTP/1.1 | 200 | 389 ms | 980 ms | 984 ms | 18.3 KiB | ok | 285 | 0 ms |
| blogs | https://buttondown.email/ | HTTP/2 | 302 | 6 ms | 41 ms | 41 ms | 0 B | http-error | - | - |
| wiki | https://www.wikipedia.org/ | HTTP/2 | 200 | 42 ms | 183 ms | 257 ms | 91.8 KiB | ok | 1103 | 1 ms |
| wiki | https://en.wikipedia.org/wiki/Web_browser | HTTP/2 | 200 | 45 ms | 143 ms | 356 ms | 405.4 KiB | ok | 4057 | 5 ms |
| wiki | https://www.wiktionary.org/ | HTTP/2 | 200 | 38 ms | 337 ms | 425 ms | 75.0 KiB | ok | 680 | 0 ms |
| wiki | https://www.wikihow.com/ | HTTP/2 | 301 | 53 ms | 167 ms | 167 ms | 0 B | http-error | - | - |
| wiki | https://www.fandom.com/ | HTTP/2 | 403 | 13 ms | 53 ms | 53 ms | 5.4 KiB | http-error | - | - |
| wiki | https://www.britannica.com/ | HTTP/2 | 403 | 11 ms | 31 ms | 31 ms | 5.4 KiB | http-error | - | - |
| wiki | https://stackexchange.com/ | HTTP/2 | 403 | 11 ms | 32 ms | 32 ms | 5.4 KiB | http-error | - | - |
| wiki | https://www.quora.com/ | HTTP/2 | 403 | 11 ms | 35 ms | 35 ms | 5.4 KiB | http-error | - | - |
| wiki | https://scholar.google.com/ | HTTP/2 | 403 | 17 ms | 2078 ms | 2078 ms | 2.2 KiB | http-error | - | - |
| wiki | https://www.wikidata.org/ | HTTP/2 | 301 | 46 ms | 134 ms | 134 ms | 0 B | http-error | - | - |
| forums | https://stackoverflow.com/ | HTTP/2 | 403 | 8 ms | 26 ms | 26 ms | 5.4 KiB | http-error | - | - |
| forums | https://news.ycombinator.com/ | HTTP/2 | 200 | 181 ms | 537 ms | 1677 ms | 34.0 KiB | ok | 821 | 0 ms |
| forums | https://lobste.rs/ |  | 0 | - | 8003 ms | 8003 ms | 0 B | transport-error | - | - |
| forums | https://meta.discourse.org/ | HTTP/2 | 200 | 217 ms | 777 ms | 1401 ms | 103.3 KiB | ok | 933 | 1 ms |
| forums | https://superuser.com/ | HTTP/2 | 403 | 8 ms | 28 ms | 29 ms | 5.4 KiB | http-error | - | - |
| forums | https://serverfault.com/ | HTTP/2 | 403 | 8 ms | 29 ms | 29 ms | 5.4 KiB | http-error | - | - |
| forums | https://askubuntu.com/ | HTTP/2 | 403 | 8 ms | 23 ms | 24 ms | 5.4 KiB | http-error | - | - |
| forums | https://www.linuxquestions.org/questions/ | HTTP/2 | 403 | 10 ms | 44 ms | 45 ms | 5.7 KiB | http-error | - | - |
| forums | https://bbs.archlinux.org/ | HTTP/1.1 | 200 | 103 ms | 853 ms | 858 ms | 23.0 KiB | ok | 546 | 0 ms |
| forums | https://www.raspberrypi.org/forums/ | HTTP/2 | 302 | 44 ms | 91 ms | 91 ms | 504 B | http-error | - | - |
| webapp | https://github.com/ | HTTP/2 | 200 | 35 ms | 107 ms | 277 ms | 563.7 KiB | ok | 1809 | 4 ms |
| webapp | https://gitlab.com/ | HTTP/2 | 301 | 6 ms | 399 ms | 399 ms | 91 B | http-error | - | - |
| webapp | https://www.notion.so/ | HTTP/2 | 307 | 8 ms | 99 ms | 99 ms | 0 B | http-error | - | - |
| webapp | https://slack.com/ | HTTP/2 | 200 | 42 ms | 455 ms | 647 ms | 249.2 KiB | ok | 2150 | 3 ms |
| webapp | https://trello.com/ | HTTP/2 | 200 | 11 ms | 56 ms | 93 ms | 521.4 KiB | ok | 1209 | 2 ms |
| webapp | https://asana.com/ | HTTP/2 | 200 | 7 ms | 173 ms | 398 ms | 898.0 KiB | ok | 2471 | 4 ms |
| webapp | https://www.atlassian.com/software/jira | HTTP/2 | 405 | 4 ms | 37 ms | 37 ms | 2.2 KiB | http-error | - | - |
| webapp | https://linear.app/ | HTTP/2 | 200 | 9 ms | 731 ms | 759 ms | 1.2 MiB | ok | 4968 | 8 ms |
| webapp | https://www.figma.com/ | HTTP/2 | 200 | 82 ms | 390 ms | 812 ms | 1.6 MiB | ok | 1376 | 4 ms |
| webapp | https://mail.proton.me/ | HTTP/2 | 200 | 180 ms | 556 ms | 577 ms | 4.7 KiB | ok | 62 | 0 ms |
