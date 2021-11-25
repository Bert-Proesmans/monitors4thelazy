use std::{error::Error, ffi::OsString, os::windows::prelude::OsStringExt};

use windows::Win32::{
    Devices::Display::{
        GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR, SetVCPFeature,
        PHYSICAL_MONITOR,
    },
    Foundation::*,
    Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR},
    UI::WindowsAndMessaging::SendMessageA,
};

struct MonitorData {
    handle: HANDLE,
    description: OsString,
}

unsafe extern "system" fn iterate_monitors(
    handle_monitor: HMONITOR,
    _handle_device_context: HDC,
    _rectangle_of_interest: *mut RECT,
    application_data: LPARAM,
) -> BOOL {
    let monitor_data = &mut *(application_data.0 as *mut Vec<MonitorData>);

    match impl_iterate_monitors(handle_monitor) {
        Ok(mut data) => monitor_data.append(&mut data),
        Err(_) => unimplemented!(),
    }

    true.into()
}

fn impl_iterate_monitors(
    handle_monitor: HMONITOR,
) -> Result<Vec<MonitorData>, windows::core::Error> {
    let mut monitor_count = 0;
    unsafe {
        let fn_result = GetNumberOfPhysicalMonitorsFromHMONITOR(handle_monitor, &mut monitor_count);
        BOOL(fn_result).ok()?;
    }

    if monitor_count == 0 {
        return Ok(vec![]);
    }

    let mut physical_monitors: Vec<PHYSICAL_MONITOR> = Vec::with_capacity(monitor_count as usize);
    unsafe {
        let fn_result = GetPhysicalMonitorsFromHMONITOR(
            handle_monitor,
            monitor_count,
            physical_monitors.as_mut_ptr(),
        );
        BOOL(fn_result).ok()?;
        physical_monitors.set_len(monitor_count as usize);
    };

    let result = physical_monitors
        .into_iter()
        .map(|monitor| {
            // WARN; References into packed struct are not guaranteed to be aligned, so our slice could technically be garbled.
            // This is solved by creating an aligned copy on the stack (curly braces 'operator') and creating an aligned reference to that memory
            let aligned_copy = { monitor.szPhysicalMonitorDescription };
            MonitorData {
                handle: monitor.hPhysicalMonitor,
                description: read_to_string(&aligned_copy),
            }
        })
        .collect();
    Ok(result)
}

// WARN; OsString doesn't handle null-termination!
// We actually want a mix of OsString and CString behaviour!
fn read_to_string(slice: &[u16]) -> OsString {
    let mut length = 0usize;
    for value in slice {
        if *value == 0 {
            break;
        }
        length += 1;
    }
    OsString::from_wide(&slice[0..length])
}

enum MonitorPowerCommand {
    ON,
    OFF,
}

fn monitor_power_switch(
    monitor: &MonitorData,
    power_command: MonitorPowerCommand,
) -> Result<(), windows::core::Error> {
    // NOTE; Consult "VESA Monitor Control Command Set" standard
    const POWER_MODE: u8 = 0xD6;

    #[repr(u16)]
    enum VesaPowerArgs {
        Ignored = 0x00,
        On = 0x01,
        Standby = 0x02,
        Suspend = 0x03,
        Off = 0x04,
        PowerOff = 0x05,
    }

    let run = |power_value| unsafe {
        let fn_result = SetVCPFeature(monitor.handle, POWER_MODE, power_value);
        BOOL(fn_result).ok()
    };

    match power_command {
        MonitorPowerCommand::ON => run(VesaPowerArgs::On as u32),
        MonitorPowerCommand::OFF => {
            let one= run(VesaPowerArgs::Off as u32);
            let two = run(VesaPowerArgs::PowerOff as u32);
            two
        }
    }
}

fn monitors_power_save() -> Result<(), windows::core::Error> {
    const BROADCAST: HWND = HWND(-1);
    const WM_SYS_COMMAND: u32 = 0x0112;
    const SYS_COMMAND_OK: LRESULT = LRESULT(0);
    const SYS_COMMAND_MONITORPOWER: WPARAM = WPARAM(0xF170);
    
    #[repr(isize)]
    enum MonitorPowerArgs {
        PoweringOn = -1,
        IntoLowPower = 1,
        PowerOff = 2,
    }

    unsafe {
        let _fn_result = SendMessageA(
            BROADCAST,
            WM_SYS_COMMAND,
            SYS_COMMAND_MONITORPOWER,
            LPARAM(MonitorPowerArgs::PowerOff as isize),
        );
        // BOOL::from(fn_result == SYS_COMMAND_OK).ok()
        Ok(())
    }
}

fn main() -> Result<(), windows::core::Error> {
    let mut monitor_data: Vec<MonitorData> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            None,
            std::ptr::null(),
            Some(iterate_monitors),
            LPARAM(&mut monitor_data as *mut _ as isize),
        )
        .ok()?;
    }

    for (idx, data) in monitor_data.iter().enumerate() {
        println!("Monitor {0}: {1:?} -> OFF", idx, data.description);
        monitor_power_switch(data, MonitorPowerCommand::OFF)?;
    }
    //monitors_power_save()?;

    // std::thread::sleep(std::time::Duration::from_secs(10));

    // for (idx, data) in monitor_data.iter().enumerate() {
    //     println!("Monitor {0}: {1:?} -> ON", idx, data.description);
    //     monitor_power_switch(data, MonitorPowerCommand::ON)?;
    // }

    Ok(())
}
