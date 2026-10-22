//! Unit-test-only faults at actual write/sync call sites. No environment switches
//! or hooks are compiled into the library shipped to callers. State belongs to
//! the current test thread so concurrent tests/transfers cannot inherit a fault.
use std::{
    cell::RefCell,
    fs::File,
    io::{self, Write},
    marker::PhantomData,
    rc::Rc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Point {
    JournalWrite,
    JournalSync,
    DirectorySync,
    PayloadSync,
}

struct State {
    point: Point,
    occurrence: usize,
    prefix: usize,
    errno: i32,
    calls: Vec<Point>,
    fired: bool,
}

thread_local! {
    static ACTIVE: RefCell<Option<State>> = const { RefCell::new(None) };
}

// Keep the guard on its creating thread, including during unwinding.
pub(crate) struct Guard(PhantomData<Rc<()>>);

impl Guard {
    pub fn arm(point: Point, occurrence: usize, prefix: usize, errno: i32) -> Self {
        ACTIVE.with_borrow_mut(|active| {
            assert!(active.is_none(), "nested fault injection");
            *active = Some(State {
                point,
                occurrence,
                prefix,
                errno,
                calls: Vec::new(),
                fired: false,
            });
        });
        Self(PhantomData)
    }

    pub fn calls(&self) -> Vec<Point> {
        ACTIVE.with_borrow(|active| active.as_ref().unwrap().calls.clone())
    }

    pub fn fired(&self) -> bool {
        ACTIVE.with_borrow(|active| active.as_ref().unwrap().fired)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        ACTIVE.with_borrow_mut(|active| *active = None);
    }
}

fn hit(point: Point) -> Option<(usize, i32)> {
    ACTIVE.with_borrow_mut(|active| {
        let state = active.as_mut()?;
        state.calls.push(point);
        if !state.fired
            && state.point == point
            && state.calls.iter().filter(|p| **p == point).count() == state.occurrence
        {
            state.fired = true;
            Some((state.prefix, state.errno))
        } else {
            None
        }
    })
}

pub(crate) fn check(point: Point) -> io::Result<()> {
    match hit(point) {
        Some((_, errno)) => Err(io::Error::from_raw_os_error(errno)),
        None => Ok(()),
    }
}

pub(crate) fn before_journal_write(file: &mut File, bytes: &[u8]) -> io::Result<()> {
    if let Some((prefix, errno)) = hit(Point::JournalWrite) {
        // Actually leave a torn record on disk, then return the injected error.
        file.write_all(&bytes[..prefix.min(bytes.len())])?;
        return Err(io::Error::from_raw_os_error(errno));
    }
    Ok(())
}
