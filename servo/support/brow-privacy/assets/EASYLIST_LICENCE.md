# brow-privacy — EasyList snapshot attribution

This directory contains a verbatim snapshot of the **EasyList** filter list,
vendored for offline unit tests and benchmarks of `brow-privacy`.

- Source:      https://easylist.to/easylist/easylist.txt
- Title:       EasyList
- Version:     202610060000
- Last modified: 06 Oct 2026 00:00 UTC
- Commit:      cd705aa5647944350ac82fcebba273367aac5901
- Lines:       79,855 (raw, including comments / metadata / cosmetic rules)
- Maintainer:  The EasyList authors (https://easylist.to/)
- Licence:     Creative Commons Attribution-ShareAlike 3.0 Unported
               https://easylist.to/pages/licence.html
               https://creativecommons.org/licenses/by-sa/3.0/

This snapshot is redistributed **for attribution-required testing purposes
only**, unmodified, with this notice. EasyList content remains (c) the
EasyList authors under CC BY-SA 3.0. The snapshot is embedded via
`include_str!` and never shipped to end-user pages at runtime; production
builds fetch fresh lists at runtime from user-configured sources.
