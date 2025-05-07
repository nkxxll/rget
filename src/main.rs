use std::fs::File;
use std::io::BufWriter;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread::{self, sleep};
use std::time::Duration;
use std::{io::Write, sync::atomic::AtomicBool};

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::Response;

const OUT_FILE: &str = "rget.out";

/// Simple program to download a URL
#[derive(Parser, Debug)]
#[command(name = "rget", about = "A Rust wget clone")]
struct Args {
    #[command(subcommand)]
    subs: SubCom,
}

#[derive(Subcommand, Debug)]
enum SubCom {
    /// get an file from an url
    Get {
        /// The URL to download
        url: String,
        #[arg(short, long, default_value = OUT_FILE)]
        outfile: String,
    },
    /// start the program in interactive mode
    Interactive,
}

struct Spinner {
    chars: Vec<char>,
    ab: Arc<AtomicBool>,
}

impl Spinner {
    fn new(ab: Arc<AtomicBool>) -> Self {
        let chars = vec!['-', '\\', '|', '/'];
        Spinner { chars, ab }
    }

    fn start(self) -> std::thread::JoinHandle<()> {
        thread::spawn(move || {
            let mut i = 0;
            while !self.ab.load(Ordering::Relaxed) {
                print!("\r{}", self.chars[i]);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                thread::sleep(Duration::from_millis(100));
                i = (i + 1) % self.chars.len();
            }
            println!("\rDone!        ");
        })
    }

    fn stop(self) {
        self.ab.store(true, Ordering::Relaxed)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let ab = Arc::new(AtomicBool::new(false));
    let clone = Arc::clone(&ab);
    let sp = Spinner::new(ab);

    sp.start();
    match &args.subs {
        SubCom::Interactive => {
            sleep(Duration::from_secs(1));
            clone.store(true, Ordering::Relaxed);
            return loop_download().await;
        }
        SubCom::Get { url, outfile } => {
            sleep(Duration::from_secs(1));
            clone.store(true, Ordering::Relaxed);
            return download(url, outfile).await;
        }
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
    let mut response = Client::new().get(url).send().await?.error_for_status()?;

    let total_size = response
        .content_length()
        .ok_or("Failed to get content length")?;

    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{wide_bar}] {bytes}/{total_bytes} ({eta})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let mut dest = BufWriter::new(File::create(outfile)?);

    let mut downloaded: usize = 0;

    while let Some(chunk) = response.chunk().await? {
        dest.write_all(chunk.as_ref())?;
        downloaded += chunk.len();
        pb.set_position(downloaded as u64);
    }

    pb.finish_with_message("Download complete");
    Ok(())
}
