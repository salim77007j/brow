/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Anti-fingerprinting defenses.
//!
//! brow ships fingerprint defenses as *generated userscripts*: a JS payload
//! is produced per (protection level, top-level origin, per-session key) and
//! injected at document load. The technique is prototype wrapping — the same
//! approach used by hardened-extension stacks (JShelter & co.) — chosen so
//! the defenses live in the privacy crate instead of deep in the DOM engine.
//!
//! Deterministic per-session, per-origin seeds make the noise stable across
//! reloads inside one session (breaking cross-session tracking while not
//! breaking sites that read canvas twice), and rotate on restart.
//!
//! Defenses by level:
//! * **Standard**: Canvas 2D (`getImageData`, `toDataURL`, `toBlob`) noise,
//!   WebGL vendor/renderer spoof + `readPixels` noise, AudioContext noise,
//!   `navigator.hardwareConcurrency`/`deviceMemory`/`platform` reduction.
//! * **Strict**: everything above plus font-measurement jitter,
//!   `document.fonts.check` restriction, ClientRect jitter, timezone
//!   reporting freeze.

use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum DefenseLevel {
    Off,
    #[default]
    Standard,
    Strict,
}

/// Per-origin override (user chooses to trust or harden a site).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SiteDefenseOverrides {
    /// origin string (`https://example.com`) -> level
    pub overrides: std::collections::BTreeMap<String, DefenseLevel>,
}

impl SiteDefenseOverrides {
    pub fn level_for(&self, origin: &str, default: DefenseLevel) -> DefenseLevel {
        self.overrides.get(origin).copied().unwrap_or(default)
    }
}

/// 128-bit per-session key; rotate on every browser start.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionKey(pub [u8; 16]);

impl SessionKey {
    pub fn from_entropy() -> Self {
        // No external rng dependency: seed from the OS clock + address-space
        // randomness + a counter — sufficient for fingerprint noise seeds.
        let t = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let p = &t as *const u128 as usize;
        let mut key = [0u8; 16];
        let mut state = (t as u64) ^ (p as u64) ^ 0x9E37_79B9_7F4A_7C15;
        for b in key.iter_mut() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17; // splitmix64
            *b = (state >> 24) as u8;
        }
        SessionKey(key)
    }
}

/// A generated defense payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DefensePayload {
    /// Fully self-contained IIFE; inject as a userscript at document start.
    pub script: String,
    /// Names of the defenses actually enabled in this payload.
    pub active: Vec<String>,
    /// Level that produced this payload.
    pub level: DefenseLevel,
}

/// FNV-1a 32-bit of a string — the runtime JS mirror in the PRNG header
/// derives per-origin seeds from `location.origin` with this algorithm.
#[allow(dead_code)]
fn fnv1a(s: &str) -> u32 {
    let mut h: u32 = 0x811C_9DC5;
    for b in s.as_bytes() {
        h ^= *b as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Session key words baked into the payload. Per-origin variation happens
/// at RUNTIME inside the script (FNV-1a of `location.origin` XOR these
/// words) so a single userscript file serves every origin with distinct,
/// session-stable seeds.
fn session_words(session: &SessionKey) -> [u32; 4] {
    let mut s = [0u32; 4];
    for (i, slot) in s.iter_mut().enumerate() {
        *slot = u32::from_le_bytes([
            session.0[i * 4],
            session.0[i * 4 + 1],
            session.0[i * 4 + 2],
            session.0[i * 4 + 3],
        ]);
    }
    if s.iter().all(|&v| v == 0) {
        s[0] = 0x1234_5678;
    }
    s
}

pub fn origin_of(url: &Url) -> String {
    url.origin().ascii_serialization()
}

/// Build the defense payload for a session. Per-origin seeds are derived
/// inside the script at document load (`location.origin` XOR the session
/// words), so one payload serves every site with origin-dependent noise.
pub fn build_payload(
    level: DefenseLevel,
    session: &SessionKey,
    config: &FingerprintConfig,
) -> DefensePayload {
    if level == DefenseLevel::Off {
        return DefensePayload { script: String::new(), active: Vec::new(), level };
    }
    let words = session_words(session);
    let mut active = Vec::new();

    let mut js = String::with_capacity(16 * 1024);
    js.push_str(HEADER);
    js.push_str(&format!(
        "var BROW_KEY=[{},{},{},{}],BROW_HC={},BROW_DM={},BROW_PLATFORM={};\n",
        words[0],
        words[1],
        words[2],
        words[3],
        config.hardware_concurrency,
        config.device_memory,
        js_str(&config.spoofed_platform)
    ));
    js.push_str(PRNG);

    if level >= DefenseLevel::Standard {
        js.push_str(CANVAS_2D);
        active.push("canvas2d-noise".to_string());
        js.push_str(WEBGL);
        active.push("webgl-spoof+noise".to_string());
        js.push_str(AUDIO);
        active.push("audiocontext-noise".to_string());
        js.push_str(NAVIGATOR);
        active.push("navigator-reduction".to_string());
    }
    if level >= DefenseLevel::Strict {
        js.push_str(FONTS);
        active.push("font-measure-jitter".to_string());
        js.push_str(RECTS);
        active.push("client-rects-jitter".to_string());
        js.push_str(TIMEZONE);
        active.push("timezone-freeze".to_string());
    }

    js.push_str(FOOTER);
    DefensePayload { script: js, active, level }
}

/// Tunable scalar spoofs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FingerprintConfig {
    pub hardware_concurrency: u32,
    pub device_memory: u32,
    pub spoofed_platform: String,
}

impl Default for FingerprintConfig {
    fn default() -> Self {
        // The most common hardware profile: blending into the crowd is the
        // whole point of the reduction.
        FingerprintConfig {
            hardware_concurrency: 4,
            device_memory: 8,
            spoofed_platform: "Win32".to_string(),
        }
    }
}

fn js_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '<' => out.push_str("\\x3c"),
            '>' => out.push_str("\\x3e"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// JS payload parts (assembled; never user-controllable text is interpolated
// except through js_str above).
// ---------------------------------------------------------------------------

const HEADER: &str = r#"(function() {
"use strict";
if (window.__brow_fp) return;
Object.defineProperty(window, "__brow_fp", { value: true, enumerable: false, configurable: false, writable: false });
"#;

const PRNG: &str = r#"/* xorshift128+ seeded per-origin per-session.
   Session words are baked in; the origin hash is computed at document load
   from location.origin, so every site gets distinct but session-stable
   noise. */
var BROW_ORIG = String(location.origin || "null");
var BROW_H = 0x811C9DC5;
for (var _i = 0; _i < BROW_ORIG.length; _i++) {
  BROW_H = ((BROW_H ^ BROW_ORIG.charCodeAt(_i)) * 0x01000193) >>> 0;
}
var S0 = ((BROW_H ^ BROW_KEY[0]) >>> 0) || 0x9E3779B9;
var S1 = BROW_KEY[1] >>> 0;
var S2 = ((BROW_H ^ BROW_KEY[2]) >>> 0) || 0x85EBCA6B;
var S3 = BROW_KEY[3] >>> 0;
function browNext() {
  var a = S0, b = S1, c = S2, d = S3;
  var t = (a + (b << 0)) >>> 0;
  a = b; S0 = a; b = c; S1 = b;
  c = (d ^ (c << 23)) >>> 0;
  S2 = c;
  d = ((d ^ a) ^ (c ^ (a >>> 18)) ^ ((c >>> 5) | 0) ^ ((d << 18) | 0)) >>> 0;
  S3 = d;
  return t >>> 0;
}
function browNoise(mag) { return ((browNext() % 3) - 1) * mag; }
function browBound(v) { return Math.max(0, Math.min(255, v | 0)); }
"#;

const CANVAS_2D: &str = r#"(function() {
  if (!window.HTMLCanvasElement || !window.CanvasRenderingContext2D) return;
  var origGetImageData = CanvasRenderingContext2D.prototype.getImageData;
  CanvasRenderingContext2D.prototype.getImageData = function() {
    var data = origGetImageData.apply(this, arguments);
    try {
      var px = data.data;
      for (var i = 3; i < px.length; i += 4 * 7) {
        px[i] = browBound(px[i] + browNoise(1));
      }
      Object.defineProperty(data, "data", { value: px });
    } catch (e) {}
    return data;
  };
  var origToDataURL = HTMLCanvasElement.prototype.toDataURL;
  HTMLCanvasElement.prototype.toDataURL = function() {
    try {
      var ctx = this.getContext && this.getContext("2d");
      if (ctx && ctx.getImageData) {
        var w = Math.min(this.width | 0, 512), h = Math.min(this.height | 0, 512);
        if (w > 0 && h > 0) {
          var d = origGetImageData.call(ctx, 0, 0, w, h);
          var px = d.data;
          for (var i = 3; i < px.length; i += 4 * 11) px[i] = browBound(px[i] + browNoise(1));
          ctx.putImageData(d, 0, 0);
        }
      }
    } catch (e) {}
    return origToDataURL.apply(this, arguments);
  };
  var origToBlob = HTMLCanvasElement.prototype.toBlob;
  HTMLCanvasElement.prototype.toBlob = function(cb, type, q) {
    try {
      var ctx = this.getContext && this.getContext("2d");
      if (ctx && ctx.getImageData) {
        var w = Math.min(this.width | 0, 512), h = Math.min(this.height | 0, 512);
        if (w > 0 && h > 0) {
          var d = origGetImageData.call(ctx, 0, 0, w, h);
          var px = d.data;
          for (var i = 3; i < px.length; i += 4 * 13) px[i] = browBound(px[i] + browNoise(1));
          ctx.putImageData(d, 0, 0);
        }
      }
    } catch (e) {}
    return origToBlob.call(this, cb, type, q);
  };
})();
"#;

const WEBGL: &str = r#"(function() {
  if (!window.WebGLRenderingContext) return;
  var VENDOR_UNMASKED = 37445, RENDERER_UNMASKED = 37446;
  var fakeVendor = "Brow Protected", fakeRenderer = "Brow WebGL (fingerprint protected)";
  var origGetParameter = WebGLRenderingContext.prototype.getParameter;
  var origGetParameter2 = window.WebGL2RenderingContext ? WebGL2RenderingContext.prototype.getParameter : null;
  function patched(orig) {
    return function(p) {
      if (p === VENDOR_UNMASKED) return fakeVendor;
      if (p === RENDERER_UNMASKED) return fakeRenderer;
      return orig.call(this, p);
    };
  }
  WebGLRenderingContext.prototype.getParameter = patched(origGetParameter);
  if (origGetParameter2) WebGL2RenderingContext.prototype.getParameter = patched(origGetParameter2);
  var origReadPixels = WebGLRenderingContext.prototype.readPixels;
  var origReadPixels2 = window.WebGL2RenderingContext ? WebGL2RenderingContext.prototype.readPixels : null;
  function patchRead(orig) {
    return function() {
      orig.apply(this, arguments);
      try {
        var px = arguments[6];
        if (px && px.length) { for (var i = 0; i < px.length; i += 97) px[i] = browBound(px[i] + browNoise(1)); }
      } catch (e) {}
    };
  }
  WebGLRenderingContext.prototype.readPixels = patchRead(origReadPixels);
  if (origReadPixels2) WebGL2RenderingContext.prototype.readPixels = patchRead(origReadPixels2);
})();
"#;

const AUDIO: &str = r#"(function() {
  if (!window.AudioBuffer) return;
  var orig = AudioBuffer.prototype.getChannelData;
  AudioBuffer.prototype.getChannelData = function(ch) {
    var data = orig.call(this, ch);
    try {
      for (var i = 0; i < data.length; i += 501) {
        data[i] = data[i] + browNoise(1) * 1e-7;
      }
    } catch (e) {}
    return data;
  };
})();
"#;

const NAVIGATOR: &str = r#"(function() {
  try {
    Object.defineProperty(navigator, "hardwareConcurrency", { get: function() { return BROW_HC; }, configurable: true });
    if ("deviceMemory" in navigator) {
      Object.defineProperty(navigator, "deviceMemory", { get: function() { return BROW_DM; }, configurable: true });
    }
    Object.defineProperty(navigator, "platform", { get: function() { return BROW_PLATFORM; }, configurable: true });
  } catch (e) {}
})();
"#;

const FONTS: &str = r#"(function() {
  if (!window.CanvasRenderingContext2D) return;
  var origMeasure = CanvasRenderingContext2D.prototype.measureText;
  CanvasRenderingContext2D.prototype.measureText = function() {
    var m = origMeasure.apply(this, arguments);
    try {
      var j = 1 + (browNext() % 1000 - 500) / 1e6;
      if (m.width !== undefined) Object.defineProperty(m, "width", { value: m.width * j });
    } catch (e) {}
    return m;
  };
  if (window.document && document.fonts && document.fonts.check) {
    var origCheck = document.fonts.check.bind(document.fonts);
    document.fonts.check = function(spec, text) {
      var allowed = ["monospace", "sans-serif", "serif", "system-ui"];
      var family = (spec || "").toLowerCase();
      for (var i = 0; i < allowed.length; i++) if (family.indexOf(allowed[i]) !== -1) return origCheck(spec, text);
      return false;
    };
  }
})();
"#;

const RECTS: &str = r#"(function() {
  if (!window.Element || !Element.prototype.getBoundingClientRect) return;
  var origRect = Element.prototype.getBoundingClientRect;
  var origRects = Element.prototype.getClientRects;
  function jitterRect(r) {
    var j = browNoise(1) / 1000;
    return { width: r.width + j, height: r.height + j, x: r.x + j, y: r.y + j,
             left: r.left + j, right: r.right + j, top: r.top + j, bottom: r.bottom + j,
             toJSON: r.toJSON ? r.toJSON.bind(r) : undefined };
  }
  Element.prototype.getBoundingClientRect = function() {
    var r = origRect.call(this);
    try { return jitterRect(r); } catch (e) { return r; }
  };
  if (origRects) {
    Element.prototype.getClientRects = function() {
      var list = origRects.call(this);
      try {
        var out = [];
        for (var i = 0; i < list.length; i++) out.push(jitterRect(list[i]));
        out.item = function(n) { return out[n]; };
        return out;
      } catch (e) { return list; }
    };
  }
})();
"#;

const TIMEZONE: &str = r#"(function() {
  try {
    var fixed = "UTC";
    var orig = Intl.DateTimeFormat.prototype.resolvedOption;
    Intl.DateTimeFormat.prototype.resolvedOption = function() {
      var o = orig.call(this);
      o.timeZone = fixed;
      return o;
    };
  } catch (e) {}
})();
"#;

const FOOTER: &str = "})();\n";

#[cfg(test)]
mod tests {
    use super::*;

    fn session() -> SessionKey {
        SessionKey([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16])
    }

    #[test]
    fn off_level_is_empty() {
        let p = build_payload(DefenseLevel::Off, &session(), &FingerprintConfig::default());
        assert!(p.script.is_empty());
        assert!(p.active.is_empty());
    }

    #[test]
    fn standard_contains_core_defenses() {
        let p = build_payload(DefenseLevel::Standard, &session(), &FingerprintConfig::default());
        assert_eq!(
            p.active,
            vec![
                "canvas2d-noise".to_string(),
                "webgl-spoof+noise".to_string(),
                "audiocontext-noise".to_string(),
                "navigator-reduction".to_string()
            ]
        );
        assert!(p.script.contains("getImageData"));
        assert!(p.script.contains("37445"));
        assert!(p.script.contains("getChannelData"));
        assert!(p.script.contains("hardwareConcurrency"));
        assert!(!p.script.contains("measureText"), "strict-only defense must be absent");
    }

    #[test]
    fn strict_adds_font_rect_timezone() {
        let p = build_payload(DefenseLevel::Strict, &session(), &FingerprintConfig::default());
        assert!(p.active.contains(&"font-measure-jitter".to_string()));
        assert!(p.active.contains(&"client-rects-jitter".to_string()));
        assert!(p.active.contains(&"timezone-freeze".to_string()));
        assert!(p.script.contains("measureText"));
        assert!(p.script.contains("getBoundingClientRect"));
        assert!(p.script.contains("timeZone"));
    }

    #[test]
    fn seeds_vary_per_session_and_derive_origin_at_runtime() {
        let s1 = session();
        let mut s2 = session();
        s2.0[0] ^= 0xFF;
        let a = build_payload(DefenseLevel::Standard, &s1, &FingerprintConfig::default());
        let c = build_payload(DefenseLevel::Standard, &s2, &FingerprintConfig::default());
        assert_ne!(a.script, c.script, "different sessions must bake different key words");
        // deterministic within the same session — one file serves all origins
        let a2 = build_payload(DefenseLevel::Standard, &s1, &FingerprintConfig::default());
        assert_eq!(a.script, a2.script);
        // runtime per-origin derivation present
        assert!(a.script.contains("location.origin"));
    }

    #[test]
    fn braces_balanced() {
        let p = build_payload(DefenseLevel::Strict, &session(), &FingerprintConfig::default());
        let mut depth = 0i32;
        let mut parens = 0i32;
        for c in p.script.chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                '(' => parens += 1,
                ')' => parens -= 1,
                _ => {}
            }
            assert!(depth >= 0 && parens >= 0, "unbalanced at some point");
        }
        assert_eq!(depth, 0);
        assert_eq!(parens, 0);
        assert!(p.script.starts_with("(function() {"));
        assert!(p.script.ends_with("})();\n"));
    }

    #[test]
    fn key_interpolation_is_present() {
        let p = build_payload(DefenseLevel::Standard, &session(), &FingerprintConfig::default());
        assert!(p.script.contains("BROW_KEY=["));
        assert!(p.script.contains("hardwareConcurrency"));
    }

    #[test]
    fn overrides_select_level() {
        let mut o = SiteDefenseOverrides::default();
        assert_eq!(o.level_for("https://x.example", DefenseLevel::Standard), DefenseLevel::Standard);
        o.overrides.insert("https://x.example".to_string(), DefenseLevel::Off);
        assert_eq!(o.level_for("https://x.example", DefenseLevel::Standard), DefenseLevel::Off);
    }

    #[test]
    fn session_key_entropy_is_nonzero() {
        let a = SessionKey::from_entropy();
        let b = SessionKey::from_entropy();
        assert_ne!(a.0, [0u8; 16]);
        assert_ne!(a.0, b.0);
    }
}
