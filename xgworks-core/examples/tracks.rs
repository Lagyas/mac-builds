fn main() {
    let d = std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
    let (m, _) = xgworks_core::edit_model(&d);
    for (i, t) in m.iter().enumerate() {
        println!("{:2} ch{:2} notes{:5} others{:4} {}", i + 1, t.channel + 1, t.notes.len(), t.others.len(), t.name);
    }
    for c in xgworks_core::iter_chunks(&d).iter().filter(|c| c.tag == *b"S4WT").take(20) {
        if let Some((n, p)) = xgworks_core::parse_s4wt(c.payload) {
            println!("S4WT {:12} {:?}", n, p.iter().map(|x| (x.block, x.start)).collect::<Vec<_>>());
        }
    }
}
