/* winit EventLoopProxy-based EventLoopWaker for libservo. */

use log::warn;
use winit::event_loop::EventLoopProxy;

use crate::state::BrowEvent;

#[derive(Clone)]
pub struct BrowWaker(pub EventLoopProxy<BrowEvent>);

impl servo::EventLoopWaker for BrowWaker {
    fn clone_box(&self) -> Box<dyn servo::EventLoopWaker> {
        Box::new(self.clone())
    }

    fn wake(&self) {
        if let Err(error) = self.0.send_event(BrowEvent::Wake) {
            warn!("Failed to wake winit event loop: {error:?}");
        }
    }
}
