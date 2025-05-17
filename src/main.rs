pub mod structures;
use std::collections::VecDeque;
use std::fs::File;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::BufWriter;
use std::ops::Sub;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Sender};
use std::thread::{self};
use std::time::Duration;
use std::{io::Write, sync::atomic::AtomicBool};

use clap::{Parser, Subcommand};
use http::header::CONTENT_TYPE;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use reqwest::Response;
use scraper::{Html, Selector};

const OUT_FILE: &str = "rget.out";
const DEFAULT_DEPTH: usize = 1;
const MAX_THREADS: usize = 10;

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

#[derive(Debug)]
pub enum TextType {
    Plain,
    Html,
    Css,
    Javascript,
    Xml,
    Markdown,
    Csv,
    Richtext,
    TabSeparatedValues,
}

#[derive(Debug)]
pub enum ContentType {
    Text(TextType), // For specific text formats
    Other(String),  // For any other content type, storing the string value
    Unknown,        // For cases where the header is missing or invalid
}

impl ContentType {
    pub fn from_header_value(ct_value: Option<&http::HeaderValue>) -> Self {
        match ct_value {
            Some(value) => {
                match value.to_str() {
                    Ok(ct_str) => match ct_str {
                        "text/plain" => ContentType::Text(TextType::Plain),
                        "text/html" => ContentType::Text(TextType::Html),
                        "text/css" => ContentType::Text(TextType::Css),
                        "text/javascript" => ContentType::Text(TextType::Javascript),
                        "text/xml" => ContentType::Text(TextType::Xml),
                        "text/markdown" => ContentType::Text(TextType::Markdown),
                        "text/csv" => ContentType::Text(TextType::Csv),
                        "text/richtext" => ContentType::Text(TextType::Richtext),
                        "text/tab-separated-values" => {
                            ContentType::Text(TextType::TabSeparatedValues)
                        }
                        other => ContentType::Other(other.to_string()), // Store the unknown type
                    },
                    Err(_) => ContentType::Unknown, // Header value not valid UTF-8
                }
            }
            None => ContentType::Unknown, // Header is missing
        }
    }
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

struct Worker {
    id: usize,
    thread: thread::JoinHandle<()>,
}

impl Worker {
    fn new(id: usize) -> Worker {
        let thread = thread::spawn(|| {});

        Worker { id, thread }
    }
}

struct ThreadPool {
    workers: Vec<Worker>,
}

impl ThreadPool {
    fn new(size: usize) -> ThreadPool {
        assert!(size > 0);
        let mut workers = Vec::with_capacity(size);

        for id in 0..size {
            workers.push(Worker::new(id));
        }

        ThreadPool { workers }
    }
    pub fn execute<F>(&self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
    }
}

#[derive(Debug, Clone)]
struct Node {
    value: String,
    children: Vec<Self>,
}

struct Tree<'a> {
    head: &'a Node,
    depth: usize,
}

impl<'a> Tree<'a> {
    fn new(h: &'a Node) -> Self {
        Tree { head: h, depth: 1 }
    }
}

impl Node {
    fn new(value: String) -> Self {
        Node {
            value,
            children: Vec::new(),
        }
    }
    async fn traverse(&self) {
        let mut q = VecDeque::new();
        q.push_back(self);

        while !q.is_empty() {
            let n = q.pop_front().unwrap();
            let value = &n.value;
            let hash = hash_file_name(value.clone());
            download(value, &hash).await.unwrap();
            n.children.iter().for_each(|node| q.push_back(node));
        }
    }
}

fn hash_file_name(s: String) -> String {
    let mut hasher = DefaultHasher::new();
    s.hash(&mut hasher);
    format!("{:x}", hasher.finish())
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
        SubCom::GetDepth { url, depth } => download_depth(url, *depth).await,
    }
}

async fn get_urls(n: Rc<Mutex<Node>>, max_depth: usize) {
    let mut cur_width = 1;
    let mut next_width = 1;
    let mut cur_count = 1;
    let mut cur_depth = 0;
    let mut q = VecDeque::new();
    q.push_back(n);
    while !q.is_empty() && max_depth < cur_depth {
        let current = q.pop_front().unwrap();
        cur_count += 1;
        let res = reqwest::get(current.lock().unwrap().value.clone())
            .await
            .unwrap()
            .error_for_status()
            .unwrap();
        let content_type = ContentType::from_header_value(res.headers().get(CONTENT_TYPE));
        match content_type {
            ContentType::Text(_) => {
                let site = res.text().await.unwrap();
                let nodes = find_https_links_with_parser(&site);
                current.lock().unwrap().children = nodes.iter().map(|x| Node::new(x.to_string())).collect();
                for node in current.lock().unwrap().children.clone() {
                    let push = Rc::new(Mutex::new(node));
                    q.push_back(push);
                }
                next_width += current.lock().unwrap().children.len();
                if cur_width <= cur_count {
                    cur_depth += 1;
                    cur_width = next_width;
                    next_width = 0;
                    cur_count = 0;
                }
            }
            ContentType::Other(string) => {
                println!("other content type: {string} stopping at depth {cur_depth}");
                continue;
            }
            _ => unreachable!("the header should work"),
        }
    }
}

fn find_https_links_with_parser(html_content: &str) -> Vec<String> {
    let document = Html::parse_document(html_content);

    let href_selector =
        Selector::parse("body a[href], body img[src]").expect("Failed to create selector");

    let mut https_urls = Vec::new();

    for element in document.select(&href_selector) {
        // Check for the 'href' attribute first
        if let Some(href) = element.attr("href") {
            if href.starts_with("https://") {
                https_urls.push(href.to_string());
            }
        }
        // If no 'href', check for the 'src' attribute (for img tags)
        else if let Some(src) = element.attr("src") {
            if src.starts_with("https://") {
                https_urls.push(src.to_string());
            }
        }
        // Add checks for other attributes/tags as needed
    }

    https_urls
}

async fn download_depth(url: &str, depth: usize) -> Result<(), Box<dyn std::error::Error>> {
    let head = Node::new(url.to_string());
    let rc_head = Rc::new(Mutex::new(head));
    let rc_headc = Rc::clone(&rc_head);
    get_urls(rc_head, depth).await;
    dbg!(&rc_headc);
    rc_headc.lock().unwrap().traverse().await;
    Ok(())
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
