//! The viewing state: everything the user is looking at.

use crate::playlist::Playlist;
use crate::settings::Settings;

mod chrome;
mod content;
mod slideshow;
mod view;

use self::chrome::Chrome;
use self::content::Content;
use self::slideshow::Slideshow;
use self::view::View;

pub struct Session {
    pub playlist: Playlist,
    pub view: View,
    pub content: Content,
    pub slideshow: Slideshow,
    pub chrome: Chrome,
}

impl Session {
    pub fn new(playlist: Playlist, settings: &Settings, autoplay: bool) -> Self {
        Self {
            playlist,
            view: View::new(settings),
            content: Content::default(),
            slideshow: Slideshow::new(autoplay, settings.play_delay),
            chrome: Chrome::default(),
        }
    }
}
