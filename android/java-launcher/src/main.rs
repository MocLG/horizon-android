//! `java` and `javac`, for the Android app. The runtime lives in the app's
//! data folder, where Android lets nothing start a program, so the app runs
//! this one from its native library folder instead. It loads the runtime's
//! own launcher library from `JAVA_HOME` and hands it the arguments, as
//! `bin/java` does. Started under a name with `javac` in it, it is the
//! compiler.
use std::ffi::{CString, c_char, c_int, c_void};

const RTLD_NOW: c_int = 2;
const RTLD_GLOBAL: c_int = 0x100;

unsafe extern "C" {
    fn dlopen(file: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, name: *const c_char) -> *mut c_void;
    fn dlerror() -> *const c_char;
}

/// `JLI_Launch` from the JDK's `java.h`.
type Launch = unsafe extern "C" fn(
    argc: c_int,
    argv: *mut *mut c_char,
    jargc: c_int,
    jargv: *const *const c_char,
    appclassc: c_int,
    appclassv: *const *const c_char,
    fullversion: *const c_char,
    dotversion: *const c_char,
    pname: *const c_char,
    lname: *const c_char,
    javaargs: u8,
    cpwildcard: u8,
    javaw: u8,
    ergo: i32,
) -> c_int;

fn error() -> String {
    // SAFETY: dlerror returns a thread-local string or null.
    let message = unsafe { dlerror() };
    if message.is_null() {
        return "unknown error".into();
    }
    // SAFETY: a non-null dlerror result is a C string.
    unsafe { std::ffi::CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

fn main() {
    let Some(home) = std::env::var_os("JAVA_HOME") else {
        eprintln!("java: JAVA_HOME is not set");
        std::process::exit(1);
    };
    let home = std::path::PathBuf::from(home);
    // libjvm and the runtime's other libraries come from the folders the app
    // puts in LD_LIBRARY_PATH; libjli is named by its full path.
    let jli = CString::new(home.join("lib/libjli.so").to_string_lossy().as_bytes()).unwrap();
    // SAFETY: a valid C string; the library stays loaded for the process.
    let handle = unsafe { dlopen(jli.as_ptr(), RTLD_NOW | RTLD_GLOBAL) };
    if handle.is_null() {
        eprintln!("java: cannot load {}: {}", jli.to_string_lossy(), error());
        std::process::exit(1);
    }
    // SAFETY: the symbol name is a C string literal.
    let launch = unsafe { dlsym(handle, c"JLI_Launch".as_ptr()) };
    if launch.is_null() {
        eprintln!("java: the runtime has no JLI_Launch: {}", error());
        std::process::exit(1);
    }
    // SAFETY: JLI_Launch has this signature in every JDK since 9.
    let launch: Launch = unsafe { std::mem::transmute(launch) };
    // The compiler is java with javac's module as the main one: bin/javac's
    // own mode (JAVA_ARGS) looks for the runtime beside the program, which
    // this one is not.
    let javac = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_name().map(|n| n.to_string_lossy().contains("javac")))
        .unwrap_or(false);
    let mut args: Vec<CString> = std::env::args_os()
        .skip(1)
        .map(|a| CString::new(a.into_encoded_bytes()).unwrap())
        .collect();
    if javac {
        args.splice(
            0..0,
            [c"-m", c"jdk.compiler/com.sun.tools.javac.Main"].map(CString::from),
        );
    }
    args.insert(0, CString::from(c"java"));
    let mut argv: Vec<*mut c_char> = args.iter_mut().map(|a| a.as_ptr().cast_mut()).collect();
    argv.push(std::ptr::null_mut());
    let java = c"java".as_ptr();
    // SAFETY: argv is null terminated and outlives the call, which does not
    // return before the program ends.
    let status = unsafe {
        launch(
            args.len() as c_int,
            argv.as_mut_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            c"25".as_ptr(),
            c"25".as_ptr(),
            java,
            java,
            0,
            1,
            0,
            0,
        )
    };
    std::process::exit(status);
}
