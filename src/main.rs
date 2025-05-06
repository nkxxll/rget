use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;
use std::{io::Write, sync::atomic::AtomicBool};

use clap::Parser;
use reqwest::Response;

const OUT_FILE: &str = "rget.out";

/// Simple program to download a URL
#[derive(Parser, Debug)]
#[command(name = "rget", about = "A Rust wget clone")]
struct Args {
    /// The URL to download
    url: String,
    #[arg(short, long, default_value = OUT_FILE)]
    outfile: String,
    #[arg(short, long)]
    /// Whether to run the rget app interactively
    interactive: bool,
}

struct Spinner<'a> {
    chars: Vec<char>,
    ab: &'a Arc<AtomicBool>,
}

impl<'a> Spinner<'a> {
    fn new(chars: Vec<char>, ab: &'a Arc<AtomicBool>) -> Self {
        Spinner { chars, ab }
    }

    fn start(self) -> std::thread::JoinHandle<()> {
        let clone = Arc::clone(self.ab);
        thread::spawn(move || {
            let mut i = 0;
            while !clone.load(Ordering::Relaxed) {
                print!("\r{}", self.chars[i]);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                thread::sleep(Duration::from_millis(100));
                i = (i + 1) % self.chars.len();
            }
            println!("\rDone!        ");
        })
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    if args.interactive {
        return loop_download().await;
    } else {
        return download(args.url.as_str(), args.outfile.as_str()).await;
    }
}

async fn loop_download() -> Result<(), Box<dyn std::error::Error>> {
    loop {
        let mut buf = String::new();
        print!("> ");
        std::io::stdout().flush().unwrap();
        std::io::stdin()
            .read_line(&mut buf)
            .expect("You should always be able to read a line");

        let mut split = buf.split_whitespace();
        let url = split.next().unwrap_or("quit");

        let of = match split.next() {
            Some(outfile) => outfile,
            None => OUT_FILE,
        };

        if url == "quit" || url == "q" {
            break;
        }

        let res = download(url, of).await;
        match res {
            Ok(()) => {}
            Err(e) => {
                return Err(e);
            }
        }
    }
    Ok(())
}

async fn download(url: &str, outfile: &str) -> Result<(), Box<dyn std::error::Error>> {
    let ab = Arc::new(AtomicBool::new(false));
    let chars = vec!['-', '\\', '|', '/'];
    let s = Spinner::new(chars, &ab);

    let join = s.start();
    let res = reqwest::get(url).await;
    match res {
        Ok(r) => {
            save_res(r, outfile).await?;
        }
        Err(e) => {
            eprintln!("Error executing GET request: {}", e);
        }
    }

    // stop the spinner
    ab.store(true, Ordering::Relaxed);

    join.join().unwrap();
    Ok(())
}

async fn save_res(r: Response, outfile: &str) -> Result<(), Box<dyn std::error::Error>> {
    let bytes = r.bytes().await?;

    // write the bites to the outfile
    let mut file = std::fs::File::create(outfile)?;
    file.write_all(&bytes)?;

    Ok(())
}
