# brow-phase6 — stress test

- rounds: 2 · requests: 200 · ok: 119 · failed: 81 · success rate: 59.5%
- TTFB p50 / p90 / p99: 143 ms / 918 ms / 8002 ms ms
- Total p50 / p99: 191 ms / 8002 ms ms
- wall: 113.9 s · throughput: 1.76 req/s
- harness RSS p50 / max: 21908 / 23776 KiB

Per-category (stress):

| Category | OK/Err/T/O | TTFB p50 | TTFB p90 | Total p50 | Body p50 | Parse p50 | Nodes p50 |
|---|---|---:|---:|---:|---:|---:|---:|
| news | 8/12/0/0 | 51 ms | 111 ms | 70 ms | 774 B | 2 ms | 1157 |
| ecommerce | 14/4/0/2 | 198 ms | 1018 ms | 283 ms | 326.6 KiB | 3 ms | 1662 |
| docs | 14/6/0/0 | 162 ms | 361 ms | 166 ms | 3.7 KiB | 0 ms | 387 |
| video | 14/6/0/0 | 128 ms | 306 ms | 182 ms | 53.6 KiB | 1 ms | 416 |
| social | 14/6/0/0 | 244 ms | 1052 ms | 373 ms | 50.1 KiB | 1 ms | 265 |
| search | 16/2/0/2 | 154 ms | 1098 ms | 254 ms | 83.5 KiB | 0 ms | 201 |
| blogs | 12/8/0/0 | 202 ms | 918 ms | 269 ms | 18.3 KiB | 1 ms | 831 |
| wiki | 6/14/0/0 | 118 ms | 303 ms | 124 ms | 5.4 KiB | 1 ms | 1103 |
| forums | 6/12/0/2 | 82 ms | 843 ms | 82 ms | 5.4 KiB | 0 ms | 821 |
| webapp | 15/5/0/0 | 183 ms | 672 ms | 404 ms | 521.4 KiB | 4 ms | 1807 |

## Per-URL detail

| Category | URL | Ver | Status | Connect | TTFB | Total | Bytes | Outcome | Nodes | Parse |
|---|---|---|---:|---:|---:|---:|---:|---|---:|---:|
| news | https://www.reuters.com/ | HTTP/2 | 401 | - | 146 ms | 146 ms | 774 B | http-error | - | - |
| news | https://apnews.com/ | HTTP/2 | 403 | - | 37 ms | 37 ms | 5.4 KiB | http-error | - | - |
| news | https://www.bbc.com/news | HTTP/2 | 200 | - | 27 ms | 45 ms | 367.5 KiB | ok | 1366 | 2 ms |
| news | https://www.cnn.com/ | HTTP/2 | 302 | - | 26 ms | 26 ms | 0 B | http-error | - | - |
| news | https://www.nytimes.com/ | HTTP/2 | 403 | - | 106 ms | 106 ms | 774 B | http-error | - | - |
| news | https://www.theguardian.com/ | HTTP/2 | 302 | - | 17 ms | 17 ms | 0 B | http-error | - | - |
| news | https://www.aljazeera.com/ | HTTP/2 | 200 | - | 31 ms | 52 ms | 395.6 KiB | ok | 1157 | 2 ms |
| news | https://www.npr.org/ | HTTP/2 | 200 | - | 55 ms | 81 ms | 769.2 KiB | ok | 4192 | 10 ms |
| news | https://www.dw.com/ | HTTP/2 | 200 | - | 97 ms | 97 ms | 11.8 KiB | ok | 35 | 0 ms |
| news | https://www.france24.com/ | HTTP/2 | 302 | - | 51 ms | 79 ms | 0 B | http-error | - | - |
| ecommerce | https://www.amazon.com/ | HTTP/2 | 200 | - | 313 ms | 1049 ms | 285.2 KiB | ok | 210 | 21 ms |
| ecommerce | https://www.ebay.com/ | HTTP/2 | 403 | - | 71 ms | 71 ms | 1.8 KiB | http-error | - | - |
| ecommerce | https://www.walmart.com/ | HTTP/2 | 200 | - | 185 ms | 283 ms | 326.6 KiB | ok | 622 | 1 ms |
| ecommerce | https://www.etsy.com/ | HTTP/2 | 403 | - | 181 ms | 181 ms | 776 B | http-error | - | - |
| ecommerce | https://www.aliexpress.com/ | HTTP/2 | 200 | - | 730 ms | 881 ms | 432.8 KiB | ok | 660 | 1 ms |
| ecommerce | https://www.bestbuy.com/ |  | 0 | - | 62 ms | 62 ms | 0 B | transport-error | - | - |
| ecommerce | https://www.target.com/ | HTTP/2 | 200 | - | 1021 ms | 1083 ms | 483.6 KiB | ok | 1769 | 3 ms |
| ecommerce | https://www.newegg.com/ | HTTP/2 | 200 | - | 38 ms | 58 ms | 711.9 KiB | ok | 2341 | 3 ms |
| ecommerce | https://www.wayfair.com/ | HTTP/2 | 200 | - | 1344 ms | 1794 ms | 971.8 KiB | ok | 1664 | 4 ms |
| ecommerce | https://www.flipkart.com/ | HTTP/2 | 200 | - | 599 ms | 644 ms | 1.2 MiB | ok | 1808 | 4 ms |
| docs | https://developer.mozilla.org/ | HTTP/2 | 302 | - | 166 ms | 166 ms | 36 B | http-error | - | - |
| docs | https://docs.python.org/3/ | HTTP/2 | 200 | - | 162 ms | 172 ms | 19.2 KiB | ok | 387 | 0 ms |
| docs | https://doc.rust-lang.org/book/ | HTTP/2 | 200 | - | 23 ms | 24 ms | 22.0 KiB | ok | 192 | 0 ms |
| docs | https://learn.microsoft.com/ | HTTP/2 | 302 | - | 191 ms | 191 ms | 0 B | http-error | - | - |
| docs | https://docs.aws.amazon.com/ | HTTP/1.1 | 200 | - | 27 ms | 50 ms | 588.9 KiB | ok | 1371 | 2 ms |
| docs | https://developer.apple.com/ | HTTP/1.1 | 200 | - | 19 ms | 28 ms | 133.8 KiB | ok | 1249 | 2 ms |
| docs | https://docs.oracle.com/ | HTTP/2 | 200 | - | 56 ms | 56 ms | 485 B | ok | 9 | 0 ms |
| docs | https://go.dev/doc/ | HTTP/2 | 200 | - | 291 ms | 351 ms | 46.7 KiB | ok | 575 | 0 ms |
| docs | https://kotlinlang.org/docs/home.html | HTTP/2 | 200 | - | 599 ms | 600 ms | 3.7 KiB | ok | 46 | 0 ms |
| docs | https://www.php.net/manual/ | HTTP/2 | 302 | - | 361 ms | 361 ms | 0 B | http-error | - | - |
| video | https://www.youtube.com/ | HTTP/2 | 200 | - | 129 ms | 248 ms | 871.5 KiB | ok | 416 | 1 ms |
| video | https://vimeo.com/ | HTTP/2 | 200 | - | 43 ms | 79 ms | 1.7 MiB | ok | 2759 | 7 ms |
| video | https://www.twitch.tv/ | HTTP/2 | 200 | - | 128 ms | 224 ms | 198.1 KiB | ok | 183 | 0 ms |
| video | https://www.dailymotion.com/ | HTTP/2 | 200 | - | 289 ms | 523 ms | 53.5 KiB | ok | 75 | 0 ms |
| video | https://www.bilibili.com/ | HTTP/2 | 200 | - | 62 ms | 62 ms | 1.3 KiB | ok | 19 | 0 ms |
| video | https://rumble.com/ | HTTP/2 | 403 | - | 31 ms | 31 ms | 5.5 KiB | http-error | - | - |
| video | https://www.netflix.com/ | HTTP/2 | 200 | - | 1162 ms | 4611 ms | 3.0 MiB | ok | 721 | 9 ms |
| video | https://www.disneyplus.com/ | HTTP/1.1 | 200 | - | 142 ms | 182 ms | 1.8 MiB | ok | 3580 | 9 ms |
| video | https://www.primevideo.com/ | HTTP/2 | 302 | - | 217 ms | 217 ms | 0 B | http-error | - | - |
| video | https://www.peacocktv.com/ | HTTP/2 | 403 | - | 56 ms | 56 ms | 371 B | http-error | - | - |
| social | https://x.com/ | HTTP/2 | 200 | - | 226 ms | 228 ms | 34.1 KiB | ok | 225 | 0 ms |
| social | https://www.facebook.com/ | HTTP/2 | 200 | - | 173 ms | 373 ms | 451.9 KiB | ok | 377 | 1 ms |
| social | https://www.instagram.com/ | HTTP/2 | 200 | - | 302 ms | 800 ms | 480.6 KiB | ok | 524 | 1 ms |
| social | https://www.linkedin.com/ | HTTP/2 | 200 | - | 430 ms | 453 ms | 476.9 KiB | ok | 586 | 1 ms |
| social | https://www.reddit.com/ | HTTP/2 | 403 | - | 25 ms | 35 ms | 185.8 KiB | http-error | - | - |
| social | https://www.tiktok.com/ | HTTP/2 | 302 | - | 68 ms | 68 ms | 136 B | http-error | - | - |
| social | https://www.pinterest.com/ | HTTP/2 | 200 | - | 500 ms | 861 ms | 1.1 MiB | ok | 766 | 3 ms |
| social | https://www.threads.net/ | HTTP/2 | 301 | - | 1306 ms | 1306 ms | 0 B | http-error | - | - |
| social | https://mastodon.social/ | HTTP/2 | 200 | - | 228 ms | 284 ms | 50.1 KiB | ok | 218 | 0 ms |
| social | https://bsky.app/ | HTTP/2 | 200 | - | 916 ms | 916 ms | 7.3 KiB | ok | 50 | 0 ms |
| search | https://www.google.com/ | HTTP/2 | 200 | - | 87 ms | 96 ms | 83.5 KiB | ok | 87 | 0 ms |
| search | https://www.bing.com/ | HTTP/2 | 200 | - | 208 ms | 254 ms | 64.3 KiB | ok | 128 | 0 ms |
| search | https://duckduckgo.com/ |  | 0 | - | 8002 ms | 8002 ms | 0 B | transport-error | - | - |
| search | https://search.brave.com/ | HTTP/2 | 200 | - | 25 ms | 45 ms | 790.2 KiB | ok | 339 | 1 ms |
| search | https://www.startpage.com/ | HTTP/2 | 200 | - | 736 ms | 828 ms | 17.1 KiB | ok | 34 | 0 ms |
| search | https://www.ecosia.org/ | HTTP/2 | 403 | - | 97 ms | 97 ms | 4.3 KiB | http-error | - | - |
| search | https://www.qwant.com/ | HTTP/2 | 200 | - | 1057 ms | 1577 ms | 138.9 KiB | ok | 356 | 0 ms |
| search | https://yandex.com/ | HTTP/2 | 200 | - | 1098 ms | 2325 ms | 377.9 KiB | ok | 201 | 1 ms |
| search | https://www.baidu.com/ | HTTP/1.1 | 200 | - | 43 ms | 68 ms | 712.6 KiB | ok | 407 | 2 ms |
| search | https://www.yahoo.com/ | HTTP/2 | 200 | - | 154 ms | 278 ms | 1020.8 KiB | ok | 2074 | 6 ms |
| blogs | https://wordpress.com/ | HTTP/2 | 403 | - | 16 ms | 16 ms | 7.2 KiB | http-error | - | - |
| blogs | https://medium.com/ | HTTP/2 | 403 | - | 27 ms | 27 ms | 5.4 KiB | http-error | - | - |
| blogs | https://substack.com/ | HTTP/2 | 200 | - | 631 ms | 645 ms | 236.0 KiB | ok | 823 | 1 ms |
| blogs | https://www.blogger.com/ | HTTP/2 | 302 | - | 264 ms | 269 ms | 0 B | http-error | - | - |
| blogs | https://www.tumblr.com/ | HTTP/2 | 200 | - | 424 ms | 1212 ms | 113.0 KiB | ok | 198 | 0 ms |
| blogs | https://ghost.org/ | HTTP/2 | 200 | - | 121 ms | 222 ms | 118.4 KiB | ok | 900 | 1 ms |
| blogs | https://dev.to/ | HTTP/2 | 200 | - | 231 ms | 398 ms | 263.6 KiB | ok | 1879 | 3 ms |
| blogs | https://hashnode.com/ | HTTP/2 | 200 | - | 202 ms | 216 ms | 368.3 KiB | ok | 1633 | 3 ms |
| blogs | https://write.as/ | HTTP/1.1 | 200 | - | 724 ms | 727 ms | 18.3 KiB | ok | 285 | 0 ms |
| blogs | https://buttondown.email/ | HTTP/2 | 302 | - | 54 ms | 54 ms | 0 B | http-error | - | - |
| wiki | https://www.wikipedia.org/ | HTTP/2 | 200 | - | 138 ms | 232 ms | 91.8 KiB | ok | 1103 | 1 ms |
| wiki | https://en.wikipedia.org/wiki/Web_browser | HTTP/2 | 200 | - | 134 ms | 325 ms | 405.4 KiB | ok | 4057 | 6 ms |
| wiki | https://www.wiktionary.org/ | HTTP/2 | 200 | - | 143 ms | 228 ms | 75.0 KiB | ok | 680 | 0 ms |
| wiki | https://www.wikihow.com/ | HTTP/2 | 301 | - | 160 ms | 160 ms | 0 B | http-error | - | - |
| wiki | https://www.fandom.com/ | HTTP/2 | 403 | - | 26 ms | 26 ms | 5.4 KiB | http-error | - | - |
| wiki | https://www.britannica.com/ | HTTP/2 | 403 | - | 25 ms | 25 ms | 5.4 KiB | http-error | - | - |
| wiki | https://stackexchange.com/ | HTTP/2 | 403 | - | 32 ms | 32 ms | 5.4 KiB | http-error | - | - |
| wiki | https://www.quora.com/ | HTTP/2 | 403 | - | 32 ms | 32 ms | 5.4 KiB | http-error | - | - |
| wiki | https://scholar.google.com/ | HTTP/2 | 403 | - | 1999 ms | 1999 ms | 2.2 KiB | http-error | - | - |
| wiki | https://www.wikidata.org/ | HTTP/2 | 301 | - | 111 ms | 111 ms | 0 B | http-error | - | - |
| forums | https://stackoverflow.com/ | HTTP/2 | 403 | - | 26 ms | 26 ms | 5.4 KiB | http-error | - | - |
| forums | https://news.ycombinator.com/ | HTTP/2 | 200 | - | 716 ms | 1420 ms | 34.0 KiB | ok | 821 | 0 ms |
| forums | https://lobste.rs/ |  | 0 | - | 8003 ms | 8003 ms | 0 B | transport-error | - | - |
| forums | https://meta.discourse.org/ | HTTP/2 | 200 | - | 767 ms | 1405 ms | 103.3 KiB | ok | 933 | 1 ms |
| forums | https://superuser.com/ | HTTP/2 | 403 | - | 28 ms | 28 ms | 5.4 KiB | http-error | - | - |
| forums | https://serverfault.com/ | HTTP/2 | 403 | - | 46 ms | 47 ms | 5.4 KiB | http-error | - | - |
| forums | https://askubuntu.com/ | HTTP/2 | 403 | - | 30 ms | 30 ms | 5.4 KiB | http-error | - | - |
| forums | https://www.linuxquestions.org/questions/ | HTTP/2 | 403 | - | 51 ms | 51 ms | 5.7 KiB | http-error | - | - |
| forums | https://bbs.archlinux.org/ | HTTP/1.1 | 200 | - | 843 ms | 851 ms | 22.9 KiB | ok | 544 | 0 ms |
| forums | https://www.raspberrypi.org/forums/ | HTTP/2 | 302 | - | 85 ms | 85 ms | 504 B | http-error | - | - |
| webapp | https://github.com/ | HTTP/2 | 200 | - | 107 ms | 274 ms | 563.6 KiB | ok | 1807 | 4 ms |
| webapp | https://gitlab.com/ | HTTP/2 | 301 | - | 672 ms | 672 ms | 91 B | http-error | - | - |
| webapp | https://www.notion.so/ | HTTP/2 | 307 | - | 108 ms | 108 ms | 0 B | http-error | - | - |
| webapp | https://slack.com/ | HTTP/2 | 200 | - | 442 ms | 663 ms | 237.5 KiB | ok | 1992 | 3 ms |
| webapp | https://trello.com/ | HTTP/2 | 200 | - | 22 ms | 35 ms | 521.4 KiB | ok | 1209 | 2 ms |
| webapp | https://asana.com/ | HTTP/2 | 200 | - | 183 ms | 407 ms | 898.0 KiB | ok | 2471 | 4 ms |
| webapp | https://www.atlassian.com/software/jira | HTTP/2 | 200 | - | 673 ms | 786 ms | 1.2 MiB | ok | 1164 | 8 ms |
| webapp | https://linear.app/ | HTTP/2 | 200 | - | 591 ms | 619 ms | 1.2 MiB | ok | 4968 | 8 ms |
| webapp | https://www.figma.com/ | HTTP/2 | 200 | - | 365 ms | 581 ms | 1.6 MiB | ok | 1376 | 4 ms |
| webapp | https://mail.proton.me/ | HTTP/2 | 200 | - | 619 ms | 631 ms | 4.7 KiB | ok | 62 | 0 ms |
| news | https://www.reuters.com/ | HTTP/2 | 401 | - | 75 ms | 75 ms | 774 B | http-error | - | - |
| news | https://apnews.com/ | HTTP/2 | 403 | - | 28 ms | 28 ms | 5.4 KiB | http-error | - | - |
| news | https://www.bbc.com/news | HTTP/2 | 200 | - | 111 ms | 122 ms | 363.6 KiB | ok | 1326 | 2 ms |
| news | https://www.cnn.com/ | HTTP/2 | 302 | - | 27 ms | 27 ms | 0 B | http-error | - | - |
| news | https://www.nytimes.com/ | HTTP/2 | 403 | - | 290 ms | 290 ms | 774 B | http-error | - | - |
| news | https://www.theguardian.com/ | HTTP/2 | 302 | - | 23 ms | 23 ms | 0 B | http-error | - | - |
| news | https://www.aljazeera.com/ | HTTP/2 | 200 | - | 47 ms | 71 ms | 395.6 KiB | ok | 1157 | 2 ms |
| news | https://www.npr.org/ | HTTP/2 | 200 | - | 59 ms | 104 ms | 769.8 KiB | ok | 4194 | 10 ms |
| news | https://www.dw.com/ | HTTP/2 | 200 | - | 56 ms | 57 ms | 11.8 KiB | ok | 35 | 0 ms |
| news | https://www.france24.com/ | HTTP/2 | 302 | - | 61 ms | 70 ms | 0 B | http-error | - | - |
| ecommerce | https://www.amazon.com/ | HTTP/2 | 200 | - | 285 ms | 996 ms | 283.7 KiB | ok | 193 | 24 ms |
| ecommerce | https://www.ebay.com/ | HTTP/2 | 403 | - | 89 ms | 89 ms | 1.8 KiB | http-error | - | - |
| ecommerce | https://www.walmart.com/ | HTTP/2 | 200 | - | 49 ms | 70 ms | 326.6 KiB | ok | 622 | 1 ms |
| ecommerce | https://www.etsy.com/ | HTTP/2 | 403 | - | 170 ms | 170 ms | 776 B | http-error | - | - |
| ecommerce | https://www.aliexpress.com/ | HTTP/2 | 200 | - | 467 ms | 619 ms | 432.7 KiB | ok | 660 | 1 ms |
| ecommerce | https://www.bestbuy.com/ |  | 0 | - | 90 ms | 90 ms | 0 B | transport-error | - | - |
| ecommerce | https://www.target.com/ | HTTP/2 | 200 | - | 1018 ms | 1489 ms | 483.1 KiB | ok | 1769 | 3 ms |
| ecommerce | https://www.newegg.com/ | HTTP/2 | 200 | - | 198 ms | 222 ms | 711.9 KiB | ok | 2341 | 3 ms |
| ecommerce | https://www.wayfair.com/ | HTTP/2 | 200 | - | 953 ms | 1298 ms | 968.4 KiB | ok | 1662 | 4 ms |
| ecommerce | https://www.flipkart.com/ | HTTP/2 | 200 | - | 716 ms | 803 ms | 1.1 MiB | ok | 1808 | 5 ms |
| docs | https://developer.mozilla.org/ | HTTP/2 | 302 | - | 166 ms | 166 ms | 36 B | http-error | - | - |
| docs | https://docs.python.org/3/ | HTTP/2 | 200 | - | 158 ms | 169 ms | 19.2 KiB | ok | 387 | 0 ms |
| docs | https://doc.rust-lang.org/book/ | HTTP/2 | 200 | - | 24 ms | 24 ms | 22.0 KiB | ok | 192 | 0 ms |
| docs | https://learn.microsoft.com/ | HTTP/2 | 302 | - | 244 ms | 259 ms | 0 B | http-error | - | - |
| docs | https://docs.aws.amazon.com/ | HTTP/1.1 | 200 | - | 27 ms | 45 ms | 588.9 KiB | ok | 1371 | 2 ms |
| docs | https://developer.apple.com/ | HTTP/1.1 | 200 | - | 27 ms | 40 ms | 133.8 KiB | ok | 1249 | 2 ms |
| docs | https://docs.oracle.com/ | HTTP/2 | 200 | - | 36 ms | 36 ms | 485 B | ok | 9 | 0 ms |
| docs | https://go.dev/doc/ | HTTP/2 | 200 | - | 282 ms | 339 ms | 46.7 KiB | ok | 575 | 0 ms |
| docs | https://kotlinlang.org/docs/home.html | HTTP/2 | 200 | - | 649 ms | 649 ms | 3.7 KiB | ok | 46 | 0 ms |
| docs | https://www.php.net/manual/ | HTTP/2 | 302 | - | 216 ms | 216 ms | 0 B | http-error | - | - |
| video | https://www.youtube.com/ | HTTP/2 | 200 | - | 224 ms | 688 ms | 871.0 KiB | ok | 416 | 1 ms |
| video | https://vimeo.com/ | HTTP/2 | 200 | - | 34 ms | 60 ms | 1.7 MiB | ok | 2764 | 7 ms |
| video | https://www.twitch.tv/ | HTTP/2 | 200 | - | 153 ms | 266 ms | 198.1 KiB | ok | 183 | 0 ms |
| video | https://www.dailymotion.com/ | HTTP/2 | 200 | - | 306 ms | 572 ms | 53.6 KiB | ok | 75 | 0 ms |
| video | https://www.bilibili.com/ | HTTP/2 | 200 | - | 58 ms | 59 ms | 1.3 KiB | ok | 19 | 0 ms |
| video | https://rumble.com/ | HTTP/2 | 403 | - | 27 ms | 27 ms | 5.5 KiB | http-error | - | - |
| video | https://www.netflix.com/ | HTTP/2 | 200 | - | 950 ms | 4137 ms | 3.0 MiB | ok | 721 | 9 ms |
| video | https://www.disneyplus.com/ | HTTP/1.1 | 200 | - | 49 ms | 84 ms | 1.8 MiB | ok | 3582 | 9 ms |
| video | https://www.primevideo.com/ | HTTP/2 | 302 | - | 208 ms | 208 ms | 0 B | http-error | - | - |
| video | https://www.peacocktv.com/ | HTTP/2 | 403 | - | 41 ms | 41 ms | 369 B | http-error | - | - |
| social | https://x.com/ | HTTP/2 | 200 | - | 262 ms | 262 ms | 34.1 KiB | ok | 225 | 0 ms |
| social | https://www.facebook.com/ | HTTP/2 | 200 | - | 197 ms | 404 ms | 451.6 KiB | ok | 377 | 1 ms |
| social | https://www.instagram.com/ | HTTP/2 | 200 | - | 244 ms | 572 ms | 480.5 KiB | ok | 524 | 1 ms |
| social | https://www.linkedin.com/ | HTTP/2 | 200 | - | 432 ms | 441 ms | 136.7 KiB | ok | 853 | 1 ms |
| social | https://www.reddit.com/ | HTTP/2 | 403 | - | 20 ms | 28 ms | 185.8 KiB | http-error | - | - |
| social | https://www.tiktok.com/ | HTTP/2 | 302 | - | 83 ms | 87 ms | 136 B | http-error | - | - |
| social | https://www.pinterest.com/ | HTTP/2 | 200 | - | 316 ms | 360 ms | 1.3 MiB | ok | 265 | 2 ms |
| social | https://www.threads.net/ | HTTP/2 | 301 | - | 242 ms | 242 ms | 0 B | http-error | - | - |
| social | https://mastodon.social/ | HTTP/2 | 200 | - | 1296 ms | 1354 ms | 50.1 KiB | ok | 218 | 0 ms |
| social | https://bsky.app/ | HTTP/2 | 200 | - | 1052 ms | 1052 ms | 7.3 KiB | ok | 50 | 0 ms |
| search | https://www.google.com/ | HTTP/2 | 200 | - | 89 ms | 97 ms | 83.5 KiB | ok | 87 | 0 ms |
| search | https://www.bing.com/ | HTTP/2 | 200 | - | 125 ms | 136 ms | 64.3 KiB | ok | 128 | 0 ms |
| search | https://duckduckgo.com/ |  | 0 | - | 8002 ms | 8002 ms | 0 B | transport-error | - | - |
| search | https://search.brave.com/ | HTTP/2 | 200 | - | 28 ms | 48 ms | 790.2 KiB | ok | 339 | 1 ms |
| search | https://www.startpage.com/ | HTTP/2 | 200 | - | 812 ms | 885 ms | 17.1 KiB | ok | 34 | 0 ms |
| search | https://www.ecosia.org/ | HTTP/2 | 403 | - | 87 ms | 87 ms | 4.3 KiB | http-error | - | - |
| search | https://www.qwant.com/ | HTTP/2 | 200 | - | 1029 ms | 1554 ms | 138.9 KiB | ok | 356 | 0 ms |
| search | https://yandex.com/ | HTTP/2 | 200 | - | 989 ms | 2024 ms | 377.4 KiB | ok | 201 | 1 ms |
| search | https://www.baidu.com/ | HTTP/1.1 | 200 | - | 48 ms | 90 ms | 712.6 KiB | ok | 407 | 3 ms |
| search | https://www.yahoo.com/ | HTTP/2 | 200 | - | 268 ms | 490 ms | 1022.7 KiB | ok | 2092 | 6 ms |
| blogs | https://wordpress.com/ | HTTP/2 | 403 | - | 22 ms | 23 ms | 7.2 KiB | http-error | - | - |
| blogs | https://medium.com/ | HTTP/2 | 403 | - | 33 ms | 33 ms | 5.4 KiB | http-error | - | - |
| blogs | https://substack.com/ | HTTP/2 | 200 | - | 689 ms | 712 ms | 267.2 KiB | ok | 831 | 1 ms |
| blogs | https://www.blogger.com/ | HTTP/2 | 302 | - | 281 ms | 285 ms | 0 B | http-error | - | - |
| blogs | https://www.tumblr.com/ | HTTP/2 | 200 | - | 918 ms | 933 ms | 115.9 KiB | ok | 198 | 0 ms |
| blogs | https://ghost.org/ | HTTP/2 | 200 | - | 144 ms | 269 ms | 118.4 KiB | ok | 900 | 1 ms |
| blogs | https://dev.to/ | HTTP/2 | 200 | - | 959 ms | 1140 ms | 263.6 KiB | ok | 1879 | 3 ms |
| blogs | https://hashnode.com/ | HTTP/2 | 200 | - | 87 ms | 100 ms | 368.3 KiB | ok | 1633 | 2 ms |
| blogs | https://write.as/ | HTTP/1.1 | 200 | - | 925 ms | 928 ms | 18.3 KiB | ok | 285 | 0 ms |
| blogs | https://buttondown.email/ | HTTP/2 | 302 | - | 51 ms | 51 ms | 0 B | http-error | - | - |
| wiki | https://www.wikipedia.org/ | HTTP/2 | 200 | - | 136 ms | 228 ms | 91.8 KiB | ok | 1103 | 1 ms |
| wiki | https://en.wikipedia.org/wiki/Web_browser | HTTP/2 | 200 | - | 118 ms | 286 ms | 405.4 KiB | ok | 4057 | 6 ms |
| wiki | https://www.wiktionary.org/ | HTTP/2 | 200 | - | 414 ms | 472 ms | 75.0 KiB | ok | 680 | 0 ms |
| wiki | https://www.wikihow.com/ | HTTP/2 | 301 | - | 155 ms | 155 ms | 0 B | http-error | - | - |
| wiki | https://www.fandom.com/ | HTTP/2 | 403 | - | 34 ms | 34 ms | 5.4 KiB | http-error | - | - |
| wiki | https://www.britannica.com/ | HTTP/2 | 403 | - | 28 ms | 28 ms | 5.4 KiB | http-error | - | - |
| wiki | https://stackexchange.com/ | HTTP/2 | 403 | - | 29 ms | 29 ms | 5.4 KiB | http-error | - | - |
| wiki | https://www.quora.com/ | HTTP/2 | 403 | - | 26 ms | 26 ms | 5.4 KiB | http-error | - | - |
| wiki | https://scholar.google.com/ | HTTP/2 | 403 | - | 303 ms | 303 ms | 1.1 KiB | http-error | - | - |
| wiki | https://www.wikidata.org/ | HTTP/2 | 301 | - | 124 ms | 124 ms | 0 B | http-error | - | - |
| forums | https://stackoverflow.com/ | HTTP/2 | 403 | - | 25 ms | 25 ms | 5.4 KiB | http-error | - | - |
| forums | https://news.ycombinator.com/ | HTTP/2 | 200 | - | 586 ms | 1438 ms | 33.9 KiB | ok | 821 | 0 ms |
| forums | https://lobste.rs/ |  | 0 | - | 8001 ms | 8001 ms | 0 B | transport-error | - | - |
| forums | https://meta.discourse.org/ | HTTP/2 | 200 | - | 731 ms | 1155 ms | 103.3 KiB | ok | 933 | 1 ms |
| forums | https://superuser.com/ | HTTP/2 | 403 | - | 33 ms | 33 ms | 5.4 KiB | http-error | - | - |
| forums | https://serverfault.com/ | HTTP/2 | 403 | - | 84 ms | 84 ms | 5.4 KiB | http-error | - | - |
| forums | https://askubuntu.com/ | HTTP/2 | 403 | - | 67 ms | 67 ms | 5.4 KiB | http-error | - | - |
| forums | https://www.linuxquestions.org/questions/ | HTTP/2 | 403 | - | 82 ms | 82 ms | 5.7 KiB | http-error | - | - |
| forums | https://bbs.archlinux.org/ | HTTP/1.1 | 200 | - | 835 ms | 844 ms | 22.9 KiB | ok | 544 | 0 ms |
| forums | https://www.raspberrypi.org/forums/ | HTTP/2 | 302 | - | 33 ms | 33 ms | 504 B | http-error | - | - |
| webapp | https://github.com/ | HTTP/2 | 200 | - | 109 ms | 280 ms | 563.6 KiB | ok | 1807 | 4 ms |
| webapp | https://gitlab.com/ | HTTP/2 | 403 | - | 30 ms | 30 ms | 2.1 KiB | http-error | - | - |
| webapp | https://www.notion.so/ | HTTP/2 | 307 | - | 94 ms | 94 ms | 0 B | http-error | - | - |
| webapp | https://slack.com/ | HTTP/2 | 200 | - | 427 ms | 642 ms | 237.5 KiB | ok | 1992 | 3 ms |
| webapp | https://trello.com/ | HTTP/2 | 200 | - | 66 ms | 86 ms | 521.4 KiB | ok | 1209 | 2 ms |
| webapp | https://asana.com/ | HTTP/2 | 200 | - | 167 ms | 393 ms | 898.0 KiB | ok | 2471 | 4 ms |
| webapp | https://www.atlassian.com/software/jira | HTTP/2 | 405 | - | 22 ms | 22 ms | 2.2 KiB | http-error | - | - |
| webapp | https://linear.app/ | HTTP/2 | 200 | - | 373 ms | 404 ms | 1.2 MiB | ok | 4968 | 8 ms |
| webapp | https://www.figma.com/ | HTTP/2 | 200 | - | 721 ms | 1797 ms | 1.6 MiB | ok | 1376 | 4 ms |
| webapp | https://mail.proton.me/ | HTTP/2 | 200 | - | 666 ms | 666 ms | 4.7 KiB | ok | 62 | 0 ms |
