#![forbid(unsafe_code)]
use std::io;
fn main() {
    // Set the standalone process policy once, before any worker threads or
    // child processes exist. The pinned executor and initialization JVM inherit
    // it; the invoking shell and existing files are unaffected. Do not set or
    // temporarily restore a process-wide umask inside the library's runners.
    #[cfg(unix)]
    let _ = rustix::process::umask(rustix::fs::Mode::RWXG | rustix::fs::Mode::RWXO);
    let raw: Vec<_> = std::env::args_os().skip(1).collect();
    let args = match raw
        .iter()
        .map(|arg| arg.to_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
    {
        Some(args) => args,
        None => {
            let mut args = vec!["invalid-non-utf8-argument".to_owned()];
            if raw
                .iter()
                .take_while(|arg| *arg != "--")
                .any(|arg| arg == "--json")
            {
                args.push("--json".into());
            }
            args
        }
    };
    let exit = bxdl::cli::run(&args, &mut io::stdout().lock(), &mut io::stderr().lock());
    std::process::exit(exit);
}
