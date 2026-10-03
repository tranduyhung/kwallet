use std::env;
use std::process;

fn help_text() -> String {
    let mut text = String::new();
    text.push_str("Usage: kwallet <COMMAND>\n\n");
    text.push_str("Commands:\n");
    text.push_str("    new      New\n");
    text.push_str("    balance  Balance\n");
    text.push_str("    send     Send\n");
    text.push_str("    version  Print version\n");
    text.push_str("    help     Print help");
    text
}

fn main() {
    // Collect command line arguments into a Vec<String>
    let args: Vec<String> = env::args().collect();

    // If no command is provided (args[0] is always the program path)
    if args.len() < 2 {
        println!("{}", help_text());
        process::exit(0);
    }
    if args.len() > 2 {
        eprintln!("error: expected only one command");
        process::exit(1);
    }

    // Borrow the argument as a string slice (&str) for pattern matching
    let command: &str = &args[1];

    match command {
        "new" | "balance" | "send" => {
            println!("not implemented yet");
        }
        "version" => {
            println!("kwallet v{}", env!("CARGO_PKG_VERSION"));
        }
        "help" => {
            println!("{}", help_text());
        }
        unknown => {
            eprintln!("error: unrecognized command '{unknown}'");
            eprintln!("{}", help_text());
            process::exit(1);
        }
    }
}