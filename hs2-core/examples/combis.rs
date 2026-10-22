fn main() {
    let dir = "F:/rea/_hypersonic2/iso/Hypersonic Content/Patches/120 Combis";
    let (mut ok, mut n) = (0, 0);
    for f in std::fs::read_dir(dir).unwrap() {
        let f = f.unwrap().path();
        n += 1;
        if let Some(c) = hs2_core::combi::parse(&std::fs::read(&f).unwrap()) {
            ok += 1;
            if ok <= 3 || c.parts.is_empty() {
                let parts: Vec<String> = c.parts.iter().map(|p| format!("{}{}:{}({})", p.part + 1, if p.link { "L" } else { "" }, p.patch.name, p.patch.elements.len())).collect();
                println!("{} -> {:?}", c.name, parts);
            }
        } else {
            println!("FAIL {}", f.display());
        }
    }
    println!("{ok}/{n} combis");
}
