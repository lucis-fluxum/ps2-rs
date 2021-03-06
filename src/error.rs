#[derive(Debug)]
pub enum ControllerError {
    Timeout,
    TestFailed { response: u8 },
}

#[derive(Debug)]
pub enum KeyboardError {
    BufferOverrun,
    SelfTestFailed,
    Resend,
    KeyDetectionError,
    InvalidResponse(u8),
    ControllerError(ControllerError),
}

#[derive(Debug)]
pub enum MouseError {
    SelfTestFailed,
    Resend,
    InvalidResponse(u8),
    InvalidResolution(u8),
    InvalidSampleRate(u8),
    ControllerError(ControllerError),
}

impl From<ControllerError> for KeyboardError {
    fn from(err: ControllerError) -> Self {
        KeyboardError::ControllerError(err)
    }
}

impl From<ControllerError> for MouseError {
    fn from(err: ControllerError) -> Self {
        MouseError::ControllerError(err)
    }
}
