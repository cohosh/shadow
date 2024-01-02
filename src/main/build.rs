use std::{env, path::PathBuf};

use shadow_build_common::{CBindgenExt, ShadowBuildCommon};

fn run_cbindgen(build_common: &ShadowBuildCommon) {
    let base_config = {
        let mut c = build_common.cbindgen_base_config();
        c.export.exclude.extend_from_slice(&[
            // Avoid re-exporting C types
            "LogLevel".into(),
            "SysCallCondition".into(),
            "Packet".into(),
            "Process".into(),
            "EmulatedTime".into(),
            "SimulationTime".into(),
            "StatusListener".into(),
            "NetworkInterface".into(),
            "Tsc".into(),
            // We have a rust `Epoll` and a C `Epoll`, so don't expose the rust `Epoll` back to C or
            // else the compiler gets confused
            "Epoll".into(),
            // We define manually with varargs
            "thread_nativeSyscall".into(),
        ]);
        c.add_opaque_types(&["RootedRefCell_StateEventSource", "InetSocketWeak"]);
        c
    };

    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    // We currently have circular dependencies between C headers and function
    // declarations in Rust code. If we try to generate only a single Rust
    // binding file, we end up with circular includes since the generated function declarations
    // need to reference types defined in C headers, and those headers end up needing to
    // include the bindings header to reference Rust types.
    //
    // We resolve this by splitting the bindings into 2 headers:
    // * bindings.h exports only function definitions, and opaque struct
    // definitions (which are legal to appear multiple times in a compilation
    // unit.)
    // * bindings-opaque.h exports everything *except* function definitions, allowing
    // it to not need to include any of the C headers.
    //
    // i.e. C headers in this project can include bindings-opaque.h and be guaranteed that
    // there will be no circular include dependency.

    // bindings.h:
    {
        let mut config = base_config.clone();
        config.include_guard = Some("main_bindings_h".into());
        // Some of our function signatures reference types defined in C headers,
        // so we need to include those here.
        config.includes = vec![
            "lib/logger/log_level.h".into(),
            "lib/shadow-shim-helper-rs/shim_helper.h".into(),
            "lib/tsc/tsc.h".into(),
            "main/bindings/c/bindings-opaque.h".into(),
            "main/core/worker.h".into(),
            "main/host/descriptor/descriptor_types.h".into(),
            "main/host/descriptor/tcp.h".into(),
            "main/host/descriptor/epoll.h".into(),
            "main/host/futex_table.h".into(),
            "main/host/network/network_interface.h".into(),
            "main/host/protocol.h".into(),
            "main/host/status_listener.h".into(),
            "main/host/tracker_types.h".into(),
            "main/routing/dns.h".into(),
            "main/routing/packet.minimal.h".into(),
        ];
        config.sys_includes = vec![
            "sys/socket.h".into(),
            "netinet/in.h".into(),
            "arpa/inet.h".into(),
        ];
        config.after_includes = {
            let mut v = base_config.after_includes.clone().unwrap();
            // We have to manually create the vararg declaration.
            // See crate::main::host::thread::export::thread_nativeSyscall.
            v.push_str("long thread_nativeSyscall(const Thread* thread, long n, ...);\n");
            Some(v)
        };
        config.export = cbindgen::ExportConfig {
            // This header's primary purpose is to export function
            // declarations.  We also need to export OpaqueItems here, or
            // else cbindgen generates bad type names when referencing those
            // types.
            item_types: vec![
                cbindgen::ItemType::Functions,
                cbindgen::ItemType::OpaqueItems,
            ],
            ..base_config.export.clone()
        };
        cbindgen::Builder::new()
            .with_crate(crate_dir.clone())
            .with_config(config)
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file("../../build/src/main/bindings/c/bindings.h");
    }

    // bindings-opaque.h
    {
        let mut config = base_config.clone();
        // We want to avoid including any C headers from this crate here,
        // which lets us avoid circular dependencies. Ok to depend on headers
        // generated by crates that this one depends on.
        config.includes = vec!["lib/shadow-shim-helper-rs/shim_helper.h".into()];
        config.include_guard = Some("main_opaque_bindings_h".into());
        config.after_includes = {
            let mut v = base_config.after_includes.clone().unwrap();
            // Manual forward declarations of C structs that we need,
            // since we can't include the corresponding header files without
            // circular definitions.
            v.push_str("typedef struct _SysCallCondition SysCallCondition;");
            Some(v)
        };
        config.export = cbindgen::ExportConfig {
            include: vec!["QDiscMode".into(), "FileSignals".into()],
            // Export everything except function definitions, since those are already
            // exported in the other header file, and need the C header files.
            item_types: base_config
                .export
                .item_types
                .iter()
                .filter(|t| **t != cbindgen::ItemType::Functions)
                .cloned()
                .collect(),
            ..base_config.export.clone()
        };
        cbindgen::Builder::new()
            .with_crate(crate_dir)
            .with_config(config)
            .generate()
            .expect("Unable to generate bindings")
            .write_to_file("../../build/src/main/bindings/c/bindings-opaque.h");
    }
}

fn run_bindgen(build_common: &ShadowBuildCommon) {
    let bindings = build_common
        .bindgen_builder()
        .header("core/affinity.h")
        .header("core/definitions.h")
        .header("core/worker.h")
        .header("host/descriptor/compat_socket.h")
        .header("host/descriptor/descriptor.h")
        .header("host/descriptor/epoll.h")
        .header("host/descriptor/regular_file.h")
        .header("host/descriptor/tcp_cong.h")
        .header("host/descriptor/tcp_cong_reno.h")
        .header("host/futex.h")
        .header("host/status.h")
        .header("host/status_listener.h")
        .header("host/syscall/fcntl.h")
        .header("host/syscall/file.h")
        .header("host/syscall/fileat.h")
        .header("host/syscall/futex.h")
        .header("host/syscall/ioctl.h")
        .header("host/syscall/poll.h")
        .header("host/syscall/protected.h")
        .header("host/syscall/select.h")
        .header("host/syscall/signal.h")
        .header("host/syscall/syscall_condition.h")
        .header("host/syscall/uio.h")
        .header("host/syscall/unistd.h")
        .header("host/syscall_numbers.h")
        .header("host/tracker.h")
        .header("routing/packet.h")
        .header("utility/rpath.h")
        .header("utility/utility.h")
        // Haven't decided how to handle glib struct types yet. Avoid using them
        // until we do.
        .blocklist_type("_?GQueue")
        .allowlist_function("g_list_append")
        .allowlist_function("g_list_free")
        .allowlist_type("GList")
        // Needs GQueue
        .opaque_type("_?LegacySocket.*")
        .blocklist_type("_?Socket.*")
        .allowlist_type("_?CompatSocket.*")
        // Uses atomics, which bindgen doesn't translate correctly.
        // https://github.com/rust-lang/rust-bindgen/issues/2151
        .blocklist_type("atomic_bool")
        .blocklist_type("_?ShimThreadSharedMem")
        .blocklist_type("_?ShimProcessSharedMem")
        .blocklist_type("ShimShmem.*")
        .allowlist_function("affinity_.*")
        .allowlist_function("managedthread_.*")
        .allowlist_function("tcp_.*")
        .allowlist_function("tcpcong_.*")
        .allowlist_function("legacyfile_.*")
        .allowlist_function("legacysocket_.*")
        .blocklist_function("legacysocket_init")
        .allowlist_function("networkinterface_.*")
        .allowlist_function("hostc_.*")
        // used by shadow's main function
        .allowlist_function("main_.*")
        .allowlist_function("tracker_.*")
        .allowlist_function("futex_.*")
        .allowlist_function("futextable_.*")
        .allowlist_function("shmemcleanup_tryCleanup")
        .allowlist_function("scanRpathForLib")
        .allowlist_function("runConfigHandlers")
        .allowlist_function("rustlogger_new")
        .allowlist_function("dns_.*")
        .allowlist_function("address_.*")
        .allowlist_function("compatsocket_.*")
        .allowlist_function("workerpool_updateMinHostRunahead")
        .allowlist_function("process_.*")
        .allowlist_function("shadow_logger_getDefault")
        .allowlist_function("shadow_logger_shouldFilter")
        .allowlist_function("logger_get_global_start_time_micros")
        .allowlist_function("regularfile_.*")
        .allowlist_function("statuslistener_.*")
        .allowlist_function("status_listener_.*")
        .allowlist_function("syscallcondition_.*")
        .allowlist_function("syscallhandler_.*")
        .allowlist_function("_syscallhandler_.*")
        .allowlist_function("tracker_*")
        .allowlist_function("worker_.*")
        .allowlist_function("workerc_.*")
        .allowlist_function("packet_.*")
        .allowlist_function("epoll_new")
        .allowlist_function("glib_check_version")
        //# Needs GQueue
        .blocklist_function("worker_finish")
        .blocklist_function("worker_bootHosts")
        .blocklist_function("worker_freeHosts")
        .allowlist_type("ForeignPtr")
        .allowlist_type("Status")
        .allowlist_type("StatusListener")
        .allowlist_type("SysCallCondition")
        .allowlist_type("LegacyFile")
        .allowlist_type("Manager")
        .allowlist_type("RegularFile")
        .allowlist_type("Epoll")
        .allowlist_type("FileType")
        .allowlist_type("Trigger")
        .allowlist_type("TriggerType")
        .allowlist_type("LogInfoFlags")
        .allowlist_type("SimulationTime")
        .allowlist_type("ProtocolTCPFlags")
        .allowlist_type("PacketDeliveryStatusFlags")
        .allowlist_type("ShadowSyscallNum")
        .allowlist_var("AFFINITY_UNINIT")
        .allowlist_var("CONFIG_HEADER_SIZE_TCP")
        .allowlist_var("CONFIG_PIPE_BUFFER_SIZE")
        .allowlist_var("CONFIG_MTU")
        .allowlist_var("SYSCALL_IO_BUFSIZE")
        .allowlist_var("SHADOW_SOMAXCONN")
        .allowlist_var("TCP_CONG_RENO_NAME")
        .allowlist_var("SHADOW_FLAG_MASK")
        .allowlist_var("GLIB_MAJOR_VERSION")
        .allowlist_var("GLIB_MINOR_VERSION")
        .allowlist_var("GLIB_MICRO_VERSION")
        .allowlist_var("glib_major_version")
        .allowlist_var("glib_minor_version")
        .allowlist_var("glib_micro_version")
        .opaque_type("SysCallCondition")
        .opaque_type("LegacyFile")
        .opaque_type("Manager")
        .opaque_type("Descriptor")
        .opaque_type("OpenFile")
        .opaque_type("File")
        .opaque_type("ConfigOptions")
        .opaque_type("Logger")
        .opaque_type("DescriptorTable")
        .opaque_type("MemoryManager")
        .opaque_type("TaskRef")
        .blocklist_type("Logger")
        .blocklist_type("Timer")
        .blocklist_type("Controller")
        .blocklist_type("Counter")
        .blocklist_type("Descriptor")
        .blocklist_type("Process")
        .blocklist_type("Host")
        .blocklist_type("HostId")
        .blocklist_type("TaskRef")
        .allowlist_type("WorkerC")
        .opaque_type("WorkerC")
        .allowlist_type("WorkerPool")
        .opaque_type("WorkerPool")
        .blocklist_type("HashSet_String")
        .blocklist_type("QDiscMode")
        // Imported from libc crate below
        .blocklist_type("siginfo_t")
        .blocklist_type("SysCallReg")
        .blocklist_type("SysCallArgs")
        .blocklist_type("ForeignPtr")
        .blocklist_type("ManagedPhysicalMemoryAddr")
        // we typedef `UntypedForeignPtr` to `ForeignPtr<()>` in rust
        .blocklist_type("UntypedForeignPtr")
        .disable_header_comment()
        .raw_line("/* automatically generated by rust-bindgen */")
        .raw_line("")
        .raw_line("use crate::core::configuration::QDiscMode;")
        .raw_line("use crate::host::descriptor::{File, FileSignals, OpenFile};")
        .raw_line("use crate::host::descriptor::socket::inet::{InetSocket, InetSocketWeak};")
        .raw_line("use crate::host::host::Host;")
        .raw_line("use crate::host::memory_manager::MemoryManager;")
        .raw_line("use crate::host::process::Process;")
        .raw_line("use crate::host::syscall::handler::SyscallHandler;")
        .raw_line("use crate::host::syscall::types::SyscallReturn;")
        .raw_line("use crate::host::thread::Thread;")
        .raw_line("use crate::utility::legacy_callback_queue::RootedRefCell_StateEventSource;")
        .raw_line("")
        .raw_line("use shadow_shim_helper_rs::HostId;")
        .raw_line("use shadow_shim_helper_rs::syscall_types::{ManagedPhysicalMemoryAddr, SysCallArgs, UntypedForeignPtr};")
        .raw_line("#[allow(unused)]")
        .raw_line("use shadow_shim_helper_rs::shim_shmem::{HostShmem, HostShmemProtected, ProcessShmem, ThreadShmem};")
        .raw_line("#[allow(unused)]")
        .raw_line("use shadow_shim_helper_rs::shim_shmem::export::{ShimShmemHost, ShimShmemHostLock, ShimShmemProcess, ShimShmemThread};")
        .raw_line("")
        // We have to manually generated the SysCallCondition opaque type.
        // bindgen skip auto-generating it because it's forward-declared in the cbindgen
        // generated headers, which we blocklist.
        .raw_line("#[repr(C)]")
        .raw_line("pub struct SysCallCondition{")
        .raw_line("    _unused: [u8; 0],")
        .raw_line("}")
        //# used to generate #[must_use] annotations)
        .enable_function_attribute_detection()
        //# don't generate rust bindings for c bindings of rust code)
        .blocklist_file(".*/bindings-opaque.h")
        .blocklist_file(".*/bindings.h")
        // shadow's C functions may call rust functions that do unwind, so I think we need this
        .override_abi(bindgen::Abi::CUnwind, ".*")
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("cshadow.rs"))
        .expect("Couldn't write bindings!");
}

fn build_remora(build_common: &ShadowBuildCommon) {
    build_common
        .cc_build()
        .cpp(true) // Switch to C++ library compilation.
        .file("host/descriptor/tcp_retransmit_tally.cc")
        .cpp_link_stdlib("stdc++")
        .compile("libremora.a");
}

fn build_shadow_c(build_common: &ShadowBuildCommon) {
    let mut build = build_common.cc_build();

    build.files(&[
        "core/affinity.c",
        "host/descriptor/descriptor.c",
        "host/status_listener.c",
        "host/descriptor/compat_socket.c",
        "host/descriptor/epoll.c",
        "host/descriptor/regular_file.c",
        "host/descriptor/socket.c",
        "host/descriptor/tcp.c",
        "host/descriptor/tcp_cong.c",
        "host/descriptor/tcp_cong_reno.c",
        "host/process.c",
        "host/futex.c",
        "host/futex_table.c",
        "host/syscall/protected.c",
        "host/syscall/fcntl.c",
        "host/syscall/file.c",
        "host/syscall/fileat.c",
        "host/syscall/futex.c",
        "host/syscall/ioctl.c",
        "host/syscall/poll.c",
        "host/syscall/select.c",
        "host/syscall/signal.c",
        "host/syscall/syscall_condition.c",
        "host/syscall/unistd.c",
        "host/syscall/uio.c",
        "host/network/network_interface.c",
        "host/network/network_queuing_disciplines.c",
        "host/tracker.c",
        "routing/payload.c",
        "routing/packet.c",
        "routing/address.c",
        "routing/dns.c",
        "utility/priority_queue.c",
        "utility/rpath.c",
        "utility/utility.c",
    ]);
    build.compile("shadow-c");
}

fn build_info() -> String {
    let profile = std::env::var("PROFILE").unwrap();
    let opt_level = std::env::var("OPT_LEVEL").unwrap();
    let debug = std::env::var("DEBUG").unwrap();
    let cflags = std::env::var("CFLAGS")
        .unwrap_or("<none>".to_string())
        .trim()
        .to_string();

    // replace the unicode separator character with a space
    let rflags = std::env::var("CARGO_ENCODED_RUSTFLAGS")
        .unwrap()
        .replace('\u{1f}', " ");

    // Note that the CFLAGS aren't necessarily the flags that the C code is built with. The `cc`
    // library is in charge of the flags. By default it's supposed to use CFLAGS (which I think we
    // should get from CMake), as well as any flags that are added manually through `flag()` or
    // similar methods. Unfortunately it doesn't seem like the library provides a way to see exactly
    // what flags were used during the build process. Here we provide this information as a best
    // effort.

    format!(
        "Shadow was built with \
        PROFILE={profile}, \
        OPT_LEVEL={opt_level}, \
        DEBUG={debug}, \
        RUSTFLAGS=\"{rflags}\", \
        CFLAGS=\"{cflags}\""
    )
}

fn main() {
    let deps = system_deps::Config::new().probe().unwrap();
    let build_common =
        shadow_build_common::ShadowBuildCommon::new(std::path::Path::new("../.."), Some(deps));

    // The C bindings should be generated first since cbindgen doesn't require
    // the Rust code to be valid, whereas bindgen does require the C code to be
    // valid. If the C bindings are no longer correct, but the Rust bindings are
    // generated first, then there will be no way to correct the C bindings
    // since the Rust binding generation will always fail before the C bindings
    // can be corrected.
    run_cbindgen(&build_common);
    run_bindgen(&build_common);

    build_remora(&build_common);
    build_shadow_c(&build_common);

    println!("cargo:rustc-env=SHADOW_BUILD_INFO={}", build_info());
}
