fn main() {
    let d = std::fs::read("F:/rea/A Dream of a Dream.XWS").unwrap();
    let (m, _) = xgworks_core::edit_model(&d);
    for (k, t) in m.iter().enumerate() {
        for e in &t.others {
            if let xgworks_core::Event::Meta { t: tk, mtype, data } = e {
                if *mtype == 0x7F || *mtype == 0x06 { println!("trk{k} t{tk} FF{mtype:02X} {:02x?}", data); }
            }
        }
    }
}
