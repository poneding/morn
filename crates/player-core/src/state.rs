#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transition {
    Play,
    Pause,
    Stop,
}

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidTransition {
    pub from: PlaybackState,
    pub transition: Transition,
}

pub struct StateMachine {
    state: PlaybackState,
}

impl StateMachine {
    pub fn new() -> Self {
        Self { state: PlaybackState::Stopped }
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }

    pub fn apply(&mut self, t: Transition) -> Result<PlaybackState, InvalidTransition> {
        use PlaybackState::*;
        use Transition::*;
        let next = match (self.state, t) {
            (Stopped, Play) => Playing,
            (Paused, Play) => Playing,
            (Playing, Pause) => Paused,
            (_, Stop) => Stopped,
            (from, transition) => return Err(InvalidTransition { from, transition }),
        };
        self.state = next;
        Ok(next)
    }
}

impl Default for StateMachine {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_stopped() {
        let m = StateMachine::new();
        assert_eq!(m.state(), PlaybackState::Stopped);
    }

    #[test]
    fn play_from_stopped_goes_playing() {
        let mut m = StateMachine::new();
        m.apply(Transition::Play).unwrap();
        assert_eq!(m.state(), PlaybackState::Playing);
    }

    #[test]
    fn pause_from_playing_goes_paused() {
        let mut m = StateMachine::new();
        m.apply(Transition::Play).unwrap();
        m.apply(Transition::Pause).unwrap();
        assert_eq!(m.state(), PlaybackState::Paused);
    }

    #[test]
    fn pause_from_stopped_is_error() {
        let mut m = StateMachine::new();
        assert!(m.apply(Transition::Pause).is_err());
        assert_eq!(m.state(), PlaybackState::Stopped);
    }

    #[test]
    fn stop_from_any_goes_stopped() {
        let mut m = StateMachine::new();
        m.apply(Transition::Play).unwrap();
        m.apply(Transition::Stop).unwrap();
        assert_eq!(m.state(), PlaybackState::Stopped);
    }
}
