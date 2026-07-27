#[cfg(target_os = "windows")]
use std::env;
#[cfg(target_os = "windows")]
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::fs;
#[cfg(target_os = "windows")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=MdiLedOn.ico");

    #[cfg(target_os = "windows")]
    embed_windows_icon();
}

#[cfg(target_os = "windows")]
fn embed_windows_icon() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let icon_path = manifest_dir.join("MdiLedOn.ico");
    let resource_script = out_dir.join("dram-rgb.rc");
    let resource_file = out_dir.join("dram-rgb.res");

    let icon_path = icon_path.to_string_lossy().replace('\\', "/");
    fs::write(&resource_script, format!("1 ICON \"{icon_path}\"\n"))
        .expect("failed to write Windows resource script");

    let resource_compiler =
        find_resource_compiler().expect("could not find rc.exe from the Windows SDK");
    let status = Command::new(&resource_compiler)
        .arg("/nologo")
        .arg(format!("/fo{}", resource_file.display()))
        .arg(&resource_script)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to run Windows resource compiler {}: {error}",
                resource_compiler.display()
            )
        });
    assert!(status.success(), "Windows resource compilation failed");

    println!(
        "cargo:rustc-link-arg-bin=dram-rgb={}",
        resource_file.display()
    );
}

#[cfg(target_os = "windows")]
fn find_resource_compiler() -> Option<PathBuf> {
    if let Some(path) = env::var_os("RC") {
        return Some(PathBuf::from(path));
    }

    let sdk_arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86") => "x86",
        Ok("aarch64") => "arm64",
        _ => "x64",
    };

    for variable in ["WindowsSdkVerBinPath", "WindowsSdkBinPath"] {
        if let Some(path) = env::var_os(variable) {
            let path = PathBuf::from(path);
            for candidate in [path.join(sdk_arch).join("rc.exe"), path.join("rc.exe")] {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
        }
    }

    let program_files = env::var_os("ProgramFiles(x86)")
        .or_else(|| env::var_os("ProgramFiles"))
        .map(PathBuf::from)?;
    let sdk_bin = program_files.join("Windows Kits").join("10").join("bin");
    newest_sdk_tool(&sdk_bin, sdk_arch, "rc.exe")
}

#[cfg(target_os = "windows")]
fn newest_sdk_tool(sdk_bin: &Path, arch: &str, tool: &str) -> Option<PathBuf> {
    let mut versions: Vec<OsString> = fs::read_dir(sdk_bin)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name())
        .collect();
    versions.sort_unstable();

    versions.into_iter().rev().find_map(|version| {
        let candidate = sdk_bin.join(version).join(arch).join(tool);
        candidate.is_file().then_some(candidate)
    })
}
