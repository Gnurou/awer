pub enum LeftRightDir {
    Neutral,
    Left,
    Right,
}

pub enum UpDownDir {
    Neutral,
    Up,
    Down,
}

pub enum ButtonState {
    Released,
    Pushed,
}

pub struct InputState {
    pub horizontal: LeftRightDir,
    pub vertical: UpDownDir,
    pub button: ButtonState,
}

impl InputState {
    pub fn new() -> InputState {
        InputState {
            horizontal: LeftRightDir::Neutral,
            vertical: UpDownDir::Neutral,
            button: ButtonState::Released,
        }
    }
}
