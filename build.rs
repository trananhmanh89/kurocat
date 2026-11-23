//! Build script: embeds the app icon, version info and the Windows manifest into KuroCat.exe.
//!
//! This needs a resource compiler: `windres` (MinGW binutils) for the GNU toolchain or `rc.exe`
//! (Windows SDK) for MSVC. Without one the build still succeeds, but the exe has no icon and
//! no manifest, so the program must set DPI awareness itself at start-up.

/// Compiles `assets/app.rc` into the executable, or prints a warning if no resource compiler exists.
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/icons/icon.ico");

    match embed_resource::compile("assets/app.rc", embed_resource::NONE) {
        embed_resource::CompilationResult::Ok | embed_resource::CompilationResult::NotWindows => {}
        embed_resource::CompilationResult::NotAttempted(reason) => {
            println!(
                "cargo:warning=No resource compiler found ({}); run scripts/set-exe-icon.ps1 after the build to add the icon.",
                reason
            );
        }
        embed_resource::CompilationResult::Failed(reason) => {
            panic!("Compiling assets/app.rc failed: {}", reason);
        }
    }
}
