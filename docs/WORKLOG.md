# brow — Work Log (v0.7 Rebuild)

This is the running work log for the v0.7.0 rebuild program (Option 2: keep the
independent Servo/Rust engine and fix it). Every subtask appends an entry here.
Rule: **no work is "done" until it is committed and pushed** — this file is the
audit trail that survives context resets.

Format per entry:

```
## [Phase.Subtask] YYYY-MM-DD — <title>
- What was done
- Evidence (SHAs, measurements, file paths)
- Next step
```

---

## [0.1] 2026-10-09 — Environment & work protection (fresh clone + HEAD verification)

Context: v0.6.0 / v0.6.1 were released but **failed real-device verification**
(Windows + AMD iGPU): extremely slow, pervasive UI visual bugs, two-window
architecture anti-pattern. Strategy decision: **Option 2 — rebuild the shell on
Servo's proven single-window mode, keep the independent engine and the working
privacy engine** (`servo/components/net/privacy.rs`). Target: v0.7.0.

What was done:

1. Restored git credentials at `/home/z/.git-credentials` (chmod 600, outside
   the repository; token never committed to any repo file).
2. Fresh clone of `https://github.com/salim77007j/brow` into
   `/home/z/my-project/brow` (195,389 files checked out).
3. HEAD verification against GitHub:
   - Remote baseline captured first: `git ls-remote` →
     `main = 7cc97381a378f117d3e713ad661df408861d1637`
   - Local clone: `git rev-parse HEAD` →
     `7cc97381a378f117d3e713ad661df408861d1637`
   - **Match: VERIFIED** (local HEAD == GitHub main).
4. Working tree confirmed clean (`git status --short` empty).
5. Repository layout confirmed:
   - Servo workspace root: `servo/Cargo.toml`
   - servoshell (single-window reference): `servo/ports/servoshell/`
   - Privacy engine (must be preserved): `servo/components/net/privacy.rs`
   - Existing CI workflows: `.github/workflows/{ci,pgo-build,phase6-comparison,release}.yml`
6. Created this file (`docs/WORKLOG.md`) and committed/pushed it.

Environment-protection rules now in force (re-stated for every future session):

- Credentials live only in `/home/z/.git-credentials` (outside any repo, 0600).
- No uncommitted work may be left behind: every subtask ends with commit+push.
- If the environment is reset, re-clone and continue from this log; never
  re-derive history from memory.
- All work happens on the `v0.7-rebuild` branch (created in 0.2); `main` is
  only updated by squash-merge after user-verified milestones.

Next step: [0.2] create and protect the `v0.7-rebuild` branch.
