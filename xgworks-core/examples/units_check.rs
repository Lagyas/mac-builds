use xgworks_core::xgtbl::parse_tbl;
use xgworks_core::xgunits::{fx_display, fx_slot_desc, Enums};
fn main() {
    let t = parse_tbl(&std::fs::read("F:/rea/XG4/XGEditor/MU128.tbl").unwrap());
    let e = Enums::parse(&std::fs::read("F:/rea/XG4/ws.ini").unwrap());
    for name in ["*DelayLCR_var", "*Chorus_cho_ins", "*LoFi", "*EaryRef", "*3-BandEq", "*Rotary_var"] {
        println!("== {name}");
        let l = &t.fx_params[name];
        for (k, p) in l.iter().enumerate() {
            let Some(p) = p else { continue };
            match fx_slot_desc(l, k) {
                Some(d) => println!("  {:2} {:24} <-> {:20} min={} max={}", k + 1, p.name, d.0,
                    fx_display(d, p.min, &e), fx_display(d, p.max, &e)),
                None => println!("  {:2} {:24} <-> (none)", k + 1, p.name),
            }
        }
    }
}
