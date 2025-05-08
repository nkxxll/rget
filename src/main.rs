use std::fs::File;
use std::io::BufWriter;
use std::ops::Sub;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::thread::{self};
use std::time::Duration;
use std::{io::Write, sync::atomic::AtomicBool};

use clap::{Parser, Subcommand};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::Response;

const OUT_FILE: &str = "rget.out";
const DEFAULT_DEPTH: usize = 1;

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
    Interactive {
        #[arg(short, long, default_value = OUT_FILE)]
        outfile: String,
    },
    GetDepth {
        /// The URL to download
        url: String,
        #[arg(short, long, default_value_t = DEFAULT_DEPTH)]
        depth: usize,
    },
}

struct Spinner {
    chars: Vec<char>,
    stop_tx: Option<Sender<bool>>,
}

impl Spinner {
    fn new(chars: Option<Vec<char>>) -> Self {
        match chars {
            Some(ch) => Spinner {
                chars: ch,
                stop_tx: None,
            },
            None => {
                let chars = vec!['-', '\\', '|', '/'];
                Spinner {
                    chars,
                    stop_tx: None,
                }
            }
        }
    }

    fn start(&mut self) -> thread::JoinHandle<()> {
        let (tx, rx) = mpsc::channel::<bool>();
        let chars = self.chars.clone();
        self.stop_tx = Some(tx);

        thread::spawn(move || {
            let mut i = 0;
            loop {
                if rx.try_recv().is_ok_and(|x| x) {
                    break;
                }

                print!("\r{}", chars[i]);
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                thread::sleep(Duration::from_millis(100));
                i = (i + 1) % chars.len();
            }
            println!("\rDone!        ");
        })
    }

    fn stop(&mut self) {
        // Dropping the sender stops the spinner
        self.stop_tx.as_ref().unwrap().send(true).unwrap();
        self.stop_tx = None;
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    match &args.subs {
        SubCom::Interactive { outfile } => {
            return loop_download(outfile).await;
        }
        SubCom::Get { url, outfile } => {
            return download(url, outfile).await;
        }
        SubCom::GetDepth { url, depth } => {
            todo!("still needs to be implemented")
        }
    }
}

async fn loop_download(outfile: &str) -> Result<(), Box<dyn std::error::Error>> {
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
            None => outfile,
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

    let total_size = response.content_length();
    match total_size {
        Some(ts) => download_pb(outfile, ts, &mut response).await,
        None => download_sp(outfile, response).await,
    }
}

async fn download_pb(
    outfile: &str,
    total_size: u64,
    response: &mut Response,
) -> Result<(), Box<dyn std::error::Error>> {
    let pb = ProgressBar::new(total_size);
    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed_precise}] [{wide_bar}] {bytes}/{total_bytes} ({eta})",
        )
        .unwrap()
        .progress_chars("#>-"),
    );

    let mut dest = BufWriter::new(File::create(outfile)?);

    let mut downloaded: u64 = 0;

    while let Some(chunk) = response.chunk().await? {
        dest.write_all(chunk.as_ref())?;
        downloaded += chunk.len() as u64;
        pb.set_position(downloaded);
    }

    pb.finish_with_message("Download complete");
    Ok(())
}

async fn download_sp(outfile: &str, response: Response) -> Result<(), Box<dyn std::error::Error>> {
    let ab = Arc::new(AtomicBool::new(false));
    let clone = Arc::clone(&ab);
    let mut sp = Spinner::new(None);

    let handle = sp.start();
    let mut outfile = File::create(outfile)?;

    let content = response.bytes().await?;
    outfile.write_all(&content)?;
    clone.store(true, Ordering::Relaxed);
    sp.stop();
    handle.join().unwrap();

    Ok(())
}
