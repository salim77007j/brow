/* WebViewDelegate implementation: forwards engine events into the chrome
 * state (brow-shell-core) and the Slint UI. */

use std::cell::RefCell;
use std::rc::Rc;

use servo::{LoadStatus, WebView, WebViewDelegate};
use url::Url;

use crate::state::BrowState;

pub struct BrowWebViewDelegate(pub Rc<RefCell<BrowState>>);

impl WebViewDelegate for BrowWebViewDelegate {
    fn notify_url_changed(&self, webview: WebView, url: Url) {
        let mut state = self.0.borrow_mut();
        let Some(tab_id) = state.webview_to_tab.get(&webview.id()).copied() else {
            return;
        };
        let _ = state.tabs.record_navigation(tab_id, url);
        state.sync_chrome();
    }

    fn notify_page_title_changed(&self, webview: WebView, title: Option<String>) {
        let mut state = self.0.borrow_mut();
        let Some(tab_id) = state.webview_to_tab.get(&webview.id()).copied() else {
            return;
        };
        let _ = state.tabs.set_title(tab_id, title.unwrap_or_default());
        state.sync_chrome();
    }

    fn notify_load_status_changed(&self, webview: WebView, status: LoadStatus) {
        let loading = matches!(status, LoadStatus::Started | LoadStatus::HeadParsed);
        let mut state = self.0.borrow_mut();
        let Some(tab_id) = state.webview_to_tab.get(&webview.id()).copied() else {
            return;
        };

        if !loading {
            // Record a history visit when a page finishes loading. Copy the
            // needed fields out first: `tabs.tab()` borrows state immutably
            // while `history.record_visit_now` needs the mutable borrow.
            let visit = state.tabs.tab(tab_id).ok().map(|tab| {
                let title = if tab.title.is_empty() {
                    tab.url.host_str().unwrap_or("").to_string()
                } else {
                    tab.title.clone()
                };
                (tab.url.to_string(), title)
            });
            if let Some((url, title)) = visit {
                state.history.record_visit_now(&url, &title);
            }
        }
        state.tab_loading.insert(tab_id.0, loading);
        state.sync_chrome();
    }

    fn notify_new_frame_ready(&self, webview: WebView) {
        // The content window repaints on its own RedrawRequested pump; ask
        // winit for one now.
        let state = self.0.borrow();
        if let Some(content) = state
            .webview_to_tab
            .get(&webview.id())
            .and_then(|tab_id| state.content.get(tab_id))
        {
            content.window.request_redraw();
        }
    }

    fn notify_closed(&self, webview: WebView) {
        // Engine-driven close (e.g. window.close()): drop our side too.
        let mut state = self.0.borrow_mut();
        if let Some(tab_id) = state.webview_to_tab.remove(&webview.id()) {
            state.content.remove(&tab_id);
            let _ = state.tab_loading.remove(&tab_id.0);
            state.sync_chrome();
        }
    }
}
