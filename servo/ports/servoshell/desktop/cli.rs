/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

use std::{env, panic};

use crate::desktop::app::App;
use crate::desktop::event_loop::ServoShellEventLoop;
use crate::panic_hook;
use crate::prefs::{ArgumentParsingResult, parse_command_line_arguments};

pub fn main() {
    run_from_args(env::args().skip(1).collect())
}

/// The full shell startup, parameterized over the argument vector so a
/// product embedder (brow, via `servoshell::shell::run_from_args`) can run
/// the same shell with its own args. Servo behavior is unchanged when no
/// `shell::set_overrides` was applied.
pub(crate) fn run_from_args(args: Vec<String>) {
    crate::crash_handler::install();
    crate::init_crypto();
    // brow (phase 6.2): chrome process residency tune. Content processes
    // are tuned in their branch below (they hold the DOM/JS/images).
    servo_allocator::tune_residency();

    // TODO: once log-panics is released, can this be replaced by
    // log_panics::init()?
    panic::set_hook(Box::new(panic_hook::panic_hook));

    let (mut opts, mut preferences, mut servoshell_preferences) =
        match parse_command_line_arguments(&*args) {
            ArgumentParsingResult::ContentProcess(token) => {
                // brow (phase 6.2): content processes hold the DOM, JS heaps
                // and image buffers — the allocator residency tune matters
                // most here (jemalloc background purge of decayed pages).
                servo_allocator::tune_residency();
                return servo::run_content_process(token);
            },
            ArgumentParsingResult::ChromeProcess(opts, preferences, servoshell_preferences) => {
                (opts, preferences, servoshell_preferences)
            },
            ArgumentParsingResult::Exit => {
                std::process::exit(0);
            },
            ArgumentParsingResult::ErrorParsing => {
                std::process::exit(1);
            },
        };

    // brow (D-010): apply the embedder's overrides — initial URL, homepage,
    // search page, engine preferences replacement, and config dir — after
    // parsing and before anything consumes them. No-op for servo itself.
    if let Some(overrides) = crate::shell::overrides() {
        if let Some(url) = &overrides.initial_url {
            servoshell_preferences.url = Some(url.clone());
        }
        if let Some(homepage) = &overrides.homepage {
            servoshell_preferences.homepage = homepage.clone();
        }
        if let Some(searchpage) = &overrides.searchpage {
            servoshell_preferences.searchpage = searchpage.clone();
        }
        if let Some(preferences_override) = &overrides.preferences {
            preferences = preferences_override.clone();
        }
        if let Some(config_dir) = &overrides.config_dir {
            opts.config_dir = Some(config_dir.clone());
        }
    }

    crate::init_tracing(servoshell_preferences.tracing_filter.as_deref());

    let clean_shutdown = servoshell_preferences.clean_shutdown;
    let event_loop = match servoshell_preferences.headless {
        true => ServoShellEventLoop::headless(),
        false => ServoShellEventLoop::headed(),
    };

    {
        let mut app = App::new(opts, preferences, servoshell_preferences, &event_loop);
        event_loop.run_app(&mut app);
    }

    crate::platform::deinit(clean_shutdown)
}
