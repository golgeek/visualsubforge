// Independently authored faulty helper. Compiled only by the process tests.
use std::{
    io::{self, Read, Write},
    time::Duration,
};
fn main() {
    let args: Vec<_> = std::env::args().collect();
    std::fs::write(&args[2], std::process::id().to_string()).unwrap();
    if args[1] != "no-read" {
        io::stdin().read_to_end(&mut Vec::new()).unwrap();
    }
    match args[1].as_str() {
        "crash" => std::process::exit(23),
        "bad-json" => {
            print!("not json");
            return;
        }
        "wrong-version" => {
            print!("{{\"version\":2,\"result\":{{\"Ok\":null}}}}");
            return;
        }
        "nested-json" | "deep-json" => {
            let depth = if args[1] == "nested-json" { 32 } else { 160 };
            print!(
                "{{\"version\":1,\"result\":{{\"Ok\":{}1{}}}}}",
                "[".repeat(depth),
                "]".repeat(depth)
            );
            return;
        }
        "output-flood" => {
            io::stdout()
                .write_all(&vec![b'x'; 32 * 1024 * 1024 + 1])
                .unwrap();
        }
        "stderr-flood" => {
            io::stderr().write_all(&vec![b'x'; 4097]).unwrap();
        }
        "after-reply" => {
            print!("{{\"version\":1,\"result\":{{\"Ok\":null}}}}");
            io::stdout().flush().unwrap();
        }
        "no-read" => {}
        _ => panic!("Unknown probe mode"),
    }
    loop {
        std::thread::sleep(Duration::from_secs(60));
    }
}
