/* brow-shell application (engine feature): single winit event loop driving
 *   - the Slint chrome (software-rendered strip + panels),
 *   - one Servo WebView per tab (own winit window + surfman context),
 *   - the brow-shell-core tab lifecycle (sleep/discard/restore),
 *   - the memory governor (RSS sampling → sleep/discard passes).
 *
 * Threading model: everything (winit, Slint, servo delegates, core state)
 * lives on the main thread. Engine threads wake us via the BrowWaker. */

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{Duration, Instant};

use slint::platform::{PointerEventButton, WindowAdapter, WindowEvent as SlintWindowEvent};
use slint::{LogicalPosition, SharedString};
use servo::{
    InputEvent, MouseButton, MouseButtonAction, MouseButtonEvent, MouseLeftViewportEvent,
    MouseMoveEvent, RenderingContext, Scroll, WebViewBuilder, WebViewPoint, WebViewVector,
    WheelDelta, WheelEvent, WheelMode,
};
use url::Url;
use webrender_api::units::DevicePoint;
use winit::event::WindowEvent as WinitWindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoopProxy};
use winit::window::{Window, WindowId};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};

use brow_shell_core::{TabEvent, TabId, normalize_url};

use crate::delegate::BrowWebViewDelegate;
use crate::platform::ChromeSurface;
use crate::state::{
    Action, BrowEvent, BrowState, ContentWindow, SharedState, SLEEP_PASS_INTERVAL,
    RESOURCE_SAMPLE_INTERVAL,
};
use crate::waker::BrowWaker;

pub const CHROME_HEIGHT_PX: u32 = 88;
const DEFAULT_WIDTH: f64 = 1240.0;
const DEFAULT_CONTENT_HEIGHT: f64 = 712.0;

/// The winit application handler.
pub struct BrowApp {
    pub state: SharedState,
    pub delegate: Rc<BrowWebViewDelegate>,
}

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = winit::event_loop::EventLoop::<BrowEvent>::with_user_event().build()?;
    let proxy: EventLoopProxy<BrowEvent> = event_loop.create_proxy();
    let state: SharedState = Rc::new(RefCell::new(BrowState::new(proxy)));
    let delegate = Rc::new(BrowWebViewDelegate(state.clone()));

    // Slint platform must be registered before the chrome component exists.
    slint::platform::set_platform(Box::new(crate::platform::BrowPlatform))
        .map_err(|e| format!("slint::set_platform failed: {e:?}"))?;

    let mut app = BrowApp { state, delegate };
    event_loop.run_app(&mut app)?;
    Ok(())
}

impl winit::application::ApplicationHandler<BrowEvent> for BrowApp {
    fn resumed(&mut self, active: &ActiveEventLoop) {
        {
            let mut state = self.state.borrow_mut();
            if state.servo.is_some() {
                return;
            }

            // 1. Chrome surface (winit window + software renderer + softbuffer).
            let chrome_window = active
                .create_window(
                    Window::default_attributes()
                        .with_title("brow")
                        .with_inner_size(winit::dpi::PhysicalSize::new(
                            DEFAULT_WIDTH as u32,
                            CHROME_HEIGHT_PX,
                        )),
                )
                .expect("chrome window");
            state.chrome_window_id = Some(chrome_window.id());

            let display_handle = active.display_handle().expect("display handle");
            // Promote to 'static: RawDisplayHandle is plain data (an id/pointer);
            // the display connection itself is owned by the event loop, which
            // outlives every surface created here.
            let display_handle = unsafe {
                raw_window_handle::DisplayHandle::borrow_raw(display_handle.as_raw())
            };
            let surface = Rc::new(ChromeSurface::new_attached(
                chrome_window,
                display_handle,
            ));
            state.chrome_surface = Some(surface.clone());

            // 2. Queue the adapter, then build the chrome component.
            crate::platform::queue_global(surface.slint_window.clone());
            let chrome = crate::generated::BrowserChrome::new().expect("chrome component");
            chrome.set_address(SharedString::from(state.settings.home_page.clone()));

            // 3. Chrome callbacks -> pending action queue.
            let st = self.state.clone();
            chrome.on_tab_activated(move |id| {
                st.borrow_mut()
                    .pending
                    .push(Action::ActivateTab(TabId(id.max(0) as u64)));
            });
            let st = self.state.clone();
            chrome.on_tab_closed(move |id| {
                st.borrow_mut()
                    .pending
                    .push(Action::CloseTab(TabId(id.max(0) as u64)));
            });
            let st = self.state.clone();
            chrome.on_new_tab(move || {
                st.borrow_mut().pending.push(Action::NewTab(home_url(&st)));
            });
            let st = self.state.clone();
            chrome.on_go_back(move || st.borrow_mut().pending.push(Action::Back));
            let st = self.state.clone();
            chrome.on_go_forward(move || st.borrow_mut().pending.push(Action::Forward));
            let st = self.state.clone();
            chrome.on_go_reload(move || st.borrow_mut().pending.push(Action::ReloadActive));
            let st = self.state.clone();
            chrome.on_go_home(move || st.borrow_mut().pending.push(Action::NewTab(home_url(&st))));
            let st = self.state.clone();
            chrome.on_address_submitted(move |text| {
                st.borrow_mut()
                    .pending
                    .push(Action::AddressSubmitted(text.to_string()));
            });
            let st = self.state.clone();
            chrome.on_bookmark_toggled(move || {
                st.borrow_mut().pending.push(Action::ToggleBookmark);
            });
            let st = self.state.clone();
            chrome.on_panel_toggled(move |kind| {
                st.borrow_mut().pending.push(Action::TogglePanel(kind));
            });
            let st = self.state.clone();
            chrome.on_panel_item_opened(move |target| {
                st.borrow_mut()
                    .pending
                    .push(Action::AddressSubmitted(target.to_string()));
            });
            let st = self.state.clone();
            chrome.on_panel_item_removed(move |id| {
                st.borrow_mut()
                    .pending
                    .push(Action::RemoveBookmark(id.max(0) as u64));
            });
            let st = self.state.clone();
            chrome.on_history_cleared(move || {
                st.borrow_mut().pending.push(Action::ClearHistory);
            });
            let st = self.state.clone();
            chrome.on_setting_toggled(move |key| {
                st.borrow_mut()
                    .pending
                    .push(Action::ToggleSetting(key.to_string()));
            });
            let st = self.state.clone();
            chrome.on_setting_cycled(move |key| {
                st.borrow_mut()
                    .pending
                    .push(Action::CycleSetting(key.to_string()));
            });

            state.chrome = Some(chrome);

            // 4. Engine.
            let waker = BrowWaker(state.proxy.clone());
            let servo = servo::ServoBuilder::default()
                .event_loop_waker(Box::new(waker))
                .build();
            servo.setup_logging();
            state.servo = Some(servo);

            // 5. Session restore / first tab.
            if state.settings.restore_session {
                let session_path = state.data_dir.join("session.json");
                if let Ok(raw) = std::fs::read_to_string(&session_path) {
                    if let Ok(urls) = serde_json::from_str::<Vec<String>>(&raw) {
                        for (i, u) in urls.iter().enumerate() {
                            if let Ok(url) = Url::parse(u) {
                                state.open_tab(url, i == 0);
                            }
                        }
                    }
                }
            }
            if state.tabs.tabs().is_empty() {
                let home = Url::parse(&state.settings.home_page)
                    .unwrap_or_else(|_| Url::parse("about:blank").unwrap());
                state.open_tab(home, true);
            }

            state.sync_chrome();
        }

        // Create the content window for the activated tab + first render.
        pump(self.state.clone(), active, &self.delegate);
        render_chrome(&self.state);
    }

    fn user_event(&mut self, active: &ActiveEventLoop, event: BrowEvent) {
        if let BrowEvent::ChromeSync = event {
            self.state.borrow().sync_chrome();
        }
        pump(self.state.clone(), active, &self.delegate);
        render_chrome(&self.state);
    }

    fn window_event(
        &mut self,
        active: &ActiveEventLoop,
        window_id: WindowId,
        event: WinitWindowEvent,
    ) {
        let (is_chrome, quitting) = {
            let state = self.state.borrow();
            (state.chrome_window_id == Some(window_id), state.quitting)
        };
        if quitting {
            return;
        }

        if is_chrome {
            // Chrome window close = app close (persist session first).
            if let WinitWindowEvent::CloseRequested = &event {
                {
                    let mut state = self.state.borrow_mut();
                    state.persist_session();
                    state.quitting = true;
                }
                active.exit();
                return;
            }
            route_chrome_event(&self.state, &event);
        } else {
            route_content_event(&self.state, window_id, event, active, &self.delegate);
        }

        pump(self.state.clone(), active, &self.delegate);
        render_chrome(&self.state);
    }

    fn about_to_wait(&mut self, active: &ActiveEventLoop) {
        pump(self.state.clone(), active, &self.delegate);
        render_chrome(&self.state);

        // Idle policy: sleep the loop until the next Slint timer, resource
        // sample, or sleep pass — the shell itself contributes near-zero idle
        // CPU (no polling spin).
        let next_timer = crate::platform::duration_until_next_slint_update();
        let (next_sample, next_pass) = {
            let state = self.state.borrow();
            (
                RESOURCE_SAMPLE_INTERVAL
                    .checked_sub(state.last_resource_sample.elapsed())
                    .unwrap_or(Duration::ZERO),
                SLEEP_PASS_INTERVAL
                    .checked_sub(state.last_sleep_pass.elapsed())
                    .unwrap_or(Duration::ZERO),
            )
        };
        let wait = next_timer
            .unwrap_or(Duration::from_secs(1))
            .min(next_sample)
            .min(next_pass);
        active.set_control_flow(winit::event_loop::ControlFlow::WaitUntil(
            std::time::Instant::now() + wait,
        ));
    }
}

fn home_url(state: &SharedState) -> Url {
    let state = state.borrow();
    Url::parse(&state.settings.home_page)
        .unwrap_or_else(|_| Url::parse("about:blank").unwrap())
}

/// Periodic work + pending-action execution + engine pump.
fn pump(
    state: SharedState,
    active: &ActiveEventLoop,
    delegate: &Rc<BrowWebViewDelegate>,
) {
    crate::platform::pump_slint_timers();

    // 1. Drain pending chrome actions.
    let actions: Vec<Action> = std::mem::take(&mut state.borrow_mut().pending);
    if !actions.is_empty() {
        let mut state = state.borrow_mut();
        for action in actions {
            execute_action(&mut state, action, active, delegate);
        }
    }

    // 2. Engine pump (delegate callbacks fire here).
    {
        let state = state.borrow();
        if let Some(servo) = &state.servo {
            servo.spin_event_loop();
        }
    }

    // 3. Resource governor tick.
    let now = Instant::now();
    let do_sample = {
        let mut state = state.borrow_mut();
        let due = now.duration_since(state.last_resource_sample) >= RESOURCE_SAMPLE_INTERVAL;
        if due {
            state.last_resource_sample = now;
        }
        due
    };
    if do_sample {
        let events = {
            let mut state = state.borrow_mut();
            let rss = brow_shell_core::memwatch::current_process_rss_bytes().unwrap_or(0);
            let _pressure = state.memwatch.sample(Some(rss));
            state.tabs.handle_memory_sample(rss, now)
        };
        state
            .borrow_mut()
            .apply_tab_events(events, active, delegate);
    }

    let do_sleep_pass = {
        let mut state = state.borrow_mut();
        let due = now.duration_since(state.last_sleep_pass) >= SLEEP_PASS_INTERVAL
            && state.settings.auto_sleep_tabs;
        if due {
            state.last_sleep_pass = now;
        }
        due
    };
    if do_sleep_pass {
        let events = {
            let mut state = state.borrow_mut();
            state.tabs.run_sleep_pass(now)
        };
        state
            .borrow_mut()
            .apply_tab_events(events, active, delegate);
    }
}

fn execute_action(
    state: &mut BrowState,
    action: Action,
    active: &ActiveEventLoop,
    delegate: &Rc<BrowWebViewDelegate>,
) {
    match action {
        Action::ActivateTab(id) => {
            let events = state.tabs.activate_tab(id, Instant::now());
            state.apply_tab_events(events, active, delegate);
        }
        Action::CloseTab(id) => {
            if let Ok(events) = state.tabs.close_tab(id, Instant::now()) {
                state.apply_tab_events(events, active, delegate);
            }
        }
        Action::NewTab(url) => {
            state.open_tab(url, true);
            if let Some(id) = state.tabs.active_id() {
                let events = vec![TabEvent::Activated(id)];
                state.apply_tab_events(events, active, delegate);
            }
        }
        Action::Back => {
            if let Some(id) = state.tabs.active_id() {
                let target = state.tabs.go_back(id).ok().flatten();
                if let Some(url) = target {
                    if let Some(content) = state.content.get(&id) {
                        content.webview.load(url);
                    }
                }
            }
        }
        Action::Forward => {
            if let Some(id) = state.tabs.active_id() {
                let target = state.tabs.go_forward(id).ok().flatten();
                if let Some(url) = target {
                    if let Some(content) = state.content.get(&id) {
                        content.webview.load(url);
                    }
                }
            }
        }
        Action::ReloadActive => {
            if let Some(id) = state.tabs.active_id() {
                if let Some(content) = state.content.get(&id) {
                    content.webview.reload();
                }
            }
        }
        Action::AddressSubmitted(text) => {
            let engine = state.settings.search_engine;
            let url = match normalize_url(&text, "https") {
                Ok(u) => u,
                Err(_) => Url::parse(&engine.search_url(&text))
                    .unwrap_or_else(|_| Url::parse("about:blank").unwrap()),
            };
            if let Some(id) = state.tabs.active_id() {
                if let Some(content) = state.content.get(&id) {
                    content.webview.load(url);
                }
            } else {
                state.open_tab(url, true);
                if let Some(id) = state.tabs.active_id() {
                    let events = vec![TabEvent::Activated(id)];
                    state.apply_tab_events(events, active, delegate);
                }
            }
        }
        Action::TogglePanel(kind) => {
            if let Some(chrome) = &state.chrome {
                chrome.set_panel_kind(kind);
            }
        }
        Action::ToggleBookmark => {
            if let Some(id) = state.tabs.active_id() {
                if let Ok(tab) = state.tabs.tab(id) {
                    let url = tab.url.clone();
                    let title = tab.title.clone();
                    if state.bookmarks.contains_url(&url) {
                        let bm_id = state
                            .bookmarks
                            .all()
                            .iter()
                            .find(|b| b.url == url)
                            .map(|b| b.id);
                        if let Some(bm_id) = bm_id {
                            state.bookmarks.remove(bm_id);
                        }
                    } else {
                        state.bookmarks.add(url, title, vec![], now_unix());
                    }
                }
            }
        }
        Action::RemoveBookmark(id) => {
            state.bookmarks.remove(id);
        }
        Action::ClearHistory => {
            state.history.clear_all();
        }
        Action::ToggleSetting(key) => {
            let current = match key.as_str() {
                "block_ads" => state.settings.block_ads,
                "dnt" => state.settings.dnt,
                "restore_session" => state.settings.restore_session,
                _ => false,
            };
            let _ = state
                .settings
                .set_from_str(&key, if current { "false" } else { "true" });
            let _ = state.settings.save(&state.data_dir.join("settings.json"));
            apply_engine_prefs(state);
        }
        Action::CycleSetting(key) => {
            match key.as_str() {
                "locale" => {
                    use brow_shell_core::Lang;
                    let next = match state.settings.locale {
                        Lang::En => Lang::Ar,
                        Lang::Ar => Lang::En,
                    };
                    let _ = state.settings.set_from_str("locale", next.code());
                    state.l10n = brow_shell_core::L10n::new(next);
                }
                "search_engine" => {
                    use brow_shell_core::settings::SearchEngine;
                    let next = match state.settings.search_engine {
                        SearchEngine::Google => SearchEngine::DuckDuckGo,
                        SearchEngine::DuckDuckGo => SearchEngine::Bing,
                        SearchEngine::Bing => SearchEngine::Google,
                    };
                    state.settings.search_engine = next;
                }
                _ => {}
            }
            let _ = state.settings.save(&state.data_dir.join("settings.json"));
        }
        Action::SleepActiveTab => {
            if let Some(id) = state.tabs.active_id() {
                if let Ok(events) = state.tabs.sleep_tab(id) {
                    state.apply_tab_events(events, active, delegate);
                }
            }
        }
    }
    state.sync_chrome();
}

fn apply_engine_prefs(state: &mut BrowState) {
    // Bridge shell settings into engine prefs (phases 2+3).
    if let Some(servo) = &state.servo {
        let _ = servo.set_preference(
            "hidden_webview_max_fps",
            servo::PrefValue::Int(i64::from(state.settings.hidden_webview_fps)),
        );
    }
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

// ---- chrome window event routing ------------------------------------------

fn route_chrome_event(state: &SharedState, event: &WinitWindowEvent) {
    let Some(surface) = state.borrow().chrome_surface.clone() else {
        return;
    };
    let slint_window = surface.slint_window.clone();
    let scale = slint_window.window().scale_factor() as f64;
    match event {
        WinitWindowEvent::Resized(size) => {
            surface.resize(size.width.max(1), size.height.max(1));
            // Slint expects the logical size (physical / scale factor).
            slint_window.window().dispatch_event(SlintWindowEvent::Resized {
                size: slint::LogicalSize::new(
                    (size.width.max(1)) as f32 / scale as f32,
                    (size.height.max(1)) as f32 / scale as f32,
                ),
            });
        }
        WinitWindowEvent::ScaleFactorChanged { scale_factor, .. } => {
            slint_window
                .window()
                .dispatch_event(SlintWindowEvent::ScaleFactorChanged {
                    scale_factor: *scale_factor as f32,
                });
        }
        WinitWindowEvent::CursorMoved { position, .. } => {
            surface.last_cursor.set((position.x, position.y));
            slint_window
                .window()
                .dispatch_event(SlintWindowEvent::PointerMoved {
                    position: LogicalPosition::new(
                        position.x as f32 / scale as f32,
                        position.y as f32 / scale as f32,
                    ),
                });
        }
        WinitWindowEvent::CursorLeft { .. } => {
            slint_window
                .window()
                .dispatch_event(SlintWindowEvent::PointerExited);
        }
        WinitWindowEvent::MouseInput {
            state: mstate,
            button,
            ..
        } => {
            let slint_button = match button {
                winit::event::MouseButton::Left => PointerEventButton::Left,
                winit::event::MouseButton::Right => PointerEventButton::Right,
                winit::event::MouseButton::Middle => PointerEventButton::Middle,
                _ => PointerEventButton::Other,
            };
            let (x, y) = surface.last_cursor.get();
            let position =
                LogicalPosition::new(x as f32 / scale as f32, y as f32 / scale as f32);
            let ev = match mstate {
                winit::event::ElementState::Pressed => {
                    SlintWindowEvent::PointerPressed { position, button: slint_button }
                }
                winit::event::ElementState::Released => {
                    SlintWindowEvent::PointerReleased { position, button: slint_button }
                }
            };
            slint_window.window().dispatch_event(ev);
        }
        WinitWindowEvent::MouseWheel { delta, .. } => {
            let (dx, dy) = match delta {
                winit::event::MouseScrollDelta::LineDelta(x, y) => (*x * 60.0, *y * 60.0),
                winit::event::MouseScrollDelta::PixelDelta(p) => (p.x as f32, p.y as f32),
            };
            let (x, y) = surface.last_cursor.get();
            slint_window
                .window()
                .dispatch_event(SlintWindowEvent::PointerScrolled {
                    position: LogicalPosition::new(
                        x as f32 / scale as f32,
                        y as f32 / scale as f32,
                    ),
                    delta_x: dx,
                    delta_y: dy,
                });
        }
        WinitWindowEvent::KeyboardInput { event, .. } => {
            let text = match &event.logical_key {
                winit::keyboard::Key::Character(c) => Some(c.to_string()),
                winit::keyboard::Key::Named(named) => slint_named_key_text(named),
                _ => None,
            };
            let Some(text) = text else { return };
            let ev = match event.state {
                winit::event::ElementState::Pressed => {
                    SlintWindowEvent::KeyPressed { text: SharedString::from(text) }
                }
                winit::event::ElementState::Released => {
                    SlintWindowEvent::KeyReleased { text: SharedString::from(text) }
                }
            };
            slint_window.window().dispatch_event(ev);
        }
        WinitWindowEvent::Focused(focused) => {
            slint_window
                .window()
                .dispatch_event(SlintWindowEvent::WindowActiveChanged(*focused));
        }
        _ => {}
    }
}

fn slint_named_key_text(named: &winit::keyboard::NamedKey) -> Option<String> {
    use winit::keyboard::NamedKey as N;
    // Slint dispatches special keys as single-char strings using the Qt/DOM
    // key codes tabulated in i-slint-common's for_each_keys (arrows in the
    // F7xx private-use area). Matching those exact codepoints keeps the
    // dispatched events identical to what Slint's own winit backend emits.
    let ch = match named {
        N::Enter => '\u{000a}',
        N::Backspace => '\u{0008}',
        N::Tab => '\u{0009}',
        N::Escape => '\u{001b}',
        N::Delete => '\u{007f}',
        N::ArrowUp => '\u{F700}',
        N::ArrowDown => '\u{F701}',
        N::ArrowLeft => '\u{F702}',
        N::ArrowRight => '\u{F703}',
        N::Home => '\u{F729}',
        N::End => '\u{F72B}',
        N::PageUp => '\u{F72C}',
        N::PageDown => '\u{F72D}',
        N::Shift => '\u{0010}',
        N::Control => '\u{0011}',
        N::Super => '\u{0017}',
        N::Alt => '\u{0012}',
        _ => return None,
    };
    Some(ch.to_string())
}

// ---- content window event routing ------------------------------------------

fn route_content_event(
    state: &SharedState,
    window_id: WindowId,
    event: WinitWindowEvent,
    active: &ActiveEventLoop,
    delegate: &Rc<BrowWebViewDelegate>,
) {
    let tab_id = {
        let state = state.borrow();
        state
            .content
            .iter()
            .find(|(_, c)| c.window.id() == window_id)
            .map(|(id, _)| *id)
    };
    let Some(tab_id) = tab_id else { return };

    // Clicking / focusing a content window activates its tab (browser
    // behavior): throttling + chrome state follow.
    if let WinitWindowEvent::Focused(true) = &event {
        let events = state
            .borrow_mut()
            .tabs
            .activate_tab(tab_id, Instant::now());
        state
            .borrow_mut()
            .apply_tab_events(events, active, delegate);
    }

    let mut state = state.borrow_mut();
    let Some(content) = state.content.get_mut(&tab_id) else {
        return;
    };
    match event {
        WinitWindowEvent::RedrawRequested => {
            let _ = content.rendering_context.make_current();
            content.webview.paint();
            content.rendering_context.present();
        }
        WinitWindowEvent::Resized(size) => {
            content.webview.resize(size);
        }
        WinitWindowEvent::CursorMoved { position, .. } => {
            content.last_cursor.set((position.x, position.y));
            content
                .webview
                .notify_input_event(InputEvent::MouseMove(MouseMoveEvent::new(
                    WebViewPoint::Device(DevicePoint::new(position.x as f32, position.y as f32)),
                )));
        }
        WinitWindowEvent::CursorLeft { .. } => {
            content
                .webview
                .notify_input_event(InputEvent::MouseLeftViewport(MouseLeftViewportEvent {
                    focus_moving_to_another_iframe: false,
                }));
        }
        WinitWindowEvent::MouseInput {
            state: mstate,
            button,
            ..
        } => {
            let (action, button) = match (mstate, button) {
                (winit::event::ElementState::Pressed, winit::event::MouseButton::Left) => {
                    (MouseButtonAction::Down, MouseButton::Primary)
                }
                (winit::event::ElementState::Released, winit::event::MouseButton::Left) => {
                    (MouseButtonAction::Up, MouseButton::Primary)
                }
                (winit::event::ElementState::Pressed, winit::event::MouseButton::Right) => {
                    (MouseButtonAction::Down, MouseButton::Secondary)
                }
                (winit::event::ElementState::Released, winit::event::MouseButton::Right) => {
                    (MouseButtonAction::Up, MouseButton::Secondary)
                }
                // DOM button 1 ("middle") maps to Auxiliary in servo's enum.
                (winit::event::ElementState::Pressed, winit::event::MouseButton::Middle) => {
                    (MouseButtonAction::Down, MouseButton::Auxiliary)
                }
                (winit::event::ElementState::Released, winit::event::MouseButton::Middle) => {
                    (MouseButtonAction::Up, MouseButton::Auxiliary)
                }
                (winit::event::ElementState::Pressed, _) => {
                    (MouseButtonAction::Down, MouseButton::Primary)
                }
                (winit::event::ElementState::Released, _) => {
                    (MouseButtonAction::Up, MouseButton::Primary)
                }
            };
            let (x, y) = content.last_cursor.get();
            content
                .webview
                .notify_input_event(InputEvent::MouseButton(MouseButtonEvent::new(
                    action,
                    button,
                    WebViewPoint::Device(DevicePoint::new(x as f32, y as f32)),
                )));
        }
        WinitWindowEvent::MouseWheel { delta, .. } => {
            let (x, y, mode) = match delta {
                winit::event::MouseScrollDelta::LineDelta(dx, dy) => {
                    ((dx * 76.0) as f64, (dy * 76.0) as f64, WheelMode::DeltaLine)
                }
                winit::event::MouseScrollDelta::PixelDelta(p) => {
                    (p.x, p.y, WheelMode::DeltaPixel)
                }
            };
            let (cx, cy) = content.last_cursor.get();
            content
                .webview
                .notify_input_event(InputEvent::Wheel(WheelEvent::new(
                    WheelDelta {
                        x,
                        y,
                        z: 0.0,
                        mode,
                    },
                    WebViewPoint::Device(DevicePoint::new(cx as f32, cy as f32)),
                )));
        }
        WinitWindowEvent::KeyboardInput { event, .. } => {
            let kb = crate::keymap::keyboard_event_from_winit(&event, Default::default());
            content
                .webview
                .notify_input_event(InputEvent::Keyboard(servo::KeyboardEvent::new(kb)));
        }
        _ => {}
    }
}

// ---- tab lifecycle -> engine primitives (phase 3 decision D4) --------------

#[cfg(feature = "engine")]
impl BrowState {
    /// Apply brow-shell-core tab lifecycle events to engine primitives:
    ///
    /// | event        | engine primitive                                       |
    /// |--------------|--------------------------------------------------------|
    /// | Created      | create the WebView (one winit window + context/tab)    |
    /// | Activated    | show + unthrottle (+ create/restore if missing)        |
    /// | Backgrounded | hide + `set_throttled(true)` (timers clamp to 1 s)     |
    /// | Slept        | same as backgrounded (already backgrounded)            |
    /// | Woken        | show + unthrottle                                      |
    /// | Discarded    | destroy the WebView (core keeps the ~2 KiB payload)    |
    /// | Restored     | recreate from the payload (URL + zoom + scroll replay) |
    /// | Closed       | destroy the WebView                                     |
    pub fn apply_tab_events(
        &mut self,
        events: Vec<TabEvent>,
        active: &ActiveEventLoop,
        delegate: &Rc<BrowWebViewDelegate>,
    ) {
        for event in events {
            match event {
                TabEvent::Created(id) => {
                    if let Ok(tab) = self.tabs.tab(id) {
                        self.ensure_webview(id, tab.url.clone(), active, delegate);
                    }
                }
                TabEvent::Activated(id) => {
                    // First activation of a tab without a WebView (covers
                    // session-restored tabs and tabs created while dormant).
                    if !self.content.contains_key(&id) {
                        if let Ok(tab) = self.tabs.tab(id) {
                            self.ensure_webview(id, tab.url.clone(), active, delegate);
                        }
                    }
                    if let Some(content) = self.content.get(&id) {
                        content.window.set_visible(true);
                        content.window.focus_window();
                        content.webview.show();
                        content.webview.set_throttled(false);
                        content.webview.focus();
                    }
                }
                TabEvent::Backgrounded(id) | TabEvent::Slept(id) => {
                    if let Some(content) = self.content.get(&id) {
                        content.window.set_visible(false);
                        content.webview.hide();
                        content.webview.set_throttled(true);
                    }
                }
                TabEvent::Woken(id) => {
                    if let Some(content) = self.content.get(&id) {
                        content.window.set_visible(true);
                        content.webview.show();
                        content.webview.set_throttled(false);
                    }
                }
                TabEvent::Discarded(id, _payload) => self.destroy_webview(id),
                TabEvent::Closed(id) => self.destroy_webview(id),
                TabEvent::Restored(id, url, scroll_y, zoom) => {
                    self.ensure_webview(id, url.clone(), active, delegate);
                    if let Some(content) = self.content.get(&id) {
                        content.webview.set_page_zoom(zoom);
                        if scroll_y > 0.0 {
                            // Approximate replay: one synthetic scroll delta in
                            // device pixels (the engine clamps to the document
                            // extent). Exact-offset restore needs the engine's
                            // session-restore path.
                            let scale = content.webview.device_pixels_per_css_pixel();
                            let dy = (scroll_y as f32) * scale.get();
                            content.webview.notify_scroll_event(
                                Scroll::Delta(WebViewVector::Device(
                                    servo::DeviceVector2D::new(0.0, dy),
                                )),
                                WebViewPoint::Device(DevicePoint::new(0.0, 0.0)),
                            );
                        }
                    }
                }
            }
            self.sync_chrome();
        }
    }

    /// Create the engine side of one tab (idempotent): a winit window, a
    /// surfman/GL rendering context and a Servo WebView bound to the shared
    /// delegate. The tab keeps its URL from brow-shell-core.
    fn ensure_webview(
        &mut self,
        id: TabId,
        url: Url,
        active: &ActiveEventLoop,
        delegate: &Rc<BrowWebViewDelegate>,
    ) {
        if self.content.contains_key(&id) {
            return;
        }
        let Some(servo) = self.servo.as_ref() else {
            return;
        };
        let window = active
            .create_window(
                Window::default_attributes()
                    .with_title("brow")
                    .with_inner_size(winit::dpi::PhysicalSize::new(
                        DEFAULT_WIDTH as u32,
                        DEFAULT_CONTENT_HEIGHT as u32,
                    )),
            )
            .expect("content window");
        let display_handle = active.display_handle().expect("display handle");
        let window_handle = window.window_handle().expect("window handle");
        let rendering_context = Rc::new(
            servo::WindowRenderingContext::new(
                display_handle,
                window_handle,
                window.inner_size(),
            )
            .expect("content rendering context"),
        );
        let webview = WebViewBuilder::new(servo, rendering_context.clone())
            .delegate(delegate.clone())
            .url(url)
            .build();
        webview.resize(window.inner_size());
        let webview_id = webview.id();
        self.webview_to_tab.insert(webview_id, id);
        self.content.insert(
            id,
            ContentWindow {
                window,
                rendering_context,
                webview,
                last_cursor: Cell::new((0.0, 0.0)),
            },
        );
    }

    /// Drop the engine side of one tab. Dropping the `ContentWindow` destroys
    /// the winit window and releases the compositor buffers; brow-shell-core
    /// retains the small payload for a later restore.
    fn destroy_webview(&mut self, id: TabId) {
        if let Some(content) = self.content.remove(&id) {
            let webview_id = content.webview.id();
            self.webview_to_tab.remove(&webview_id);
        }
        let _ = self.tab_loading.remove(&id.0);
    }
}

// ---- chrome rendering ------------------------------------------------------

pub fn render_chrome(state: &RefCell<BrowState>) {
    let surface = state.borrow().chrome_surface.clone();
    let Some(surface) = surface else { return };
    surface.draw_if_needed();
}
