use x86_64::instructions::port::Port;

use crate::{
    error::ControllerError,
    flags::{
        ControllerConfigFlags, ControllerStatusFlags, InputPortFlags, OutputPortFlags,
        TestPortFlags,
    },
    keyboard::Keyboard,
    mouse::Mouse,
};

const DATA_REGISTER: u16 = 0x60;
const COMMAND_REGISTER: u16 = 0x64;
const DEFAULT_TIMEOUT: usize = 10_000;

type Result<T> = core::result::Result<T, ControllerError>;

#[repr(u8)]
pub(crate) enum Command {
    ReadInternalRam = 0x20,
    WriteInternalRam = 0x60,
    DisableMouse = 0xa7,
    EnableMouse = 0xa8,
    TestMouse = 0xa9,
    TestController = 0xaa,
    TestKeyboard = 0xab,
    DiagnosticDump = 0xac,
    DisableKeyboard = 0xad,
    EnableKeyboard = 0xae,
    ReadControllerInput = 0xc0,
    WriteLowInputNibbleToStatus = 0xc1,
    WriteHighInputNibbleToStatus = 0xc2,
    ReadControllerOutput = 0xd0,
    WriteControllerOutput = 0xd1,
    WriteKeyboardBuffer = 0xd2,
    WriteMouseBuffer = 0xd3,
    WriteMouse = 0xd4,
    ReadTestPort = 0xe0,
    PulseOutput = 0xf0,
}

/// The PS/2 controller.
///
/// Provides the functionality of an Intel 8042 chip. Many computers nowadays don't have PS/2
/// connectors, but emulate the mouse and keyboard as PS/2 devices through USB. The implementation
/// of this emulation is usually different from manufacturer to manufacturer and cannot always be
/// relied upon to perform correctly. Therefore, if you're writing an operating system, you should
/// disable this legacy support once the USB controller has been initialized.
#[derive(Debug)]
pub struct Controller {
    command_register: Port<u8>,
    data_register: Port<u8>,
    timeout: usize,
}

impl Controller {
    /// Create a handle to the PS/2 controller. Uses a default IO timeout of 10,000 tries.
    ///
    /// # Safety
    ///
    /// Ensure that IO ports `0x60` and `0x64` are not accessed by any other code, and that only
    /// one `Controller` accesses those ports at any point in time.
    pub const unsafe fn new() -> Self {
        Self::with_timeout(DEFAULT_TIMEOUT)
    }

    /// Like `new`, but allows specifying an IO timeout, which is the number of times an IO
    /// operation will be attempted before returning [`ControllerError::Timeout`].
    pub const unsafe fn with_timeout(timeout: usize) -> Self {
        Self {
            command_register: Port::new(COMMAND_REGISTER),
            data_register: Port::new(DATA_REGISTER),
            timeout,
        }
    }

    /// Obtain a handle to the keyboard.
    pub const fn keyboard(&mut self) -> Keyboard<'_> {
        Keyboard::new(self)
    }

    /// Obtain a handle to the mouse.
    pub const fn mouse(&mut self) -> Mouse<'_> {
        Mouse::new(self)
    }

    /// Read the status register of the controller.
    pub fn read_status(&mut self) -> ControllerStatusFlags {
        ControllerStatusFlags::from_bits_truncate(unsafe { self.command_register.read() })
    }

    fn wait_for_read(&mut self) -> Result<()> {
        let mut cycles = 0;
        while cycles < self.timeout {
            if self
                .read_status()
                .contains(ControllerStatusFlags::OUTPUT_FULL)
            {
                return Ok(());
            }
            cycles += 1;
        }
        Err(ControllerError::Timeout)
    }

    fn wait_for_write(&mut self) -> Result<()> {
        let mut cycles = 0;
        while cycles < self.timeout {
            if !self
                .read_status()
                .contains(ControllerStatusFlags::INPUT_FULL)
            {
                return Ok(());
            }
            cycles += 1;
        }
        Err(ControllerError::Timeout)
    }

    pub(crate) fn write_command(&mut self, command: Command) -> Result<()> {
        self.wait_for_write()?;
        unsafe { self.command_register.write(command as u8) };
        Ok(())
    }

    /// Read a byte from the data buffer once it is full.
    ///
    /// If there is no data available to read within the configured timeout, this will return
    /// [`ControllerError::Timeout`].
    pub fn read_data(&mut self) -> Result<u8> {
        self.wait_for_read()?;
        Ok(unsafe { self.data_register.read() })
    }

    /// Write a byte to the data buffer once it is empty.
    ///
    /// If a write cannot be performed within the configured timeout, this will return
    /// [`ControllerError::Timeout`].
    pub fn write_data(&mut self, data: u8) -> Result<()> {
        self.wait_for_write()?;
        unsafe { self.data_register.write(data) };
        Ok(())
    }

    /// Read a byte from the controller's internal RAM.
    ///
    /// The desired byte index must be between 0 and 31. Byte 0 is also known as the configuration
    /// byte or command byte.
    pub fn read_internal_ram(&mut self, byte_number: u8) -> Result<u8> {
        // Limit from 0 - 31, start command byte at 0x20
        let command = Command::ReadInternalRam as u8 | byte_number & 0x1f;
        // Since we did some bit fiddling, we can't use write_command
        self.wait_for_write()?;
        unsafe {
            self.command_register.write(command as u8);
        }
        self.read_data()
    }

    /// Write a byte to the controller's internal RAM.
    ///
    /// The desired byte index must be between 0 and 31. Byte 0 is also known as the configuration
    /// byte or command byte.
    pub fn write_internal_ram(&mut self, byte_number: u8, data: u8) -> Result<()> {
        // Limit from 0 - 31, start command byte at 0x60
        let command = Command::WriteInternalRam as u8 | byte_number & 0x1f;
        // Since we did some bit fiddling, we can't use write_command
        self.wait_for_write()?;
        unsafe {
            self.command_register.write(command as u8);
        }
        self.write_data(data)
    }

    /// Read the configuration byte (or command byte) of the controller. This is the same as
    /// reading byte 0 of the internal RAM.
    pub fn read_config(&mut self) -> Result<ControllerConfigFlags> {
        Ok(ControllerConfigFlags::from_bits_truncate(
            self.read_internal_ram(0)?,
        ))
    }

    /// Write the configuration byte (or command byte) of the controller. This is the same as
    /// writing to byte 0 of the internal RAM.
    pub fn write_config(&mut self, config: ControllerConfigFlags) -> Result<()> {
        self.write_internal_ram(0, config.bits())
    }

    /// Disable the mouse. Sets the [`ControllerConfigFlags::DISABLE_MOUSE`] flag.
    pub fn disable_mouse(&mut self) -> Result<()> {
        self.write_command(Command::DisableMouse)
    }

    /// Enable the mouse. Clears the [`ControllerConfigFlags::DISABLE_MOUSE`] flag.
    pub fn enable_mouse(&mut self) -> Result<()> {
        self.write_command(Command::EnableMouse)
    }

    /// Perform a self-test on the mouse.
    ///
    /// Returns [`ControllerError::TestFailed`] if the test fails.
    pub fn test_mouse(&mut self) -> Result<()> {
        self.write_command(Command::TestMouse)?;
        match self.read_data()? {
            0x00 => Ok(()),
            err => Err(ControllerError::TestFailed { response: err }),
        }
    }

    /// Perform a self-test on the controller.
    ///
    /// Returns [`ControllerError::TestFailed`] if the test fails.
    pub fn test_controller(&mut self) -> Result<()> {
        self.write_command(Command::TestController)?;
        match self.read_data()? {
            0x55 => Ok(()),
            err => Err(ControllerError::TestFailed { response: err }),
        }
    }

    /// Perform a self-test on the keyboard.
    ///
    /// Returns [`ControllerError::TestFailed`] if the test fails.
    pub fn test_keyboard(&mut self) -> Result<()> {
        self.write_command(Command::TestKeyboard)?;
        match self.read_data()? {
            0x00 => Ok(()),
            err => Err(ControllerError::TestFailed { response: err }),
        }
    }

    /// Dump all bytes of the controller's internal RAM.
    // TODO: Test this, eventually. I wasn't able to get it working with any of my devices
    pub fn diagnostic_dump(&mut self) -> Result<[u8; 32]> {
        self.write_command(Command::DiagnosticDump)?;
        let mut result = [0; 32];
        for byte in result.iter_mut() {
            *byte = self.read_data()?;
        }
        Ok(result)
    }

    /// Disable the keyboard.
    ///
    /// Sets the [`ControllerConfigFlags::DISABLE_KEYBOARD`] flag.
    pub fn disable_keyboard(&mut self) -> Result<()> {
        self.write_command(Command::DisableKeyboard)
    }

    /// Enable the keyboard.
    ///
    /// Clears the [`ControllerConfigFlags::DISABLE_KEYBOARD`] flag.
    pub fn enable_keyboard(&mut self) -> Result<()> {
        self.write_command(Command::EnableKeyboard)
    }

    /// Read the state of the controller's input port.
    pub fn read_input_port(&mut self) -> Result<InputPortFlags> {
        self.write_command(Command::ReadControllerInput)?;
        Ok(InputPortFlags::from_bits_truncate(self.read_data()?))
    }

    /// Write the low nibble of the controller's input port to the low nibble of the controller
    /// status register.
    pub fn write_input_low_nibble_to_status(&mut self) -> Result<()> {
        self.write_command(Command::WriteLowInputNibbleToStatus)
    }

    /// Write the high nibble of the controller's input port to the high nibble of the controller
    /// status register.
    pub fn write_input_high_nibble_to_status(&mut self) -> Result<()> {
        self.write_command(Command::WriteHighInputNibbleToStatus)
    }

    /// Read the state of the controller's output port.
    pub fn read_output_port(&mut self) -> Result<OutputPortFlags> {
        self.write_command(Command::ReadControllerOutput)?;
        Ok(OutputPortFlags::from_bits_truncate(self.read_data()?))
    }

    /// Write the state of the controller's output port.
    pub fn write_output_port(&mut self, output: OutputPortFlags) -> Result<()> {
        self.write_command(Command::WriteControllerOutput)?;
        self.write_data(output.bits())
    }

    /// Write a byte to the data buffer as if it were received from the keyboard.
    ///
    /// This will trigger an interrupt if interrupts are enabled.
    pub fn write_keyboard_buffer(&mut self, data: u8) -> Result<()> {
        self.write_command(Command::WriteKeyboardBuffer)?;
        self.write_data(data)
    }

    /// Write a byte to the data buffer as if it were received from the mouse.
    ///
    /// This will trigger an interrupt if interrupts are enabled.
    pub fn write_mouse_buffer(&mut self, data: u8) -> Result<()> {
        self.write_command(Command::WriteMouseBuffer)?;
        self.write_data(data)
    }

    /// Write a byte to the mouse's data buffer.
    pub fn write_mouse(&mut self, data: u8) -> Result<()> {
        self.write_command(Command::WriteMouse)?;
        self.write_data(data)
    }

    /// Read the state of the controller's test port.
    pub fn read_test_port(&mut self) -> Result<TestPortFlags> {
        self.write_command(Command::ReadTestPort)?;
        Ok(TestPortFlags::from_bits_truncate(self.read_data()?))
    }

    /// Pulse the low nibble of the given byte onto the lower nibble of the controller output port.
    pub fn pulse_output_low_nibble(&mut self, data: u8) -> Result<()> {
        // Make the high nibble all 1's
        let command = Command::PulseOutput as u8 | data;
        // Since we did some bit fiddling, we can't use write_command
        self.wait_for_write()?;
        unsafe {
            self.command_register.write(command as u8);
        }
        Ok(())
    }
}
