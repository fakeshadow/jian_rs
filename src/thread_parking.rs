use core::time::Duration;

use std::thread::{self, ThreadId};

use parking::{Parker, Unparker};

// A unparker for given threadId.
pub(crate) struct ThreadUnparker {
    state: ParkerState,
    thread_id: ThreadId,
    unparker: Unparker,
}

// A parker paired to it's ThreadUnparker
pub(crate) struct ThreadParker {
    timeout: Duration,
    thread_id: ThreadId,
    parker: Parker,
}

pub(crate) fn pair(timeout: Duration) -> (ThreadUnparker, ThreadParker) {
    let parker = Parker::new();
    let thread_id = thread::current().id();

    let unparker = ThreadUnparker {
        state: ParkerState::UNPARKING,
        thread_id,
        unparker: parker.unparker(),
    };

    let parker = ThreadParker {
        timeout,
        thread_id,
        parker,
    };

    (unparker, parker)
}

impl ThreadUnparker {
    pub(crate) fn id(&self) -> ThreadId {
        self.thread_id
    }

    // Only unpark if we are in parking state. and return true if we successfully unparked.
    pub(crate) fn try_unpark(&mut self) -> bool {
        let set = self.set_state(ParkerState::UNPARKING);

        if set {
            self.unparker.unpark();
        }

        set
    }

    pub(crate) fn set_park(&mut self) {
        self.set_state(ParkerState::PARKING);
    }

    fn set_state(&mut self, state: ParkerState) -> bool {
        if self.state != state {
            self.state = state;
            true
        } else {
            false
        }
    }
}

impl ThreadParker {
    pub(crate) fn id(&self) -> ThreadId {
        self.thread_id
    }

    pub(crate) fn park_timeout(&self) -> bool {
        !self.parker.park_timeout(self.timeout)
    }
}

enum ParkerState {
    PARKING,
    UNPARKING,
}

impl PartialEq for ParkerState {
    fn eq(&self, other: &Self) -> bool {
        match self {
            ParkerState::PARKING => match other {
                ParkerState::PARKING => true,
                ParkerState::UNPARKING => false,
            },
            ParkerState::UNPARKING => match other {
                ParkerState::PARKING => false,
                ParkerState::UNPARKING => true,
            },
        }
    }
}
