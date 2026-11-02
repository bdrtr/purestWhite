//! The storage loop: the application's jobs answered with events, pages read into the page buffer
//! for drawing to copy, nothing found without a card, and the rest between two jobs.

use super::pages::{bmp, card, frame, pattern, shows_pattern};
use crate::{Links, run_storage};
use embassy_futures::block_on;
use embassy_futures::select::{Either, select};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use purestwhite_app::{Event, Job};
use purestwhite_fat::Volume;
use purestwhite_fat::testing::MemCard;
use purestwhite_render::{HEIGHT, WIDTH};
use std::boxed::Box;
use std::vec::Vec;

/// The answers of the storage loop over `volume` to `jobs`, given one at a time, and `links`.
fn answers(
    volume: Option<Volume<MemCard>>,
    links: &Links<NoopRawMutex>,
    jobs: &[Job],
) -> Vec<Event> {
    let script = async {
        let mut answers = Vec::new();
        for &job in jobs {
            links.jobs.give(job);
            assert!(!links.jobs.is_idle());
            answers.push(links.events.receive().await);
        }
        answers
    };
    match block_on(select(run_storage(volume, links.storage_links()), script)) {
        Either::First(()) => panic!("nothing asked the storage loop to rest"),
        Either::Second(answers) => answers,
    }
}

#[test]
fn pages_are_counted_and_read_into_the_page_drawing_copies() {
    let page = bmp(1, false, WIDTH, HEIGHT, pattern);
    let Some(volume) = card(&[(1, page.clone()), (2, page)]) else {
        return;
    };
    let links: Links<NoopRawMutex> = Links::new();
    assert!(links.page.give(Box::leak(frame())));
    let jobs = [Job::CountPages, Job::LoadPage(2), Job::LoadPage(3)];
    assert_eq!(
        answers(Some(volume), &links, &jobs),
        [Event::Pages(2), Event::PageLoaded(2), Event::PageFailed(3)]
    );
    assert!(links.jobs.is_idle());
    let mut drawn = frame();
    assert!(links.page.copy_into(&mut drawn));
    assert!(shows_pattern(&drawn));
}

#[test]
fn without_a_card_or_a_page_buffer_nothing_is_found() {
    let links: Links<NoopRawMutex> = Links::new();
    let jobs = [Job::CountPages, Job::LoadPage(1)];
    assert_eq!(
        answers(None, &links, &jobs),
        [Event::Pages(0), Event::PageFailed(1)]
    );
    let Some(volume) = card(&[(1, bmp(1, false, WIDTH, HEIGHT, pattern))]) else {
        return;
    };
    // No page buffer given: the page cannot be read.
    assert_eq!(
        answers(Some(volume), &links, &[Job::LoadPage(1)]),
        [Event::PageFailed(1)]
    );
    assert!(!links.page.copy_into(&mut frame()));
}

#[test]
fn the_page_buffer_is_given_once() {
    let links: Links<NoopRawMutex> = Links::new();
    assert!(links.page.give(Box::leak(frame())));
    assert!(!links.page.give(Box::leak(frame())));
}

#[test]
fn asked_to_rest_the_loop_ends_before_the_next_job() {
    let links: Links<NoopRawMutex> = Links::new();
    let script = async {
        links.storage_rest.signal(());
        links.jobs.give(Job::CountPages);
        core::future::pending::<()>().await;
    };
    let rested = block_on(select(
        run_storage(None::<Volume<MemCard>>, links.storage_links()),
        script,
    ));
    assert!(matches!(rested, Either::First(())), "the loop ended");
    assert!(
        links.events.is_empty(),
        "the job given with the rest is not taken"
    );
}

#[test]
fn asked_to_rest_while_a_page_is_read_the_loop_answers_it_first() {
    let Some(volume) = card(&[(1, bmp(1, false, WIDTH, HEIGHT, pattern))]) else {
        return;
    };
    let links: Links<NoopRawMutex> = Links::new();
    assert!(links.page.give(Box::leak(frame())));
    let script = async {
        links.jobs.give(Job::LoadPage(1));
        // The storage loop takes the job and reads the page a chunk of rows at a time, yielding
        // between them: the rest comes while it reads.
        embassy_futures::yield_now().await;
        assert!(!links.jobs.is_idle(), "not answered yet");
        links.storage_rest.signal(());
        core::future::pending::<()>().await;
    };
    let rested = block_on(select(
        run_storage(Some(volume), links.storage_links()),
        script,
    ));
    assert!(matches!(rested, Either::First(())), "the loop ended");
    assert_eq!(links.events.try_receive(), Ok(Event::PageLoaded(1)));
    assert!(links.jobs.is_idle());
}
