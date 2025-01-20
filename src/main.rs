use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use my_bittorrent::{Keys, Torrent, TrackerRequest, TrackerResponse};
use sha1::{Digest, Sha1};

#[derive(Debug, Parser)]
#[command(author, version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Decode { value: String },
    Info { torrent: PathBuf },
    Peers { torrent: PathBuf },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    match args.command {
        Command::Decode { value } => {
            let v: serde_json::Value = decode_bencoded_value(&value).0;
            println!("{}", v);
        }
        Command::Info { torrent } => {
            let dot_torrent = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent =
                serde_bencode::from_bytes(&dot_torrent).context("parse torrent file")?;
            println!("Tracker URL: {}", t.announce);
            let length = if let Keys::SingleFile { length } = t.info.keys {
                length
            } else {
                todo!();
            };
            println!("{length}");
            let info_hash = t.info_hash();
            println!("Info Hash: {}", hex::encode(info_hash));
            println!("Piece Length: {}", t.info.plength);
            println!("Piece Hashes:");
            for hash in t.info.pieces.0 {
                println!("{}", hex::encode(&hash));
            }
        }
        Command::Peers { torrent } => {
            let dot_torrent = std::fs::read(torrent).context("read torrent file")?;
            let t: Torrent =
                serde_bencode::from_bytes(&dot_torrent).context("parse torrent file")?;
            let length = if let Keys::SingleFile { length } = t.info.keys {
                length
            } else {
                todo!();
            };
            let info_hash = t.info_hash();

            let request = TrackerRequest {
                info_hash: info_hash.into(),
                peer_id: String::from("00112233445566778899"),
                port: 6881,
                uploaded: 0,
                downloaded: 0,
                left: length,
                compact: 1,
            };

            let url_params =
                serde_urlencoded::to_string(&request).context("url-encode tracker parameters")?;
            let tracker_url = format!(
                "{}?{}&info_hash={}",
                t.announce,
                url_params,
                &urlencode(&info_hash)
            );
            let response = reqwest::get(tracker_url).await.context("query tracker")?;
            let response = response.bytes().await.context("fetch tracker response")?;
            let response: TrackerResponse =
                serde_bencode::from_bytes(&response).context("parse tracker response")?;
            for peer in &response.peers.0 {
                println!("{}:{}", peer.ip(), peer.port());
            }
        }
    }

    Ok(())
}

fn decode_bencoded_value(encoded_value: &str) -> (serde_json::Value, &str) {
    match encoded_value.chars().next() {
        Some('i') => {
            if let Some((n, rest)) = encoded_value
                .strip_prefix('i')
                .and_then(|rest| rest.split_once('e'))
                .and_then(|(digits, rest)| {
                    let n = digits.parse::<i64>().ok();
                    Some((n, rest))
                })
            {
                return (n.into(), rest);
            }
        }
        Some('l') => {
            let mut values = Vec::new();
            let mut rest = encoded_value.split_at(1).1;
            while !rest.is_empty() && !rest.starts_with('e') {
                let (v, remainder) = decode_bencoded_value(rest);
                values.push(v);
                rest = remainder;
            }
            return (values.into(), &rest[1..]);
        }
        Some('d') => {
            let mut dict = serde_json::Map::new();
            let mut rest = encoded_value.split_at(1).1;
            while !rest.is_empty() && !rest.starts_with('e') {
                let (k, remainder) = decode_bencoded_value(rest);
                let k = match k {
                    serde_json::Value::String(k) => k,
                    k => {
                        panic!("dict keys must be strings, not {k:?}");
                    }
                };
                let (v, remainder) = decode_bencoded_value(remainder);
                dict.insert(k, v);
                rest = remainder;
            }
            return (dict.into(), &rest[1..]);
        }
        Some('0'..='9') => {
            if let Some((len, rest)) = encoded_value.split_once(":") {
                if let Ok(len) = len.parse::<usize>() {
                    return (serde_json::Value::String(rest[..len].into()), &rest[len..]);
                }
            }
        }
        _ => {}
    }

    panic!("unhandled encoded value: {}", encoded_value);
}

fn urlencode(t: &[u8; 20]) -> String {
    let mut encoded = String::with_capacity(3 * t.len());
    for &byte in t {
        encoded.push('%');
        encoded.push_str(&hex::encode(&[byte]));
    }
    encoded
}
