#![cfg_attr(not(target_os = "windows"), allow(dead_code))]

#[cfg(not(target_os = "windows"))]
compile_error!("dram-rgb only supports Windows.");

use std::env;
use std::ffi::{OsStr, c_void};
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::ptr::{null, null_mut};

type Handle = *mut c_void;

const INVALID_HANDLE_VALUE: Handle = usize::MAX as Handle;
const GENERIC_READ: u32 = 0x8000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;
const OPEN_EXISTING: u32 = 3;
const FILE_ATTRIBUTE_NORMAL: u32 = 0x80;

const IOCTL_READ_SMB_TYPE: u32 = 0x8050_2140;
const IOCTL_READ_SMB_QUICK: u32 = 0x8050_2144;
const IOCTL_READ_SMB_BYTE: u32 = 0x8050_2148;
const IOCTL_WRITE_SMB_BYTE: u32 = 0x8050_214C;
const IOCTL_WRITE_SMB_WORD: u32 = 0x8050_2154;

const ENE_REG_DEVICE_NAME: u16 = 0x1000;
const ENE_REG_CONFIG_TABLE: u16 = 0x1C00;
const ENE_REG_COLORS_DIRECT_V1: u16 = 0x8000;
const ENE_REG_COLORS_DIRECT_V2: u16 = 0x8100;
const ENE_REG_DIRECT: u16 = 0x8020;
const ENE_REG_MODE: u16 = 0x8021;
const ENE_REG_SPEED: u16 = 0x8022;
const ENE_REG_DIRECTION: u16 = 0x8023;
const ENE_REG_APPLY: u16 = 0x80A0;
const ENE_REG_SLOT_INDEX: u16 = 0x80F8;
const ENE_REG_I2C_ADDRESS: u16 = 0x80F9;
const ENE_APPLY: u8 = 0x01;
const ENE_MODE_OFF: u8 = 0x00;
const ENE_CONFIG_LED_COUNT: u16 = 0x02;

const MAX_CONTROLLERS: u8 = 8;
const ENE_ADDRESSES: [u8; 16] = [
    0x70, 0x71, 0x72, 0x73, 0x74, 0x75, 0x76, 0x77, 0x4F, 0x66, 0x67, 0x39, 0x3A, 0x3B, 0x3C, 0x3D,
];

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *const c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: Handle,
    ) -> Handle;

    fn DeviceIoControl(
        device: Handle,
        control_code: u32,
        in_buffer: *const c_void,
        in_buffer_size: u32,
        out_buffer: *mut c_void,
        out_buffer_size: u32,
        bytes_returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;

    fn CloseHandle(object: Handle) -> i32;
}

#[derive(Clone, Copy)]
struct EneDevice {
    controller: u8,
    address: u8,
}

struct EneProfile {
    name: String,
    led_count: u8,
    direct_register: u16,
}

struct CtiIo {
    handle: Handle,
}

impl CtiIo {
    fn open() -> io::Result<Self> {
        let path = wide(r"\\.\CTIIO");
        let handle = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                0,
                null(),
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self { handle })
        }
    }

    fn ioctl(&self, code: u32, input: &[u8; 12], output: Option<&mut [u8; 8]>) -> io::Result<()> {
        let (output_ptr, output_len) = match output {
            Some(buffer) => (buffer.as_mut_ptr().cast::<c_void>(), buffer.len() as u32),
            None => (null_mut(), 0),
        };
        let mut bytes_returned = 0u32;
        let ok = unsafe {
            DeviceIoControl(
                self.handle,
                code,
                input.as_ptr().cast::<c_void>(),
                input.len() as u32,
                output_ptr,
                output_len,
                &mut bytes_returned,
                null_mut(),
            )
        };

        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn smbus_type(&self) -> io::Result<u8> {
        let input = [0u8; 12];
        let mut output = [0u8; 8];
        self.ioctl(IOCTL_READ_SMB_TYPE, &input, Some(&mut output))?;
        Ok(output[0])
    }

    fn quick(&self, controller: u8, address: u8) -> bool {
        let input = packet(controller, address, 0, 0);
        self.ioctl(IOCTL_READ_SMB_QUICK, &input, None).is_ok()
    }

    fn read_byte(&self, controller: u8, address: u8, command: u8) -> io::Result<u8> {
        let input = packet(controller, address, command, 0);
        let mut output = [0u8; 8];
        self.ioctl(IOCTL_READ_SMB_BYTE, &input, Some(&mut output))?;
        Ok(output[0])
    }

    fn write_byte(&self, controller: u8, address: u8, command: u8, value: u8) -> io::Result<()> {
        let input = packet(controller, address, command, value as u64);
        self.ioctl(IOCTL_WRITE_SMB_BYTE, &input, None)
    }

    fn write_word(&self, controller: u8, address: u8, command: u8, value: u16) -> io::Result<()> {
        let input = packet(controller, address, command, value as u64);
        self.ioctl(IOCTL_WRITE_SMB_WORD, &input, None)
    }

    fn ene_read(&self, device: EneDevice, register: u16) -> io::Result<u8> {
        // CTIIO serializes WRITE_SMB_WORD in ENE register order; do not byte-swap.
        self.write_word(device.controller, device.address, 0x00, register)?;
        self.read_byte(device.controller, device.address, 0x81)
    }

    fn ene_write(&self, device: EneDevice, register: u16, value: u8) -> io::Result<()> {
        // CTIIO serializes WRITE_SMB_WORD in ENE register order; do not byte-swap.
        self.write_word(device.controller, device.address, 0x00, register)?;
        self.write_byte(device.controller, device.address, 0x01, value)
    }
}

impl Drop for CtiIo {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

fn wide(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(Some(0)).collect()
}

fn packet(controller: u8, address: u8, command: u8, value: u64) -> [u8; 12] {
    let mut packet = [0u8; 12];
    packet[0] = controller;
    packet[1] = address;
    packet[2] = command;
    packet[3..11].copy_from_slice(&value.to_le_bytes());
    packet
}

fn is_ene_device(driver: &CtiIo, controller: u8, address: u8) -> bool {
    for command in 0xA0u8..=0xAF {
        match driver.read_byte(controller, address, command) {
            Ok(value) if value == command - 0xA0 => {}
            _ => return false,
        }
    }
    true
}

fn scan(driver: &CtiIo) -> Vec<EneDevice> {
    let mut devices = Vec::new();
    for controller in 0..MAX_CONTROLLERS {
        for address in ENE_ADDRESSES {
            if is_ene_device(driver, controller, address) {
                devices.push(EneDevice {
                    controller,
                    address,
                });
            }
        }
    }
    devices
}

fn device_profile(driver: &CtiIo, device: EneDevice) -> Result<EneProfile, String> {
    let mut name_bytes = [0u8; 16];
    for (offset, byte) in name_bytes.iter_mut().enumerate() {
        *byte = driver
            .ene_read(device, ENE_REG_DEVICE_NAME + offset as u16)
            .map_err(|error| {
                format!(
                    "ctl={} addr=0x{:02X}: failed to read device name: {error}",
                    device.controller, device.address
                )
            })?;
    }

    let name_length = name_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(name_bytes.len());
    let name = String::from_utf8_lossy(&name_bytes[..name_length]).into_owned();

    let (direct_register, maximum_led_count) = match name.as_str() {
        "DIMM_LED-0102" => (ENE_REG_COLORS_DIRECT_V1, 5),
        "AUDA0-E6K5-0101" => (ENE_REG_COLORS_DIRECT_V2, 10),
        _ => {
            return Err(format!(
                "ctl={} addr=0x{:02X}: unsupported ENE DRAM controller version: {name:?}",
                device.controller, device.address
            ));
        }
    };

    let led_count = driver
        .ene_read(device, ENE_REG_CONFIG_TABLE + ENE_CONFIG_LED_COUNT)
        .map_err(|error| {
            format!(
                "ctl={} addr=0x{:02X}: failed to read LED count: {error}",
                device.controller, device.address
            )
        })?;
    if led_count == 0 || led_count > maximum_led_count {
        return Err(format!(
            "ctl={} addr=0x{:02X}: invalid LED count {led_count} for {name}",
            device.controller, device.address
        ));
    }

    Ok(EneProfile {
        name,
        led_count,
        direct_register,
    })
}

fn remap_unassigned_modules(driver: &CtiIo) -> Result<usize, String> {
    let mut remapped = 0usize;

    for controller in 0..MAX_CONTROLLERS {
        if !is_ene_device(driver, controller, 0x77) {
            continue;
        }

        for slot in 0u8..8 {
            if !is_ene_device(driver, controller, 0x77) {
                break;
            }

            let target = ENE_ADDRESSES
                .iter()
                .copied()
                .find(|address| !driver.quick(controller, *address))
                .ok_or_else(|| format!("controller {controller}: no free ENE SMBus address"))?;

            let unassigned = EneDevice {
                controller,
                address: 0x77,
            };
            driver
                .ene_write(unassigned, ENE_REG_SLOT_INDEX, slot)
                .map_err(|error| {
                    format!("controller {controller}: failed to select DIMM slot {slot}: {error}")
                })?;
            driver
                .ene_write(unassigned, ENE_REG_I2C_ADDRESS, target << 1)
                .map_err(|error| {
                    format!(
                        "controller {controller}: failed to map slot {slot} to 0x{target:02X}: \
                         {error}"
                    )
                })?;

            if !is_ene_device(driver, controller, target) {
                return Err(format!(
                    "controller {controller}: slot {slot} did not respond at 0x{target:02X}"
                ));
            }
            remapped += 1;
        }
    }

    Ok(remapped)
}

fn turn_off(driver: &CtiIo, device: EneDevice) -> Result<(), String> {
    let writes = [
        (ENE_REG_MODE, ENE_MODE_OFF),
        (ENE_REG_SPEED, 0),
        (ENE_REG_DIRECTION, 0),
        (ENE_REG_APPLY, ENE_APPLY),
        (ENE_REG_DIRECT, 0),
        (ENE_REG_APPLY, ENE_APPLY),
    ];

    for (register, value) in writes {
        driver.ene_write(device, register, value).map_err(|error| {
            format!(
                "ctl={} addr=0x{:02X}: write 0x{register:04X} failed: {error}",
                device.controller, device.address
            )
        })?;
    }

    let mode = driver.ene_read(device, ENE_REG_MODE).map_err(|error| {
        format!(
            "ctl={} addr=0x{:02X}: verify mode failed: {error}",
            device.controller, device.address
        )
    })?;
    let direct = driver.ene_read(device, ENE_REG_DIRECT).map_err(|error| {
        format!(
            "ctl={} addr=0x{:02X}: verify direct mode failed: {error}",
            device.controller, device.address
        )
    })?;

    if mode != 0 || direct != 0 {
        return Err(format!(
            "ctl={} addr=0x{:02X}: verification failed (mode={mode}, direct={direct})",
            device.controller, device.address
        ));
    }
    Ok(())
}

fn set_solid_color(
    driver: &CtiIo,
    device: EneDevice,
    red: u8,
    green: u8,
    blue: u8,
) -> Result<EneProfile, String> {
    let profile = device_profile(driver, device)?;
    let color = [red, blue, green];

    for led in 0..profile.led_count {
        let register = profile.direct_register + u16::from(led) * 3;
        for (offset, value) in color.into_iter().enumerate() {
            driver
                .ene_write(device, register + offset as u16, value)
                .map_err(|error| {
                    format!(
                        "ctl={} addr=0x{:02X}: failed to write LED {led}: {error}",
                        device.controller, device.address
                    )
                })?;
        }
    }

    driver
        .ene_write(device, ENE_REG_DIRECT, 1)
        .and_then(|()| driver.ene_write(device, ENE_REG_APPLY, ENE_APPLY))
        .map_err(|error| {
            format!(
                "ctl={} addr=0x{:02X}: failed to enable direct mode: {error}",
                device.controller, device.address
            )
        })?;

    let direct = driver.ene_read(device, ENE_REG_DIRECT).map_err(|error| {
        format!(
            "ctl={} addr=0x{:02X}: failed to verify direct mode: {error}",
            device.controller, device.address
        )
    })?;
    if direct != 1 {
        return Err(format!(
            "ctl={} addr=0x{:02X}: direct mode verification failed ({direct})",
            device.controller, device.address
        ));
    }

    for led in 0..profile.led_count {
        let register = profile.direct_register + u16::from(led) * 3;
        for (offset, expected) in color.into_iter().enumerate() {
            let actual = driver
                .ene_read(device, register + offset as u16)
                .map_err(|error| {
                    format!(
                        "ctl={} addr=0x{:02X}: failed to verify LED {led}: {error}",
                        device.controller, device.address
                    )
                })?;
            if actual != expected {
                return Err(format!(
                    "ctl={} addr=0x{:02X}: LED {led} verification failed at \
                     0x{:04X} (expected {expected}, got {actual})",
                    device.controller,
                    device.address,
                    register + offset as u16
                ));
            }
        }
    }

    Ok(profile)
}

fn parse_color_component(value: Option<String>, name: &str) -> Result<u8, String> {
    value
        .ok_or_else(|| format!("missing {name} value"))?
        .parse::<u8>()
        .map_err(|_| format!("{name} must be an integer from 0 to 255"))
}

fn open_driver() -> Result<CtiIo, String> {
    CtiIo::open().map_err(|error| {
        format!(
            "cannot open CTIIO driver: {error}\n\
             If CTIIO is not installed, run install-driver.ps1 as Administrator.\n\
             If it is installed, close other RGB software and retry."
        )
    })
}

fn print_help() {
    println!(
        "dram-rgb - direct ENE DRAM RGB control through CTIIO\n\n\
         Usage:\n\
           dram-rgb list          Detect ENE DRAM modules (read-only)\n\
           dram-rgb off           Turn off all detected ENE DRAM LEDs\n\
           dram-rgb set R G B     Set a solid RGB color (0-255 per component)\n\
           dram-rgb driver-status Open CTIIO and show its SMBus type"
    );
}

fn run() -> Result<(), String> {
    let mut arguments = env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "help".to_owned());

    match command.as_str() {
        "list" => {
            let driver = open_driver()?;
            let devices = scan(&driver);
            if devices.is_empty() {
                println!("No ENE DRAM controller detected.");
            } else {
                for (index, device) in devices.iter().enumerate() {
                    println!(
                        "[{}] ENE DRAM: ctl={} SMBus=0x{:02X}",
                        index, device.controller, device.address
                    );
                }
            }
            Ok(())
        }
        "off" => {
            let driver = open_driver()?;
            let remapped = remap_unassigned_modules(&driver)?;
            if remapped != 0 {
                println!("Remapped {remapped} unassigned ENE DRAM module(s).");
            }

            let devices = scan(&driver);
            if devices.is_empty() {
                return Err("no ENE DRAM controller detected".to_owned());
            }

            for device in &devices {
                turn_off(&driver, *device)?;
                println!(
                    "Off: ENE DRAM ctl={} SMBus=0x{:02X}",
                    device.controller, device.address
                );
            }
            println!("Done: {} module(s) verified off.", devices.len());
            Ok(())
        }
        "set" => {
            let red = parse_color_component(arguments.next(), "red")?;
            let green = parse_color_component(arguments.next(), "green")?;
            let blue = parse_color_component(arguments.next(), "blue")?;
            if let Some(extra) = arguments.next() {
                return Err(format!("unexpected argument: {extra}"));
            }

            let driver = open_driver()?;
            let remapped = remap_unassigned_modules(&driver)?;
            if remapped != 0 {
                println!("Remapped {remapped} unassigned ENE DRAM module(s).");
            }

            let devices = scan(&driver);
            if devices.is_empty() {
                return Err("no ENE DRAM controller detected".to_owned());
            }

            for device in &devices {
                let profile = set_solid_color(&driver, *device, red, green, blue)?;
                println!(
                    "Set: {} ctl={} SMBus=0x{:02X} LEDs={} RGB=({red},{green},{blue})",
                    profile.name, device.controller, device.address, profile.led_count
                );
            }
            println!("Done: {} module(s) verified.", devices.len());
            Ok(())
        }
        "driver-status" => {
            let driver = open_driver()?;
            let smbus_type = driver
                .smbus_type()
                .map_err(|error| format!("failed to query SMBus type: {error}"))?;
            println!("CTIIO is ready; SMBus type=0x{smbus_type:02X}");
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        _ => {
            print_help();
            Err(format!("unknown command: {command}"))
        }
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
