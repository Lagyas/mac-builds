//! `xgworks` CLI: inspect an .XWS and convert between .XWS and .MID.
//!
//!   xgworks dump   song.XWS
//!   xgworks mid    song.XWS out.mid   # .XWS -> .MID (faithful named tracks)
//!   xgworks blocks song.XWS out.mid   # .XWS -> .MID (one track per S4WB block)
//!   xgworks import song.MID out.xws   # .MID -> .XWS

use std::process::exit;
use xgworks_core::*;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!(
            "usage:\n  {0} dump <file.XWS>\n  {0} mid <in.XWS> <out.MID>\n  \
             {0} blocks <in.XWS> <out.MID>\n  {0} import <in.MID> <out.XWS>",
            args.first().map(String::as_str).unwrap_or("xgworks")
        );
        exit(2);
    }
    let cmd = args[1].as_str();
    let infile = &args[2];
    let data = match std::fs::read(infile) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("cannot read {infile}: {e}");
            exit(1);
        }
    };

    match cmd {
        "dump" => dump(&data),
        "mid" | "blocks" | "import" => {
            if args.len() < 4 {
                eprintln!("{cmd} needs an output path");
                exit(2);
            }
            let out = &args[3];
            let bytes = match cmd {
                "mid" => export_named(&data),
                "blocks" => export_blocks(&data),
                _ => import_mid(&data),
            };
            if let Err(e) = std::fs::write(out, &bytes) {
                eprintln!("cannot write {out}: {e}");
                exit(1);
            }
            println!("wrote {out} ({} bytes)", bytes.len());
        }
        other => {
            eprintln!("unknown command: {other}");
            exit(2);
        }
    }
}

fn dump(data: &[u8]) {
    let mut s4wb = 0usize;
    let mut s4wt = 0usize;
    let mut consumed = 0usize;
    for c in iter_chunks(data) {
        consumed = c.offset + 8 + c.payload.len();
        let tag = String::from_utf8_lossy(&c.tag);
        match &c.tag {
            b"S4WH" => {
                let h = parse_s4wh(c.payload);
                println!("[{:#08x}] S4WH  len={}", c.offset, c.payload.len());
                println!("    version : {}.{}", h.version_major, h.version_minor);
                println!("    tracks  : {}  (S4WT pool)", h.track_count);
                println!("    blocks  : {}  (S4WB count)", h.block_count);
                println!("    ppqn    : {}  ticks/quarter", h.ppqn);
            }
            b"S4WB" => {
                let ev = parse_s4wb(c.payload);
                let notes = ev.iter().filter(|e| matches!(e, Event::Note { .. })).count();
                let maxt = ev.iter().map(|e| e.t()).max().unwrap_or(0);
                println!(
                    "[{:#08x}] S4WB #{s4wb}  len={}  events={}  notes={notes}  maxtick={maxt}",
                    c.offset,
                    c.payload.len(),
                    ev.len()
                );
                s4wb += 1;
            }
            b"S4WT" => {
                if let Some((name, pls)) = parse_s4wt(c.payload) {
                    let desc: Vec<String> =
                        pls.iter().map(|p| format!("blk{}@{}", p.block, p.start)).collect();
                    println!(
                        "[{:#08x}] S4WT #{s4wt:<3} len={:<4} name=\"{name}\"  [{}]",
                        c.offset,
                        c.payload.len(),
                        desc.join(", ")
                    );
                }
                s4wt += 1;
            }
            b"S4ST" => println!("[{:#08x}] S4ST (trailer)  len={}", c.offset, c.payload.len()),
            _ => println!("[{:#08x}] {}  len={}  (unrecognized)", c.offset, tag, c.payload.len()),
        }
    }
    let clean = consumed == data.len();
    println!(
        "\nWalk {}",
        if clean { "clean to EOF".into() } else { format!("STOPPED at {consumed:#x} of {:#x}", data.len()) }
    );
}
