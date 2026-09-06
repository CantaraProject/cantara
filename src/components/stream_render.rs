//! One rendering, two surfaces: turning a presentation into HTML that a phone
//! on the network can be shown.
//!
//! # Why this exists
//!
//! Cantara used to draw a slide twice. The window drew it with the components
//! in [`crate::components::presentation_components`]; the page served to
//! browsers drew it again, in JavaScript, from a description of the slide that
//! [`crate::logic::stream::protocol`] built for it. Two renderers of the same
//! thing, and the second one only ever knew about the features the first had
//! when it was written — which is exactly how a monitor design came out as a
//! plain wall of text on a phone. Its layout and its widgets were not missing
//! from the data; the page simply had no idea such things existed.
//!
//! So the page stops rendering. What it is given is HTML produced by *the same
//! components the projector uses*, through [`dioxus_ssr`]. A feature added to
//! a slide, a design, or a monitor layout reaches the network by existing.
//!
//! # Why here and not in the helper
//!
//! Cantara's network side runs as a second process — see
//! [`crate::logic::network_server`] — and the obvious place to render would be
//! there, next to the socket. It is the wrong place, for two reasons that both
//! come down to the helper deliberately knowing nothing:
//!
//! * **The pictures.** A slide's background and a picture slide are inlined as
//!   data URLs out of [`crate::logic::images`], whose cache is filled from the
//!   library on disk. The helper has neither the library nor the cache; it is
//!   handed rendered bytes and serves them. Cantara has both, warm, because it
//!   is already showing the same slide on the projector.
//! * **The settings.** Which design a view is shown in is a setting, and the
//!   helper has no settings — by design, so that a service cannot be changed by
//!   whatever reaches the socket.
//!
//! Rendering here also keeps the fan-out that made the stream a static page in
//! the first place: the slide is rendered **once per change**, not once per
//! viewer, and the same bytes go to every phone in the building. That is what
//! ruled out giving each viewer a `dioxus-liveview` session the way the remote
//! presenter console has one — a console is one operator, a stream is the whole
//! congregation, and a server-side `VirtualDom` per phone is a different
//! proposition entirely.
//!
//! # What a caller has to know
//!
//! [`render_presentation`] is synchronous and does no I/O of its own beyond the
//! picture cache it reads. It builds a `VirtualDom`, renders it once, and drops
//! it — so anything a component does in an effect or a spawned task has *not*
//! happened. In practice that means:
//!
//! * A picture that is not in the cache yet renders as no picture, and the next
//!   render — after it has been prepared — has it. Nothing waits.
//! * Time-based widgets read the clock at the moment of rendering. They are as
//!   fresh as the last render, which is why anything showing one has to be
//!   re-rendered on a timer rather than only when the slide changes.

use dioxus::prelude::*;

use crate::components::monitor_view::DesignedPresentation;
use crate::components::presentation_components::PresentationRole;
use crate::logic::settings::PresentationDesign;
use crate::logic::states::RunningPresentation;

/// The presentation as HTML, drawn the way `design` says it should be.
///
/// `design` is the design of the *view* being rendered — a stage monitor's, a
/// stream's — or `None` to use the design the presentation's current chapter
/// carries, which is what the projection shows.
///
/// The markup is a fragment, not a document: it carries no stylesheet and no
/// `<html>`. Whoever serves it says how it is dressed, because the stylesheets
/// are the same files the window links and are better sent once with the page
/// than with every slide.
#[cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "the stream still serves its own page; wiring this in is the                   next step, and it is tested and documented in the meantime"
    )
)]
pub fn render_presentation(
    presentation: &RunningPresentation,
    design: Option<PresentationDesign>,
) -> String {
    let mut dom = VirtualDom::new_with_props(
        StreamRoot,
        StreamRootProps {
            presentation: presentation.clone(),
            design,
        },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// The root of a rendering that has no window.
///
/// Its only job is to turn the value it is given into the signal the
/// components want. A `Signal` cannot be made outside a running `VirtualDom`,
/// so it cannot be built by the caller and handed in — which is the whole
/// reason this component exists rather than [`render_presentation`] calling
/// [`DesignedPresentation`] directly.
#[component]
fn StreamRoot(
    presentation: RunningPresentation,
    design: Option<PresentationDesign>,
) -> Element {
    let running_presentation = use_signal(|| presentation.clone());

    rsx! {
        DesignedPresentation {
            running_presentation,
            design,
            // Never the window the audience is looking at: this rendering
            // drives no timer, publishes no layout size, and is one of many.
            role: PresentationRole::Follower,
            // Shown inside whatever box the page gives it, not filling a
            // window of its own.
            contained: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::logic::settings::{
        MonitorDesign, MonitorLayout, MonitorWidget, PresentationDesignSettings, WidgetKind,
        WidgetPlacement,
    };
    use crate::logic::sourcefiles::{SourceFile, SourceFileType};
    use crate::logic::states::SlideChapter;
    use cantara_songlib::slides::SlideSettings;

    /// A short service of one song, built the way a presentation is.
    fn service() -> RunningPresentation {
        let slides = crate::logic::presentation::slides_from_song_content(
            "#title: Amazing Grace\n\nAmazing grace how sweet the sound\nThat saved a wretch like me\n\n---\n\nI once was lost but now am found\nWas blind but now I see\n",
            "Amazing Grace.song",
            &SlideSettings::default(),
            "Amazing Grace",
            &[],
        )
        .expect("the preview song builds into slides");

        let chapter = SlideChapter::new(
            slides,
            SourceFile {
                name: "Amazing Grace".to_string(),
                path: std::path::PathBuf::from("Amazing Grace.song"),
                file_type: SourceFileType::Song,
                md5_hash: None,
                relative_path: None,
            },
            None,
            None,
        );

        RunningPresentation::new(vec![chapter])
    }

    fn monitor_design(layout: MonitorLayout, widgets: Vec<MonitorWidget>) -> PresentationDesign {
        PresentationDesign {
            name: "Stage".to_string(),
            description: String::new(),
            presentation_design_settings: PresentationDesignSettings::Monitor(MonitorDesign {
                layout,
                widgets,
                ..MonitorDesign::default()
            }),
        }
    }

    /// The ordinary case: an audience design renders the slide that is up.
    ///
    /// This is what the stream showed before, and it has to keep showing it —
    /// the whole change is meant to be invisible for a design that has not
    /// changed.
    #[test]
    fn an_audience_design_renders_the_slide_that_is_up() {
        let mut running = service();
        running.jump_to(0, 1);

        let html = render_presentation(&running, None);

        assert!(
            html.contains("Amazing grace how sweet the sound"),
            "the slide's own words are missing from: {html}"
        );
        assert!(
            html.contains("presentation"),
            "the slide was not drawn by the presentation components: {html}"
        );
    }

    /// The failure this module was written for: a monitor design used to reach
    /// a phone as a plain slide, because the page rendering it had never heard
    /// of layouts. Now the layout is what is rendered.
    #[test]
    fn a_monitor_design_renders_its_layout_and_not_a_bare_slide() {
        let mut running = service();
        running.jump_to(0, 1);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert!(
            html.contains("monitor-view"),
            "a monitor design did not render as a monitor view: {html}"
        );
        assert!(
            html.contains("presenter-text-panel"),
            "the slide list is missing: {html}"
        );
    }

    /// The list shows the service, not only the slide that is up — which is
    /// the point of it, and something a single-slide renderer cannot express.
    #[test]
    fn the_slide_list_renders_more_than_the_current_slide() {
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert!(
            html.contains("Amazing grace how sweet the sound"),
            "the first slide is missing: {html}"
        );
        assert!(
            html.contains("I once was lost but now am found"),
            "a slide that is not the current one is missing: {html}"
        );
    }

    /// The speaker layout renders both slides and says which is which.
    #[test]
    fn the_speaker_layout_renders_the_current_slide_and_the_next() {
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::Speaker {
                    next_slide_share: 0.25,
                },
                Vec::new(),
            )),
        );

        assert!(html.contains("monitor-speaker-current"), "no current slide");
        assert!(html.contains("monitor-speaker-next"), "no next slide");
    }

    /// Widgets reach the network too. They are part of the design, and the old
    /// page had no notion of them at all.
    #[test]
    fn widgets_are_rendered_into_the_stream() {
        let mut running = service();
        running.jump_to(0, 0);

        let html = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                vec![
                    MonitorWidget {
                        kind: WidgetKind::Clock { with_date: false },
                        placement: WidgetPlacement::TopRight,
                    },
                    MonitorWidget {
                        kind: WidgetKind::ChapterTimer {
                            warn_after_seconds: None,
                        },
                        placement: WidgetPlacement::BottomLeft,
                    },
                ],
            )),
        );

        assert!(html.contains("monitor-clock"), "the clock is missing: {html}");
        assert!(html.contains("monitor-timer"), "the timer is missing: {html}");
        assert!(
            html.contains("monitor-widget-top-right"),
            "a widget was not put in the corner it was given: {html}"
        );
    }

    /// The design decides, so two designs over the same presentation give two
    /// different renderings. Without this the whole exercise would be a very
    /// elaborate way of ignoring the design.
    #[test]
    fn the_design_is_what_decides_the_rendering() {
        let mut running = service();
        running.jump_to(0, 0);

        let audience = render_presentation(&running, None);
        let monitor = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert_ne!(audience, monitor);
        assert!(!audience.contains("monitor-view"));
    }

    /// A presentation that has not started renders something rather than
    /// failing. The address is open before the service begins, and a viewer who
    /// opens it early must not be handed a panic.
    #[test]
    fn a_presentation_with_no_slides_renders_without_failing() {
        let running = RunningPresentation::new(Vec::new());

        let audience = render_presentation(&running, None);
        let monitor = render_presentation(
            &running,
            Some(monitor_design(
                MonitorLayout::SlideList { context: None },
                Vec::new(),
            )),
        );

        assert!(!audience.is_empty());
        assert!(monitor.contains("monitor-view"));
    }

    /// Rendering the same thing twice gives the same bytes.
    ///
    /// What the stream sends is compared against what it last sent, so that a
    /// state which has not changed is not pushed to every phone again. A
    /// rendering that differed run to run — an id counted up per render, say —
    /// would make every republish look like a change.
    #[test]
    fn the_same_presentation_renders_the_same_way_twice() {
        let mut running = service();
        running.jump_to(0, 1);

        assert_eq!(
            render_presentation(&running, None),
            render_presentation(&running, None)
        );
    }

    /// Moving the presentation changes the rendering. The obvious property,
    /// and the one that would make the stream look frozen if it failed.
    #[test]
    fn moving_to_another_slide_changes_the_rendering() {
        let mut running = service();
        running.jump_to(0, 0);
        let first = render_presentation(&running, None);

        running.next_slide();

        assert_ne!(first, render_presentation(&running, None));
    }
}
