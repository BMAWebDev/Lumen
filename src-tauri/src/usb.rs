//! Lists the game controllers plugged into the PC's USB ports (DualSense, DualShock, Xbox, other gamepads).
//!
//! It reads the present Plug and Play devices whose id starts with `USB\VID_`, skipping the per-interface
//! children (`&MI_`) and the hubs, keeps those that are game controllers, and uses the name the device reports about itself ("DualSense Wireless
//! Controller") instead of the generic Windows one ("USB Composite Device").

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UsbDevice {
    /// The Windows instance id.
    pub id: String,
    pub name: String,
}

const SCRIPT: &str = r#"
$ErrorActionPreference = 'SilentlyContinue'
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
# Controllers are recognised by the game-pad / joystick usage their HID interface declares (a hardware id,
# so it does not depend on Windows' language), plus Xbox controllers, which do not use HID.
$pads = @{}
foreach ($h in (Get-CimInstance Win32_PnPEntity | Where-Object { $_.PNPDeviceID -like 'HID\VID_*' })) {
  $ids = @($h.HardwareID)
  if (($ids -contains 'HID_DEVICE_SYSTEM_GAME') -or ($ids -contains 'HID_DEVICE_UP:0001_U:0004')) {
    if ($h.PNPDeviceID -match 'VID_([0-9A-F]{4})&PID_([0-9A-F]{4})') { $pads["$($Matches[1])&$($Matches[2])"] = $true }
  }
}
$dev = @(Get-CimInstance Win32_PnPEntity | Where-Object {
  $_.PNPDeviceID -like 'USB\VID_*' -and $_.PNPDeviceID -notlike '*&MI_*' -and $_.Name -notmatch 'Hub' -and
  (($_.PNPDeviceID -match 'VID_([0-9A-F]{4})&PID_([0-9A-F]{4})' -and $pads.ContainsKey("$($Matches[1])&$($Matches[2])")) -or $_.Name -match 'Xbox|XInput|Gamepad')
})
$key = 'DEVPKEY_Device_BusReportedDeviceDesc'
$desc = @{}
if ($dev.Count -gt 0) {
  # One call for all devices is fast but skips some; those get a call of their own.
  foreach ($p in (Get-PnpDeviceProperty -InstanceId ($dev | ForEach-Object { $_.PNPDeviceID }) -KeyName $key)) { if ($p.Data) { $desc[$p.InstanceId] = $p.Data } }
  foreach ($d in $dev) {
    if (-not $desc.ContainsKey($d.PNPDeviceID)) {
      $p = Get-PnpDeviceProperty -InstanceId $d.PNPDeviceID -KeyName $key
      if ($p.Data) { $desc[$d.PNPDeviceID] = $p.Data }
    }
  }
}
$rows = @($dev | ForEach-Object {
  $n = $desc[$_.PNPDeviceID]
  if (-not $n) { $n = $_.Name }
  [pscustomobject]@{ id = $_.PNPDeviceID; name = [string]$n }
})
[Console]::Out.Write('OK:' + (ConvertTo-Json -InputObject $rows -Compress))
"#;

/// Parses the script's JSON: hubs and controller hubs dropped, sorted by name.
fn parse(json: &str) -> Vec<UsbDevice> {
    #[derive(serde::Deserialize)]
    struct Row {
        id: String,
        name: String,
    }
    let rows: Vec<Row> = serde_json::from_str(json).unwrap_or_default();
    let mut v: Vec<UsbDevice> = rows
        .into_iter()
        .filter(|r| !r.name.trim().is_empty() && !r.name.to_lowercase().contains("hub"))
        .map(|r| UsbDevice { id: r.id, name: r.name.trim().to_owned() })
        .collect();
    v.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then_with(|| a.id.cmp(&b.id)));
    v
}

/// The USB devices connected right now.
pub fn connected() -> Result<Vec<UsbDevice>, String> {
    let out = crate::platform::powershell_timeout(SCRIPT, 40)?;
    match out.trim().strip_prefix("OK:") {
        Some(json) => Ok(parse(json)),
        None => Err(crate::t!("The USB devices could not be listed.", "Dispozitivele USB nu au putut fi listate.").into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hubs_and_nameless_rows_are_dropped_and_the_rest_sorted() {
        let json = r#"[{"id":"2","name":"Zeta Mouse"},{"id":"1","name":"USB4206 Smart Hub"},{"id":"3","name":" "},{"id":"4","name":"alpha Pad"}]"#;
        let names: Vec<String> = parse(json).into_iter().map(|d| d.name).collect();
        assert_eq!(names, ["alpha Pad", "Zeta Mouse"]);
    }

    #[test]
    fn a_single_row_still_parses() {
        // ConvertTo-Json with -InputObject keeps a one-element array, but be lenient if it ever prints an object.
        assert_eq!(parse(r#"[{"id":"a","name":"Pad"}]"#).len(), 1);
        assert!(parse("not json").is_empty());
    }

    #[test]
    #[ignore]
    fn live() {
        println!("{:#?}", connected());
    }
}
