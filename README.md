# dram-rgb

A small Windows x64 command-line tool that turns off ENE-controlled DRAM
lighting by communicating directly with the ENE/CTIIO kernel driver.

Runtime path:

```text
dram-rgb.exe -> \\.\CTIIO -> SMBus -> ENE DRAM RGB controller
```

The Rust code communicates directly with the CTIIO driver and has no
third-party crate dependencies.

## Usage

Detect supported modules without changing their lighting:

```powershell
.\target\release\dram-rgb.exe list
```

Turn off every detected ENE DRAM module:

```powershell
.\target\release\dram-rgb.exe off
```

Turn the modules off now and register the same command to run automatically
after the current user signs in to Windows:

```powershell
.\target\release\dram-rgb.exe off --startup
```

The startup entry points to the executable's current absolute path. Move the
executable first if it will be installed somewhere else. The entry is stored
for the current user under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run` and does not require
administrator privileges.

Set every detected module to a solid RGB color using decimal values from 0 to
255:

```powershell
.\target\release\dram-rgb.exe set 255 0 0
```

Add `--startup` to apply the color now and again after Windows sign-in:

```powershell
.\target\release\dram-rgb.exe set 255 0 0 --startup
```

There is one `dram-rgb` startup entry per user, so the most recent successful
`off --startup` or `set R G B --startup` command replaces the previous startup
action.

Check whether CTIIO is available:

```powershell
.\target\release\dram-rgb.exe driver-status
```

The program opens CTIIO exclusively. If another RGB application is using the
driver, the command fails instead of writing to SMBus concurrently.

## Driver

`vendor\ene\CtiIo64.sys` is the CTIIO x64 kernel driver used by ENE RGB
software. This project calls its user-mode device interface directly and does
not use the ENE HAL DLL.

If CTIIO is already installed, no installation step is required. On a system
without CTIIO, run the following command as Administrator:

```powershell
.\install-driver.ps1
```

The installer accepts only the bundled driver with the documented SHA-256 and
a valid Authenticode signature. It refuses to overwrite an existing CTIIO
service or driver with a different version.

The ENE/CTIIO binary is not an open-source component and is not covered by the
source-code license. Confirm the applicable ENE/CTI redistribution terms before
publishing the repository or distributing binary packages. See
`THIRD_PARTY_NOTICES.md`.

## Build

```powershell
cargo build --release
```

Output:

```text
target\release\dram-rgb.exe
```

## ENE off sequence

The program first identifies devices using the ENE signature registers
`0xA0..0xAF = 0x00..0x0F`. It then writes:

```text
0x8021 MODE      = 0
0x8022 SPEED     = 0
0x8023 DIRECTION = 0
0x80A0 APPLY     = 1
0x8020 DIRECT    = 0
0x80A0 APPLY     = 1
```

It reports success only after reading `MODE` and `DIRECT` back as zero. The
command does not save the state to the controller's persistent configuration.

## Raw effect controls

Raw effect control is planned but is not exposed as a command yet. ENE uses
these registers:

| Register | Purpose | Values |
| --- | --- | --- |
| `0x8021` | Mode | `0..14` |
| `0x8022` | Speed | `0` is fastest, `4` is slowest |
| `0x8023` | Direction | `0` is forward, `1` is reverse |
| `0x80A0` | Apply | Write `1` to apply changes |

Mode values:

| Value | Mode |
| ---: | --- |
| `0` | Off |
| `1` | Static |
| `2` | Breathing |
| `3` | Flashing |
| `4` | Spectrum Cycle |
| `5` | Rainbow |
| `6` | Spectrum Cycle Breathing |
| `7` | Chase Fade |
| `8` | Spectrum Cycle Chase Fade |
| `9` | Chase |
| `10` | Spectrum Cycle Chase |
| `11` | Spectrum Cycle Wave |
| `12` | Chase Rainbow Pulse |
| `13` | Random Flicker |
| `14` | Double Fade; supported by some controllers only |

Speed values:

| Value | Speed |
| ---: | --- |
| `0` | Fastest |
| `1` | Fast |
| `2` | Normal |
| `3` | Slow |
| `4` | Slowest |

The existing `set R G B` command uses direct color mode instead of these effect
registers. It writes each LED color, sets `DIRECT` (`0x8020`) to `1`, and
applies the change through `0x80A0`.
