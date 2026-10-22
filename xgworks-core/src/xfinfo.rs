//! XF information (XGworks' "XF Information Editor", xfh.dll). XGworks keeps it
//! as events at tick 0 of the conductor track, appended after the existing
//! tick-0 events, in this order (checked on a song saved by the original):
//!
//! ```text
//! FF 02 copyright
//! FF 7F 43 7B 00 "XF01" 00 11          XF version
//! FF 01 "XFhd:date:country:category:beat:instrument:vocal:composer:lyricist:arranger:performer:programmer"
//! FF 01 "XFln:language:song:composer:lyricist:arranger:performer:programmer"
//! FF 07 "$Lyrc:melody1[,melody2]:offset:language"
//! ```
//!
//! plus the sequence name (FF 03) and the first time signature / tempo.

use crate::{EditTrack, Event};

pub const COUNTRIES: &str = "AD: Andorra|AE: United Arab Emirates|AF: Afghanistan|AG: Antigua and Barbuda|AI: Anguilla|AL: Albania|AM: Armenia|AN: Netherlands Antilles|AO: Angola|AQ: Antarctica|AR: Argentina|AS: American Samoa|AT: Austria|AU: Australia|AW: Aruba|AZ: Azerbaijan|BA: Bosnia-Hercegovina|BB: Barbados|BD: Bangladesh|BE: Belgium|BF: Burkina Faso|BG: Bulgaria|BH: Bahrain|BI: Burundi|BJ: Benin|BM: Bermuda|BN: Brunei Darussalam|BO: Bolivia|BR: Brazil|BS: Bahamas|BT: Bhutan|BV: Bouvet Island|BW: Botswana|BY: Belarus|BZ: Belize|CA: Canada|CC: Cocos (Keeling) Islands|CF: Central African Republic|CG: Congo|CH: Switzerland|CI: Cote d'Ivoire|CK: Cook Islands|CL: Chile|CM: Cameroon|CN: China|CO: Colombia|CR: Costa Rica|CU: Cuba|CV: Cape Verde|CX: Christmas Island|CY: Cyprus|CZ: Czech Republic|DE: Germany|DJ: Djibouti|DK: Denmark|DM: Dominica|DO: Dominican Republic|DZ: Algeria|EC: Ecuador|EE: Estonia|EG: Egypt|EH: Western Sahara|ES: Spain|ET: Ethiopia|FI: Finland|FJ: Fiji|FK: Falkland Islands|FM: Micronesia|FO: Faroe Islands|FR: France|GA: Gabon|GD: Grenada|GE: Georgia|GF: French Guiana|GH: Ghana|GI: Gibraltar|GL: Greenland|GM: Gambia|GN: Guinea|GP: Guadeloupe|GQ: Equatorial Guinea|GR: Greece|GT: Guatemala|GU: Guam|GW: Guinea-Bissau|GY: Guyana|HK: Hong Kong|HM: Heard and McDonald Islands|HN: Honduras|HR: Croatia|HT: Haiti|HU: Hungary|ID: Indonesia|IE: Ireland|IL: Israel|IN: India|IO: British Indian Ocean Territory|IQ: Iraq|IR: Iran|IS: Iceland|IT: Italy|JM: Jamaica|JO: Jordan|JP: Japan|KE: Kenya|KG: Kyrgyzstan|KH: Cambodia|KI: Kiribati|KM: Comoros|KN: Saint Kitts and Nevis|KR: Korea|KW: Kuwait|KY: Cayman Islands|KZ: Kazakhstan|LA: Lao People's Democratic Republic|LB: Lebanon|LC: Saint Lucia|LI: Liechtenstein|LK: Sri Lanka|LR: Liberia|LS: Lesotho|LT: Lithuania|LU: Luxembourg|LV: Latvia|LY: Libyan Arab Jamahiriya|MA: Morocco|MC: Monaco|MD: Moldova|MG: Madagascar|MH: Marshall Islands|ML: Mali|MM: Myanmar|MN: Mongolia|MO: Macau|MP: Northern Mariana Islands|MQ: Martinique|MR: Mauritania|MS: Montserrat|MT: Malta|MU: Mauritius|MV: Maldives|MW: Malawi|MX: Mexico|MY: Malasia|MZ: Mozambique|NA: Namibia|NC: New Caledonia|NE: Niger|NF: Norfolk Island|NG: Nigeria|NI: Nicaragua|NL: Netherlands|NO: Norway|NP: Nepal|NR: Nauru|NT: Neutral Zone (between Saudi Arabia & Iraq)|NU: Niue|NZ: New Zealand|OM: Oman|PA: Panama|PE: Peru|PF: French Polynesia|PG: Papua New Guinea|PH: Philippines|PK: Pakistan|PL: Poland|PM: Saint Pierre and Miquelon|PN: Pitcairn|PR: Puerto Rico|PT: Portugal|PW: Palau|PY: Paraguay|QA: Qatar|RE: Re'union|RO: Romania|RU: Russian Federation|RW: Rwanda|SA: Saudi Arabia|SB: Solomon Islands|SC: Seychelles|SD: Sudan|SE: Sweden|SG: Singapore|SH: Saint Helena|SI: Slovenia|SJ: Svalbard and Jan Mayen islands|SK: Slovakia|SL: Sierra Leone|SM: San Marino|SN: Senegal|SO: Somalia|SR: Suriname|ST: Sao Tome and Principe|SU: Former Soviet Union|SV: El Salvador|SY: Syria|SZ: Swaziland|TC: Turks and Caicos Islands|TD: Chad|TF: French Southern Territories|TG: Togo|TH: Thailand|TJ: Tajikistan|TK: Tokelau|TM: Turkmenistan|TN: Tunisia|TO: Tonga|TP: East Timor|TR: Turkey|TT: Trinidad and Tobago|TV: Tuvalu|TW: Taiwan|TZ: Tanzania|UA: Ukraine|UG: Uganda|UK: United Kingdom|UM: United States Minor Outlying Islands|US: United States|UY: Uruguay|UZ: Uzbekistan|VA: Vatican City State|VC: Saint Vincent and the Grenadines|VE: Venezuela|VG: Virgin Islands (British)|VI: Virgin Islands (U.S.)|VN: Vietnam|VU: Vanuatu|WF: Wallis & Futuna Islands|WS: Samoa|YE: Yemen|YU: Yugoslavia|ZA: South Africa|ZM: Zambia|ZR: Zaire|ZW: Zimbabwe";
pub const CATEGORIES: [&str; 29] = [
    "Ballroom", "Blues", "Bossa-Samba", "Children's Song", "Christmas-Hymn", "Classical", "C&W", "Dance", "Easy Listening", "Folk",
    "Jazz-Fusion", "J.Enka", "J.Folk-GS", "J.Pop", "J.Kayoukyoku", "J.Minyou", "Latin", "March", "NewAge", "Reggae", "Rock",
    "Rock'n Roll-R&B", "Pops", "Soul-Funk", "Soundtrack", "Standard", "Techno", "World", "Others",
];
pub const BEATS: [&str; 12] = [
    "8 Beat", "16 Beat", "Other 4/4", "12/8(6/8)", "Waltz", "Waltz(Swing)", "Other 3/4", "4 Beat(Swing)", "2 Beat(March)",
    "Shuffle(Bounce)", "24 Beat", "Others",
];
pub const VOCALS: [&str; 6] = ["f1: Female Solo", "m1: Male Solo", "fm: Female & Male", "fp: Female Chorus", "mp: Male Chorus", "no: Instrumental"];
/// Lyrics / karaoke languages (the original names them in Japanese).
pub const LANGUAGES: [&str; 7] = [
    "L1: Western ( Latin 1 )", "JP: Japanese ( Shift-JIS )", "KR: Korean ( ISO-2022-KR )", "HZ: Chinese ( HZ-GB-2312 )",
    "B5: Traditional Chinese ( Big Five )", "CY: Cyrillic ( KOI8-R )", "VN: Vietnamese ( TCVN-5773:1993 )",
];

/// The code before ": " of a list entry ("DE: Germany" → "DE").
pub fn code(entry: &str) -> &str {
    entry.split(':').next().unwrap_or("").trim()
}

#[derive(Clone, Debug, PartialEq)]
pub struct XfInfo {
    // Song information (XFhd)
    pub date: String,
    pub country: String,
    pub category: String,
    pub beat: String,
    pub instrument: u8,
    pub vocal: String,
    pub composer: String,
    pub lyricist: String,
    pub arranger: String,
    pub performer: String,
    pub programmer: String,
    // Karaoke display (XFln)
    pub k_language: String,
    pub k_song: String,
    pub k_composer: String,
    pub k_lyricist: String,
    pub k_arranger: String,
    pub k_performer: String,
    pub k_programmer: String,
    // Other
    pub seq_name: String,
    pub copyright: String,
    pub time_sig: (u8, u8),
    pub tempo: u32,
    pub melody1: Option<u8>,
    pub melody2: Option<u8>,
    pub lyrics_offset: i32,
    pub lyrics_language: String,
}

impl Default for XfInfo {
    fn default() -> Self {
        XfInfo {
            date: String::new(),
            country: String::new(),
            category: String::new(),
            beat: String::new(),
            instrument: 1,
            vocal: String::new(),
            composer: String::new(),
            lyricist: String::new(),
            arranger: String::new(),
            performer: String::new(),
            programmer: String::new(),
            k_language: String::new(),
            k_song: String::new(),
            k_composer: String::new(),
            k_lyricist: String::new(),
            k_arranger: String::new(),
            k_performer: String::new(),
            k_programmer: String::new(),
            seq_name: String::new(),
            copyright: String::new(),
            time_sig: (4, 4),
            tempo: 120,
            melody1: None,
            melody2: None,
            lyrics_offset: 0,
            lyrics_language: String::new(),
        }
    }
}

fn text(d: &[u8]) -> String {
    d.iter().map(|&b| b as char).collect() // Latin-1, as XGworks writes them
}

fn bytes(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}

fn conductor(tracks: &[EditTrack]) -> usize {
    tracks.iter().position(|t| t.conductor).unwrap_or(0)
}

fn is_xf_version(e: &Event) -> bool {
    matches!(e, Event::Meta { mtype: 0x7F, data, .. } if data.starts_with(&[0x43, 0x7B, 0x00, b'X', b'F']))
}

fn starts(e: &Event, mtype: u8, prefix: &[u8]) -> bool {
    matches!(e, Event::Meta { mtype: m, data, .. } if *m == mtype && data.starts_with(prefix))
}

/// Read the XF information of a song.
pub fn read(tracks: &[EditTrack]) -> XfInfo {
    let mut x = XfInfo::default();
    if tracks.is_empty() {
        return x;
    }
    let mut ts_done = false;
    let mut tempo_done = false;
    // the conductor first, then the other tracks (MIDI files keep these anywhere)
    let order: Vec<usize> = std::iter::once(conductor(tracks)).chain((0..tracks.len()).filter(|&i| i != conductor(tracks))).collect();
    for ti in order {
        for e in &tracks[ti].others {
            let Event::Meta { t, mtype, data } = e else { continue };
            match mtype {
                0x03 if x.seq_name.is_empty() && *t == 0 => x.seq_name = text(data),
                0x02 if x.copyright.is_empty() => x.copyright = text(data),
                0x58 if !ts_done && data.len() >= 2 => {
                    x.time_sig = (data[0], 1u8 << data[1].min(5));
                    ts_done = true;
                }
                0x51 if !tempo_done && data.len() == 3 => {
                    let us = (data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32;
                    x.tempo = (60_000_000.0 / us.max(1) as f64).round() as u32;
                    tempo_done = true;
                }
                0x01 if data.starts_with(b"XFhd:") => {
                    let f: Vec<String> = text(&data[5..]).split(':').map(str::to_string).collect();
                    let g = |i: usize| f.get(i).cloned().unwrap_or_default();
                    x.date = g(0);
                    x.country = g(1);
                    x.category = g(2);
                    x.beat = g(3);
                    x.instrument = g(4).parse().unwrap_or(1);
                    x.vocal = g(5);
                    x.composer = g(6);
                    x.lyricist = g(7);
                    x.arranger = g(8);
                    x.performer = g(9);
                    x.programmer = g(10);
                }
                0x01 if data.starts_with(b"XFln:") => {
                    let f: Vec<String> = text(&data[5..]).split(':').map(str::to_string).collect();
                    let g = |i: usize| f.get(i).cloned().unwrap_or_default();
                    x.k_language = g(0);
                    x.k_song = g(1);
                    x.k_composer = g(2);
                    x.k_lyricist = g(3);
                    x.k_arranger = g(4);
                    x.k_performer = g(5);
                    x.k_programmer = g(6);
                }
                0x07 if data.starts_with(b"$Lyrc:") => {
                    let f: Vec<String> = text(&data[6..]).split(':').map(str::to_string).collect();
                    let mut ch = f.first().map(|s| s.split(',').map(|c| c.trim().parse::<u8>().ok()).collect::<Vec<_>>()).unwrap_or_default().into_iter();
                    x.melody1 = ch.next().flatten();
                    x.melody2 = ch.next().flatten();
                    x.lyrics_offset = f.get(1).and_then(|s| s.trim().parse().ok()).unwrap_or(0);
                    x.lyrics_language = f.get(2).cloned().unwrap_or_default();
                }
                _ => {}
            }
        }
    }
    x
}

/// Write the XF information into the song as XGworks does.
pub fn write(tracks: &mut [EditTrack], x: &XfInfo) {
    if tracks.is_empty() {
        return;
    }
    // the old XF events go (from every track; XGworks keeps them in the conductor)
    for t in tracks.iter_mut() {
        t.others.retain(|e| {
            !(e.t() == 0
                && (matches!(e, Event::Meta { mtype: 0x02, .. }) || is_xf_version(e) || starts(e, 0x01, b"XFhd:") || starts(e, 0x01, b"XFln:") || starts(e, 0x07, b"$Lyrc:")))
        });
    }
    let ci = conductor(tracks);
    let c = &mut tracks[ci];
    // sequence name, time signature and tempo: change the first ones (or add them)
    let mut set = |mtype: u8, data: Vec<u8>| {
        match c.others.iter_mut().find(|e| matches!(e, Event::Meta { mtype: m, .. } if *m == mtype)) {
            Some(Event::Meta { data: d, .. }) => *d = data,
            _ => c.others.insert(0, Event::Meta { t: 0, mtype, data }),
        }
    };
    set(0x03, bytes(&x.seq_name));
    set(0x58, vec![x.time_sig.0, x.time_sig.1.max(1).trailing_zeros() as u8, 24, 8]);
    let us = 60_000_000 / x.tempo.max(1);
    set(0x51, vec![(us >> 16) as u8, (us >> 8) as u8, us as u8]);
    // after the other tick-0 events
    let at = c.others.iter().position(|e| e.t() > 0).unwrap_or(c.others.len());
    let mut new = Vec::new();
    if !x.copyright.is_empty() {
        new.push(Event::Meta { t: 0, mtype: 0x02, data: bytes(&x.copyright) });
    }
    new.push(Event::Meta { t: 0, mtype: 0x7F, data: vec![0x43, 0x7B, 0x00, b'X', b'F', b'0', b'1', 0x00, 0x11] });
    let hd = format!(
        "XFhd:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{}",
        x.date, x.country, x.category, x.beat, x.instrument, x.vocal, x.composer, x.lyricist, x.arranger, x.performer, x.programmer
    );
    new.push(Event::Meta { t: 0, mtype: 0x01, data: bytes(&hd) });
    let ln = format!("XFln:{}:{}:{}:{}:{}:{}:{}", x.k_language, x.k_song, x.k_composer, x.k_lyricist, x.k_arranger, x.k_performer, x.k_programmer);
    new.push(Event::Meta { t: 0, mtype: 0x01, data: bytes(&ln) });
    if let Some(m1) = x.melody1 {
        let ch = match x.melody2 {
            Some(m2) => format!("{m1},{m2}"),
            None => m1.to_string(),
        };
        let ly = format!("$Lyrc:{ch}:{}:{}", x.lyrics_offset, x.lyrics_language);
        new.push(Event::Meta { t: 0, mtype: 0x07, data: bytes(&ly) });
    }
    c.others.splice(at..at, new);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_like_xgworks() {
        let mut t = vec![EditTrack {
            conductor: true,
            others: vec![
                Event::Meta { t: 0, mtype: 0x03, data: b"Song".to_vec() },
                Event::Meta { t: 0, mtype: 0x06, data: b"Setup".to_vec() },
                Event::Meta { t: 3840, mtype: 0x06, data: b"Intro".to_vec() },
            ],
            ..Default::default()
        }];
        let mut x = read(&t);
        x.copyright = "cTest".into();
        x.date = "1999/12/31".into();
        x.category = "Easy Listening".into();
        x.k_song = "TestSong".into();
        x.k_composer = "KComp".into();
        x.k_performer = "KPerf".into();
        x.melody1 = Some(2);
        x.melody2 = Some(3);
        x.lyrics_offset = 5;
        x.lyrics_language = "L1".into();
        write(&mut t, &x);
        // what XGworks wrote for the same input
        let texts: Vec<Vec<u8>> = t[0].others.iter().filter_map(|e| match e {
            Event::Meta { mtype: 0x01 | 0x02 | 0x07, data, .. } => Some(data.clone()),
            _ => None,
        }).collect();
        assert_eq!(texts, [
            b"cTest".to_vec(),
            b"XFhd:1999/12/31::Easy Listening::1::::::".to_vec(),
            b"XFln::TestSong:KComp:::KPerf:".to_vec(),
            b"$Lyrc:2,3:5:L1".to_vec(),
        ]);
        assert_eq!(t[0].others.last().map(|e| e.t()), Some(3840)); // after the tick-0 events
        assert_eq!(read(&t), x);
    }
}
