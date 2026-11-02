//! Build script for `harbor_llama_sys`.
//!
//! Adapted from `llama-cpp-sys-2` 0.1.156 (MIT OR Apache-2.0, Dial AI); see
//! NOTICE. Harbor changes:
//! - builds from the pinned, unmodified llama.cpp tree in
//!   `third_party/llama.cpp` (HARBOR_PIN.json) instead of a crate-bundled copy;
//! - `mtmd` is built by llama.cpp's own CMake target (`LLAMA_BUILD_MTMD=ON`,
//!   i.e. its explicit `add_library(mtmd ...)` source list) instead of a source
//!   glob, and the `vendor-hash` static library it needs is linked;
//! - only the JSON-schema -> GBNF part of `common/` is compiled (no
//!   llama-common, no cpp-httplib, no subprocess);
//! - every option that would download something at build time is pinned OFF;
//! - CUDA / ROCm / OpenCL / MKL / system-ggml / dynamic-backend support is
//!   removed (Harbor does not ship them);
//! - `LLAMA_CPP_TAG` / `LLAMA_CPP_COMMIT` are generated from the pin.

use std::env;
use std::num::NonZeroUsize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr as _;

use cmake::Config;
use glob::glob;

enum WindowsVariant {
    Msvc,
    Other,
}

enum AppleVariant {
    MacOS,
    Other,
}

enum TargetOs {
    Windows(WindowsVariant),
    Apple(AppleVariant),
    Linux,
    Android,
}

macro_rules! debug_log {
    ($($arg:tt)*) => {
        if std::env::var("BUILD_DEBUG").is_ok() {
            println!("cargo:warning=[DEBUG] {}", format!($($arg)*));
        }
    };
}

/// Sources from llama.cpp `common/` that Harbor needs (structured output):
/// JSON Schema -> GBNF and what it links against. Everything else in
/// llama-common is left out.
const COMMON_SOURCES: &[&str] = &[
    "json-schema-to-grammar.cpp",
    "json-schema.cpp",
    "json.cpp",
    "trie.cpp",
    "unicode.cpp",
];

fn emit_compiler_static_archive_search_path(archive: &str) {
    let compiler = cc::Build::new().get_compiler();
    let Ok(output) = Command::new(compiler.path())
        .arg(format!("--print-file-name={archive}"))
        .output()
    else {
        return;
    };

    if !output.status.success() {
        return;
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let path = Path::new(&path);
    if path.is_file() {
        if let Some(parent) = path.parent() {
            println!("cargo:rustc-link-search=native={}", parent.display());
        }
    }
}

fn parse_target_os() -> Result<(TargetOs, String), String> {
    let target = env::var("TARGET").unwrap();

    if target.contains("windows") {
        if target.ends_with("-windows-msvc") {
            Ok((TargetOs::Windows(WindowsVariant::Msvc), target))
        } else {
            Ok((TargetOs::Windows(WindowsVariant::Other), target))
        }
    } else if target.contains("apple") {
        if target.ends_with("-apple-darwin") {
            Ok((TargetOs::Apple(AppleVariant::MacOS), target))
        } else {
            Ok((TargetOs::Apple(AppleVariant::Other), target))
        }
    } else if target.contains("android") {
        Ok((TargetOs::Android, target))
    } else if target.contains("linux") {
        Ok((TargetOs::Linux, target))
    } else {
        Err(target)
    }
}

fn extract_lib_names(out_dir: &Path, target_os: &TargetOs) -> Vec<String> {
    let lib_pattern = match target_os {
        // MSVC emits .lib; the GNU (MinGW) toolchain emits .a static archives.
        TargetOs::Windows(WindowsVariant::Msvc) => "*.lib",
        _ => "*.a",
    };
    let libs_dir = out_dir.join("lib*");
    let pattern = libs_dir.join(lib_pattern);
    debug_log!("Extract libs {}", pattern.display());

    let mut lib_names: Vec<String> = Vec::new();
    for entry in glob(pattern.to_str().unwrap()).unwrap() {
        match entry {
            Ok(path) => {
                let stem = path.file_stem().unwrap();
                let stem_str = stem.to_str().unwrap();

                // Remove the "lib" prefix if present
                let lib_name = if let Some(stripped) = stem_str.strip_prefix("lib") {
                    stripped
                } else {
                    if path.extension() == Some(std::ffi::OsStr::new("a")) {
                        let target = path.parent().unwrap().join(format!("lib{stem_str}.a"));
                        std::fs::rename(&path, &target).unwrap_or_else(|e| {
                            panic!("Failed to rename {path:?} to {target:?}: {e:?}");
                        })
                    }
                    stem_str
                };
                lib_names.push(lib_name.to_string());
            }
            Err(e) => println!("cargo:warning=error={e}"),
        }
    }
    lib_names
}

fn library_file_exists(search_dirs: &[PathBuf], lib_name: &str, target_os: &TargetOs) -> bool {
    let (prefixes, extensions): (&[&str], &[&str]) = match target_os {
        TargetOs::Windows(WindowsVariant::Msvc) => (&["", "lib"], &["lib"]),
        _ => (&["lib"], &["a"]),
    };

    search_dirs.iter().any(|dir| {
        prefixes.iter().any(|prefix| {
            extensions
                .iter()
                .any(|extension| dir.join(format!("{prefix}{lib_name}.{extension}")).is_file())
        })
    })
}

/// Link a static library that CMake builds but does not install (it lives
/// in the build tree under `build/<rel_dir>`, or `build/<rel_dir>/<profile>`
/// for multi-config generators).
fn link_build_tree_lib(
    out_dir: &Path,
    rel_dir: &str,
    lib: &str,
    profile: &str,
    target_os: &TargetOs,
) {
    let dir = out_dir.join("build").join(rel_dir);
    let mut dirs = vec![dir.clone()];
    let profile_dir = dir.join(profile);
    if profile_dir.is_dir() {
        dirs.push(profile_dir);
    }
    assert!(
        library_file_exists(&dirs, lib, target_os),
        "expected the CMake build to produce static library `{lib}` under {}",
        dir.display()
    );
    for d in &dirs {
        println!("cargo:rustc-link-search=native={}", d.display());
    }
    println!("cargo:rustc-link-lib=static={lib}");
}

fn macos_link_search_path() -> Option<String> {
    let output = Command::new("clang")
        .arg("--print-search-dirs")
        .output()
        .ok()?;
    if !output.status.success() {
        println!(
            "failed to run 'clang --print-search-dirs', continuing without a link search path"
        );
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        if line.contains("libraries: =") {
            let path = line.split('=').nth(1)?;
            return Some(format!("{path}/lib/darwin"));
        }
    }

    println!("failed to determine link search path, continuing without it");
    None
}

fn validate_android_ndk(ndk_path: &str) -> Result<(), String> {
    let ndk_path = Path::new(ndk_path);

    if !ndk_path.exists() {
        return Err(format!(
            "Android NDK path does not exist: {}",
            ndk_path.display()
        ));
    }

    let toolchain_file = ndk_path.join("build/cmake/android.toolchain.cmake");
    if !toolchain_file.exists() {
        return Err(format!(
            "Android NDK toolchain file not found: {}\n\
             This indicates an incomplete NDK installation.",
            toolchain_file.display()
        ));
    }

    Ok(())
}

/// Android NDK toolchain facts derived from the environment for a given Rust target
/// triple. Shared by the bindgen setup (clang args) and the Vulkan backend build (CMake
/// cache vars) so the two agree on which NDK, sysroot, arch and API level to use.
struct AndroidToolchain {
    /// NDK root (e.g. the value of `ANDROID_NDK`).
    ndk: String,
    /// `<ndk>/toolchains/llvm/prebuilt/<host_tag>`
    toolchain_path: String,
    /// `<toolchain_path>/sysroot`
    sysroot: String,
    /// NDK sysroot arch directory / triple, e.g. `aarch64-linux-android`.
    arch_triple: &'static str,
    /// CMake `ANDROID_ABI` name, e.g. `arm64-v8a`.
    abi: &'static str,
    /// Android API level (numeric), e.g. `28`.
    api: String,
}

fn android_toolchain(target_triple: &str) -> AndroidToolchain {
    // NDK path: explicit env vars first, then auto-detect the newest NDK under the SDK.
    let ndk = env::var("ANDROID_NDK")
        .or_else(|_| env::var("ANDROID_NDK_ROOT"))
        .or_else(|_| env::var("NDK_ROOT"))
        .or_else(|_| env::var("CARGO_NDK_ANDROID_NDK"))
        .or_else(|_| {
            if let Some(home) = env::home_dir() {
                let android_home = env::var("ANDROID_HOME")
                    .or_else(|_| env::var("ANDROID_SDK_ROOT"))
                    .unwrap_or_else(|_| format!("{}/Android/Sdk", home.display()));
                let ndk_dir = format!("{android_home}/ndk");
                if let Ok(entries) = std::fs::read_dir(&ndk_dir) {
                    let mut versions: Vec<_> = entries
                        .filter_map(|e| e.ok())
                        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                        .filter_map(|e| e.file_name().to_str().map(|s| s.to_string()))
                        .collect();
                    versions.sort();
                    if let Some(latest) = versions.last() {
                        return Ok(format!("{ndk_dir}/{latest}"));
                    }
                }
            }
            Err(env::VarError::NotPresent)
        })
        .unwrap_or_else(|_| {
            panic!(
                "Android NDK not found. Please set one of: ANDROID_NDK, NDK_ROOT, ANDROID_NDK_ROOT\n\
                 Current target: {target_triple}"
            )
        });

    if let Err(e) = validate_android_ndk(&ndk) {
        panic!("{e}");
    }

    // API level: ANDROID_API_LEVEL, else the numeric part of ANDROID_PLATFORM (`android-NN`).
    let api = env::var("ANDROID_API_LEVEL")
        .or_else(|_| env::var("ANDROID_PLATFORM").map(|p| p.replace("android-", "")))
        .or_else(|_| env::var("CARGO_NDK_ANDROID_PLATFORM").map(|p| p.replace("android-", "")))
        .unwrap_or_else(|_| "28".to_string());

    let host_tag = if cfg!(target_os = "macos") {
        "darwin-x86_64"
    } else if cfg!(target_os = "linux") {
        "linux-x86_64"
    } else if cfg!(target_os = "windows") {
        "windows-x86_64"
    } else {
        panic!("Unsupported host platform for Android NDK");
    };

    // Both the sysroot arch triple (used for header/lib paths) and the CMake ABI name.
    let (arch_triple, abi) = if target_triple.contains("aarch64") {
        ("aarch64-linux-android", "arm64-v8a")
    } else if target_triple.contains("armv7") {
        ("arm-linux-androideabi", "armeabi-v7a")
    } else if target_triple.contains("x86_64") {
        ("x86_64-linux-android", "x86_64")
    } else if target_triple.contains("i686") {
        ("i686-linux-android", "x86")
    } else {
        panic!(
            "Unsupported Android target: {target_triple}\n\
             Supported: aarch64-linux-android, armv7-linux-androideabi, \
             i686-linux-android, x86_64-linux-android"
        );
    };

    let toolchain_path = format!("{ndk}/toolchains/llvm/prebuilt/{host_tag}");
    if !Path::new(&toolchain_path).exists() {
        panic!(
            "Android NDK toolchain not found at: {toolchain_path}\n\
             Please ensure you have the correct Android NDK for your platform."
        );
    }
    let sysroot = format!("{toolchain_path}/sysroot");

    for var in [
        "ANDROID_NDK",
        "ANDROID_NDK_ROOT",
        "NDK_ROOT",
        "CARGO_NDK_ANDROID_NDK",
        "ANDROID_PLATFORM",
        "ANDROID_API_LEVEL",
        "CARGO_NDK_ANDROID_PLATFORM",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }

    AndroidToolchain {
        ndk,
        toolchain_path,
        sysroot,
        arch_triple,
        abi,
        api,
    }
}

/// Read one top-level string field from HARBOR_PIN.json (flat, generated by
/// tools/vendor_llama_cpp.py) without pulling a JSON crate into the build.
fn pin_field(pin: &str, key: &str) -> String {
    let needle = format!("\"{key}\":");
    let start = pin
        .find(&needle)
        .unwrap_or_else(|| panic!("HARBOR_PIN.json has no `{key}`"))
        + needle.len();
    let rest = pin[start..].trim_start();
    let rest = rest
        .strip_prefix('"')
        .unwrap_or_else(|| panic!("HARBOR_PIN.json `{key}` is not a string"));
    let end = rest.find('"').expect("unterminated string in HARBOR_PIN.json");
    rest[..end].to_string()
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let (target_os, target_triple) =
        parse_target_os().unwrap_or_else(|t| panic!("Unsupported target {t}"));
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let llama_src = env::var("HARBOR_LLAMA_CPP_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest_dir.join("../../third_party/llama.cpp"));
    let llama_src = llama_src
        .canonicalize()
        .unwrap_or_else(|e| panic!("llama.cpp source tree {}: {e}", llama_src.display()));
    println!("cargo:rerun-if-env-changed=HARBOR_LLAMA_CPP_DIR");

    // Pin -> constants.
    let pin_path = llama_src.join("HARBOR_PIN.json");
    println!("cargo:rerun-if-changed={}", pin_path.display());
    let pin = std::fs::read_to_string(&pin_path)
        .unwrap_or_else(|e| panic!("{}: {e} (run tools/vendor_llama_cpp.py)", pin_path.display()));
    let tag = pin_field(&pin, "tag");
    let commit = pin_field(&pin, "commit");
    assert!(
        commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()),
        "HARBOR_PIN.json commit must be 40 hex chars"
    );
    std::fs::write(
        out_dir.join("pin.rs"),
        format!(
            "/// Upstream llama.cpp release tag this crate is built from.\n\
             pub const LLAMA_CPP_TAG: &str = {tag:?};\n\
             /// Full upstream llama.cpp commit this crate is built from.\n\
             pub const LLAMA_CPP_COMMIT: &str = {commit:?};\n\
             /// `<tag>+<first 10 hex of commit>`, e.g. `v0.8.0+8642541b25`.\n\
             pub const LLAMA_CPP_REVISION: &str = \"{tag}+{short}\";\n\
             /// Runtime identity reported by Harbor: `llama.cpp@<tag>+<commit[..10]>`.\n\
             pub const LLAMA_CPP_RUNTIME: &str = \"llama.cpp@{tag}+{short}\";\n",
            short = &commit[..10],
        ),
    )
    .expect("write pin.rs");

    let profile = env::var("LLAMA_LIB_PROFILE").unwrap_or("Release".to_string());
    let static_crt = env::var("LLAMA_STATIC_CRT")
        .map(|v| v == "1")
        .unwrap_or(false);

    println!("cargo:rerun-if-env-changed=LLAMA_LIB_PROFILE");
    println!("cargo:rerun-if-env-changed=LLAMA_STATIC_CRT");

    debug_log!("TARGET: {}", target_triple);
    debug_log!("LLAMA_SRC: {}", llama_src.display());
    debug_log!("OUT_DIR: {}", out_dir.display());

    // The vendored tree is immutable and hashed by the pin; a change to the
    // pin file is the rebuild trigger for it. Also watch the CMake inputs so
    // a hand edit during development still rebuilds.
    for dir in ["src", "ggml/src", "common", "tools/mtmd", "include", "ggml/include"] {
        println!("cargo:rerun-if-changed={}", llama_src.join(dir).display());
    }

    let cmake_build_parallelism_level =
        match env::var("CMAKE_BUILD_PARALLEL_LEVEL").map(|v| NonZeroUsize::from_str(&v)) {
            Ok(Ok(v)) => v.to_string(),
            _ => std::thread::available_parallelism()
                .expect("failed to load available parallelism")
                .get()
                .to_string(),
        };
    env::set_var("CMAKE_BUILD_PARALLEL_LEVEL", cmake_build_parallelism_level);

    // Bindings
    let mut bindings_builder = bindgen::Builder::default()
        .header(manifest_dir.join("wrapper.h").to_string_lossy())
        .clang_arg(format!("-I{}", llama_src.join("include").display()))
        .clang_arg(format!("-I{}", llama_src.join("ggml/include").display()))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .derive_partialeq(true)
        .allowlist_function("ggml_.*")
        .allowlist_type("ggml_.*")
        .allowlist_function("gguf_.*")
        .allowlist_type("gguf_.*")
        .allowlist_function("llama_.*")
        .allowlist_type("llama_.*")
        .prepend_enum_name(false);

    // The `llama_rs_*` symbols come from `wrapper_common.cpp`, compiled (and
    // its header included from `wrapper.h`) only with the `common` feature.
    if cfg!(feature = "common") {
        bindings_builder = bindings_builder
            .clang_arg("-DLLAMA_RS_BUILD_COMMON")
            .allowlist_function("llama_rs_.*")
            .allowlist_type("llama_rs_.*");
    }

    if cfg!(feature = "mtmd") {
        bindings_builder = bindings_builder
            .header(manifest_dir.join("wrapper_mtmd.h").to_string_lossy())
            .clang_arg(format!("-I{}", llama_src.join("tools/mtmd").display()))
            .allowlist_function("mtmd_.*")
            .allowlist_type("mtmd_.*");
    }

    if matches!(target_os, TargetOs::Android) {
        let tc = android_toolchain(&target_triple);

        // Find clang builtin includes
        let clang_builtin_includes = {
            let clang_lib_path = format!("{}/lib/clang", tc.toolchain_path);
            std::fs::read_dir(&clang_lib_path).ok().and_then(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .find(|entry| {
                        entry.file_type().map(|t| t.is_dir()).unwrap_or(false)
                            && entry
                                .file_name()
                                .to_str()
                                .map(|name| name.chars().next().unwrap_or('0').is_ascii_digit())
                                .unwrap_or(false)
                    })
                    .and_then(|entry| {
                        let include_path =
                            format!("{}/{}/include", clang_lib_path, entry.file_name().to_str()?);
                        if std::path::Path::new(&include_path).exists() {
                            Some(include_path)
                        } else {
                            None
                        }
                    })
            })
        };

        bindings_builder = bindings_builder
            .clang_arg(format!("--sysroot={}", tc.sysroot))
            .clang_arg(format!("-D__ANDROID_API__={}", tc.api))
            .clang_arg("-D__ANDROID__");

        if let Some(ref builtin_includes) = clang_builtin_includes {
            bindings_builder = bindings_builder
                .clang_arg("-isystem")
                .clang_arg(builtin_includes);
        }

        bindings_builder = bindings_builder
            .clang_arg("-isystem")
            .clang_arg(format!("{}/usr/include/{}", tc.sysroot, tc.arch_triple))
            .clang_arg("-isystem")
            .clang_arg(format!("{}/usr/include", tc.sysroot))
            .clang_arg("-include")
            .clang_arg("stdbool.h")
            .clang_arg("-include")
            .clang_arg("stdint.h");

        if env::var("CARGO_SUBCOMMAND").as_deref() == Ok("ndk") {
            std::env::set_var("BINDGEN_EXTRA_CLANG_ARGS", format!("--target={target_triple}"));
        }
    }

    // Fix bindgen header discovery on Windows MSVC: let the cc crate set up
    // the MSVC environment and forward its INCLUDE paths.
    if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
        let dummy_c = out_dir.join("dummy.c");
        std::fs::write(&dummy_c, "int main() { return 0; }").unwrap();
        let mut build = cc::Build::new();
        build.file(&dummy_c);
        let compiler = build.try_get_compiler().unwrap();
        let env_include = compiler
            .env()
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("INCLUDE"))
            .map(|(_, v)| v);
        if let Some(include_paths) = env_include {
            for include_path in include_paths
                .to_string_lossy()
                .split(';')
                .filter(|s| !s.is_empty())
            {
                bindings_builder = bindings_builder
                    .clang_arg("-isystem")
                    .clang_arg(include_path);
            }
        }
        bindings_builder = bindings_builder
            .clang_arg(format!("--target={target_triple}"))
            .clang_arg("-fms-compatibility")
            .clang_arg("-fms-extensions");
    }

    let bindings = bindings_builder
        .generate()
        .expect("Failed to generate bindings");
    bindings
        .write_to_file(out_dir.join("bindings.rs"))
        .expect("Failed to write bindings");

    for f in [
        "wrapper.h",
        "wrapper_common.h",
        "wrapper_common.cpp",
        "wrapper_utils.h",
        "wrapper_mtmd.h",
        "common_shim.cpp",
    ] {
        println!("cargo:rerun-if-changed={}", manifest_dir.join(f).display());
    }

    // JSON Schema -> GBNF (structured output) from llama.cpp common/, plus
    // the exception-safe C shims. Compiled directly: llama-common itself
    // (HTTP download, chat parsers, subprocess, ...) is not built.
    if cfg!(feature = "common") {
        let common = llama_src.join("common");
        let mut common_build = cc::Build::new();
        common_build
            .cpp(true)
            .file(manifest_dir.join("wrapper_common.cpp"))
            .file(manifest_dir.join("common_shim.cpp"))
            .include(&manifest_dir)
            .include(&common)
            .include(llama_src.join("include"))
            .include(llama_src.join("ggml/include"))
            .include(llama_src.join("vendor"))
            .flag_if_supported("-std=c++17")
            .pic(true);
        for f in COMMON_SOURCES {
            common_build.file(common.join(f));
        }

        if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
            common_build.flag("/std:c++17").flag("/utf-8").flag("/bigobj");
        }

        // When static-stdcxx is enabled on Android, suppress the cc crate's automatic
        // C++ stdlib linking (which defaults to c++_shared) so we can link c++_static instead.
        if matches!(target_os, TargetOs::Android) && cfg!(feature = "static-stdcxx") {
            common_build.cpp_link_stdlib(None);
        }

        common_build.compile("harbor_llama_common");
    }

    // Build with CMake
    let mut config = Config::new(&llama_src);

    // Library-only build. Nothing that downloads at build time, spawns
    // processes or serves HTTP is configured.
    for (key, value) in [
        ("LLAMA_BUILD_TESTS", "OFF"),
        ("LLAMA_BUILD_EXAMPLES", "OFF"),
        ("LLAMA_BUILD_SERVER", "OFF"),
        ("LLAMA_BUILD_TOOLS", "OFF"),
        ("LLAMA_BUILD_APP", "OFF"),
        ("LLAMA_BUILD_UI", "OFF"),
        ("LLAMA_USE_PREBUILT_UI", "OFF"),
        ("LLAMA_BUILD_COMMON", "OFF"),
        ("LLAMA_TOOLS_INSTALL", "OFF"),
        ("LLAMA_CONNECT", "OFF"),
        ("LLAMA_SUBPROCESS", "OFF"),
        ("LLAMA_OPENSSL", "OFF"),
        ("LLAMA_LLGUIDANCE", "OFF"),
        ("MTMD_VIDEO", "OFF"),
        ("GGML_OPENMP_FETCH", "OFF"),
        ("GGML_CPU_KLEIDIAI", "OFF"),
    ] {
        config.define(key, value);
    }
    config.define(
        "LLAMA_BUILD_MTMD",
        if cfg!(feature = "mtmd") { "ON" } else { "OFF" },
    );
    // Build identity from the pin instead of `git` in the enclosing (Harbor)
    // checkout, which would report Harbor's HEAD and churn on every commit.
    config.define("LLAMA_BUILD_COMMIT", &commit[..10]);
    config.define("LLAMA_BUILD_NUMBER", "0");

    // Pass CMAKE_ environment variables down to CMake
    for (key, value) in env::vars() {
        if key.starts_with("CMAKE_") {
            config.define(&key, &value);
        }
    }

    // Forward GGML_ environment variables as CMake cache entries, so a
    // downstream can toggle a ggml build option (e.g. `GGML_CPU_REPACK=OFF`).
    // Options set explicitly below win over env values.
    for (key, value) in env::vars() {
        if key.starts_with("GGML_") {
            println!("cargo:rerun-if-env-changed={key}");
            config.define(&key, &value);
        }
    }
    // Re-assert the no-download pins after the env forwarding.
    config.define("GGML_OPENMP_FETCH", "OFF");
    config.define("GGML_CPU_KLEIDIAI", "OFF");

    // extract the target-cpu config value, if specified
    let target_cpu = std::env::var("CARGO_ENCODED_RUSTFLAGS")
        .ok()
        .and_then(|rustflags| {
            rustflags
                .split('\x1f')
                .find(|f| f.contains("target-cpu="))
                .and_then(|f| f.split("target-cpu=").nth(1))
                .map(|s| s.to_string())
        });

    if target_cpu == Some("native".into()) {
        config.define("GGML_NATIVE", "ON");
    } else {
        config.define("GGML_NATIVE", "OFF");

        if let Some(ref cpu) = target_cpu {
            config.cflag(format!("-march={cpu}"));
            config.cxxflag(format!("-march={cpu}"));
        }

        let features = std::env::var("CARGO_CFG_TARGET_FEATURE")
            .expect("Env var CARGO_CFG_TARGET_FEATURE not found.");
        // GGML config flags: llama.cpp/ggml/src/ggml-cpu/CMakeLists.txt
        for feature in features.split(',') {
            let flag = match feature {
                "avx" => "GGML_AVX",
                "avx2" => "GGML_AVX2",
                "avx512bf16" => "GGML_AVX512_BF16",
                "avx512vbmi" => "GGML_AVX512_VBMI",
                "avx512vnni" => "GGML_AVX512_VNNI",
                "avxvnni" => "GGML_AVX_VNNI",
                "bmi2" => "GGML_BMI2",
                "f16c" => "GGML_F16C",
                "fma" => "GGML_FMA",
                "sse4.2" => "GGML_SSE42",
                _ => continue,
            };
            config.define(flag, "ON");
        }
    }

    config.define("BUILD_SHARED_LIBS", "OFF");

    if matches!(target_os, TargetOs::Apple(_)) {
        config.define("GGML_BLAS", "OFF");
    }

    if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc))
        && matches!(
            profile.as_str(),
            "Release" | "RelWithDebInfo" | "MinSizeRel"
        )
    {
        // Debug Rust builds under MSVC turn off optimization even though we
        // build llama.cpp's Release profile (rust-lang/cmake-rs#240).
        for flag in &["/O2", "/DNDEBUG", "/Ob2"] {
            config.cflag(flag);
            config.cxxflag(flag);
        }
    }

    config.static_crt(static_crt);

    if matches!(target_os, TargetOs::Android) {
        if cfg!(feature = "shared-stdcxx") && cfg!(feature = "static-stdcxx") {
            panic!("Features 'shared-stdcxx' and 'static-stdcxx' are mutually exclusive");
        }

        let tc = android_toolchain(&target_triple);

        config.define(
            "CMAKE_TOOLCHAIN_FILE",
            format!("{}/build/cmake/android.toolchain.cmake", tc.ndk),
        );
        config.define("ANDROID_PLATFORM", format!("android-{}", tc.api));
        config.define("ANDROID_ABI", tc.abi);

        if cfg!(feature = "static-stdcxx") {
            config.define("ANDROID_STL", "c++_static");
        } else if cfg!(feature = "shared-stdcxx") {
            config.define("ANDROID_STL", "c++_shared");
        }

        match tc.abi {
            "arm64-v8a" => {
                config.cflag("-march=armv8-a");
                config.cxxflag("-march=armv8-a");
                // Opt-in Arm architecture override for ggml-cpu, e.g.
                // `armv8.2-a+dotprod`; the default (`armv8-a`) is unchanged.
                println!("cargo:rerun-if-env-changed=GGML_CPU_ARM_ARCH");
                if let Ok(arch) = std::env::var("GGML_CPU_ARM_ARCH") {
                    config.define("GGML_CPU_ARM_ARCH", &arch);
                }
            }
            "armeabi-v7a" => {
                for f in ["-march=armv7-a", "-mfpu=neon", "-mthumb"] {
                    config.cflag(f);
                    config.cxxflag(f);
                }
            }
            "x86_64" => {
                config.cflag("-march=x86-64");
                config.cxxflag("-march=x86-64");
            }
            "x86" => {
                config.cflag("-march=i686");
                config.cxxflag("-march=i686");
            }
            _ => {}
        }

        config.define("GGML_LLAMAFILE", "OFF");

        println!("cargo:rustc-link-lib=log");
        println!("cargo:rustc-link-lib=android");
    }

    if matches!(target_os, TargetOs::Linux)
        && target_triple.contains("aarch64")
        && target_cpu != Some("native".into())
    {
        config.define("GGML_NATIVE", "OFF");
        config.define("GGML_CPU_ARM_ARCH", "armv8-a");
    }

    if cfg!(feature = "vulkan") {
        config.define("GGML_VULKAN", "ON");
        match target_os {
            TargetOs::Windows(_) => {
                let vulkan_path = env::var("VULKAN_SDK").expect(
                    "Please install Vulkan SDK and ensure that VULKAN_SDK env variable is set",
                );
                let vulkan_lib_path = Path::new(&vulkan_path).join("Lib");
                println!("cargo:rustc-link-search={}", vulkan_lib_path.display());
                println!("cargo:rustc-link-lib=vulkan-1");

                // MSBuild FileTracker cannot handle the long paths of a nested
                // cargo build; disable it and serialise PDB access with /FS.
                if matches!(target_os, TargetOs::Windows(WindowsVariant::Msvc)) {
                    env::set_var("TrackFileAccess", "false");
                    config.cflag("/FS");
                    config.cxxflag("/FS");
                }
            }
            TargetOs::Linux => {
                if let Ok(vulkan_path) = env::var("VULKAN_SDK") {
                    let vulkan_lib_path = Path::new(&vulkan_path).join("lib");
                    println!("cargo:rustc-link-search={}", vulkan_lib_path.display());
                }
                println!("cargo:rustc-link-lib=vulkan");
            }
            TargetOs::Android => {
                // The NDK ships only the C Vulkan headers and re-roots
                // find_package() into its sysroot, so the caller supplies
                // vulkan.hpp (VULKAN_INCLUDE_DIR), SPIRV-Headers
                // (SPIRV_HEADERS_DIR) and glslc; see scripts/build_android_vulkan.sh.
                let tc = android_toolchain(&target_triple);

                let vk_include = env::var("VULKAN_INCLUDE_DIR").expect(
                    "the Vulkan backend for Android requires VULKAN_INCLUDE_DIR to point at \
                     Vulkan headers that include <vulkan/vulkan.hpp> (the NDK ships only the C \
                     headers)",
                );
                config.define("Vulkan_INCLUDE_DIR", &vk_include);
                config.define(
                    "Vulkan_LIBRARY",
                    format!(
                        "{}/usr/lib/{}/{}/libvulkan.so",
                        tc.sysroot, tc.arch_triple, tc.api
                    ),
                );

                let glslc = env::var("VULKAN_GLSLC")
                    .ok()
                    .or_else(|| {
                        env::var_os("PATH").and_then(|paths| {
                            env::split_paths(&paths)
                                .map(|p| p.join("glslc"))
                                .find(|p| p.exists())
                                .map(|p| p.to_string_lossy().into_owned())
                        })
                    })
                    .expect(
                        "the Vulkan backend for Android requires `glslc` on PATH or VULKAN_GLSLC",
                    );
                config.define("Vulkan_GLSLC_EXECUTABLE", glslc);

                let spirv_dir = env::var("SPIRV_HEADERS_DIR").expect(
                    "the Vulkan backend for Android requires SPIRV_HEADERS_DIR to point at the \
                     directory containing SPIRV-HeadersConfig.cmake",
                );
                config.define("SPIRV-Headers_DIR", spirv_dir);
                let spirv_include = env::var("SPIRV_HEADERS_INCLUDE_DIR").unwrap_or(vk_include);
                config.cflag(format!("-I{spirv_include}"));
                config.cxxflag(format!("-I{spirv_include}"));

                println!("cargo:rustc-link-lib=vulkan");
            }
            _ => (),
        }
        for var in [
            "VULKAN_SDK",
            "VULKAN_INCLUDE_DIR",
            "VULKAN_GLSLC",
            "SPIRV_HEADERS_DIR",
            "SPIRV_HEADERS_INCLUDE_DIR",
        ] {
            println!("cargo:rerun-if-env-changed={var}");
        }
    }

    // Android has no OpenMP runtime Harbor ships; keep it off there.
    if cfg!(feature = "openmp") && !matches!(target_os, TargetOs::Android) {
        config.define("GGML_OPENMP", "ON");
    } else {
        config.define("GGML_OPENMP", "OFF");
    }

    config
        .profile(&profile)
        .very_verbose(std::env::var("CMAKE_VERBOSE").is_ok())
        .always_configure(false);

    let build_dir = config.build();

    // Search paths
    println!("cargo:rustc-link-search={}", out_dir.join("lib").display());
    println!("cargo:rustc-link-search={}", out_dir.join("lib64").display());
    println!("cargo:rustc-link-search={}", build_dir.display());

    let llama_libs = extract_lib_names(&out_dir, &target_os);
    assert_ne!(llama_libs.len(), 0);
    if cfg!(feature = "mtmd") {
        assert!(
            llama_libs.iter().any(|l| l == "mtmd"),
            "LLAMA_BUILD_MTMD did not install libmtmd: {llama_libs:?}"
        );
    }
    for lib in &llama_libs {
        println!("cargo:rustc-link-lib=static={lib}");
    }

    // mtmd hashes media (xxHash / SHA-1 / SHA-256) through the `vendor-hash`
    // static library, which has no install() rule: link it from the build tree.
    if cfg!(feature = "mtmd") {
        link_build_tree_lib(&out_dir, "vendor/hash", "vendor-hash", &profile, &target_os);
    }

    // OpenMP
    if cfg!(feature = "openmp") && target_triple.contains("gnu") {
        if cfg!(feature = "static-openmp") {
            emit_compiler_static_archive_search_path("libgomp.a");
            println!("cargo:rustc-link-lib=static=gomp");
        } else {
            println!("cargo:rustc-link-lib=gomp");
        }
    }

    match target_os {
        TargetOs::Windows(WindowsVariant::Msvc) => {
            println!("cargo:rustc-link-lib=advapi32");
            if profile.eq_ignore_ascii_case("debug") {
                if static_crt {
                    println!("cargo:rustc-link-lib=libcmtd");
                } else {
                    println!("cargo:rustc-link-lib=dylib=msvcrtd");
                }
            }
        }
        TargetOs::Linux => {
            if cfg!(feature = "static-stdcxx") {
                emit_compiler_static_archive_search_path("libstdc++.a");
                println!("cargo:rustc-link-lib=static=stdc++");
            } else {
                println!("cargo:rustc-link-lib=dylib=stdc++");
            }
        }
        TargetOs::Apple(ref variant) => {
            println!("cargo:rustc-link-lib=framework=Foundation");
            println!("cargo:rustc-link-lib=framework=Metal");
            println!("cargo:rustc-link-lib=framework=MetalKit");
            println!("cargo:rustc-link-lib=framework=Accelerate");
            println!("cargo:rustc-link-lib=c++");

            if let AppleVariant::MacOS = variant {
                // Older macOS needs the clang runtime from a non-default path
                // (alexcrichton/curl-rust#279).
                if let Some(path) = macos_link_search_path() {
                    println!("cargo:rustc-link-lib=clang_rt.osx");
                    println!("cargo:rustc-link-search={path}");
                }
            }
        }
        TargetOs::Android => {
            if cfg!(feature = "static-stdcxx") {
                println!("cargo:rustc-link-lib=c++_static");
                println!("cargo:rustc-link-lib=c++abi");
            } else if cfg!(feature = "shared-stdcxx") {
                println!("cargo:rustc-link-lib=c++_shared");
            }
        }
        _ => (),
    }
}
